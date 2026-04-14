pub mod capability;
pub mod external;
pub mod git_ops;
pub mod merge;
pub mod runner;

use crate::db::Database;
use crate::llm::Message;
use crate::models::piece::Phase;
use crate::models::Piece;
use tracing::{debug, trace};

/// Context about connected pieces for prompt building
pub struct PieceContext {
    pub connected_pieces: Vec<Piece>,
    pub parent: Option<Piece>,
    /// (piece_name, summary_content) from context_summary artifacts of connected pieces
    pub connected_summaries: Vec<(String, String)>,
}

/// Build the LLM messages from a piece's configuration
pub fn build_agent_prompt(piece: &Piece, context: &PieceContext) -> Vec<Message> {
    let mut messages = Vec::new();

    debug!(
        piece = %piece.name,
        phase = ?piece.phase,
        interfaces = piece.interfaces.len(),
        constraints = piece.constraints.len(),
        connected = context.connected_pieces.len(),
        summaries = context.connected_summaries.len(),
        "Building piece agent prompt"
    );

    // Build system message from piece metadata
    let mut system_parts = Vec::new();

    system_parts.push(format!("You are an AI agent working on the piece \"{}\".", piece.name));

    if !piece.piece_type.is_empty() {
        system_parts.push(format!("Role: {}", piece.piece_type));
    }

    if !piece.responsibilities.is_empty() {
        system_parts.push(format!("Responsibilities:\n{}", piece.responsibilities));
    }

    if !piece.interfaces.is_empty() {
        let iface_desc: Vec<String> = piece
            .interfaces
            .iter()
            .map(|i| format!("  - {} ({:?}): {}", i.name, i.direction, i.description))
            .collect();
        system_parts.push(format!("Interfaces:\n{}", iface_desc.join("\n")));
    }

    if !piece.constraints.is_empty() {
        let constraint_desc: Vec<String> = piece
            .constraints
            .iter()
            .map(|c| format!("  - [{}] {}", c.category, c.description))
            .collect();
        system_parts.push(format!("Constraints:\n{}", constraint_desc.join("\n")));
    }

    if !context.connected_pieces.is_empty() {
        let connected_desc: Vec<String> = context
            .connected_pieces
            .iter()
            .map(|p| format!("  - {} ({}): {}", p.name, p.piece_type, p.responsibilities))
            .collect();
        system_parts.push(format!("Connected pieces:\n{}", connected_desc.join("\n")));
    }

    if let Some(parent) = &context.parent {
        system_parts.push(format!("Parent piece: {} ({})", parent.name, parent.piece_type));
    }

    if !context.connected_summaries.is_empty() {
        let summary_parts: Vec<String> = context
            .connected_summaries
            .iter()
            .map(|(name, summary)| format!("### {} — What it produced:\n{}", name, summary))
            .collect();
        system_parts.push(format!(
            "Context from connected pieces (use this to understand what they built):\n\n{}",
            summary_parts.join("\n\n")
        ));
    }

    system_parts.push(format!(
        "Current phase: {:?}\n{}",
        piece.phase,
        phase_instructions(&piece.phase)
    ));

    messages.push(Message {
        role: "system".to_string(),
        content: system_parts.join("\n\n"),
    });

    // User message is the agent prompt, with @references resolved
    let prompt = resolve_references(&piece.agent_prompt, &context.connected_pieces);
    if !prompt.is_empty() {
        messages.push(Message {
            role: "user".to_string(),
            content: prompt,
        });
    }

    trace!(system_prompt = %messages.iter().find(|m| m.role == "system").map(|m| m.content.as_str()).unwrap_or(""), "Piece agent system prompt");
    trace!(user_prompt = %messages.iter().find(|m| m.role == "user").map(|m| m.content.as_str()).unwrap_or(""), "Piece agent user prompt");

    messages
}

/// Build a CTO system prompt from full project context
pub fn build_cto_prompt(db: &Database, project_id: &str) -> Vec<Message> {
    debug!(project_id, "Building CTO prompt");

    let mut system_parts = vec![
        "You are the CTO of this project. You make decisions — you don't ask permission or suggest options. When something needs to change, you propose the change directly. When the architecture needs a new component, you create it. When responsibilities need updating, you update them.\n\nBe direct and assertive. State what you're doing and why, then include the action block. Your response is reviewed before execution.".to_string(),
    ];

    // Build capability snapshot early so every section below can reference it.
    let snapshot = capability::build_capability_snapshot(db, project_id);

    let mut has_working_directory = false;
    if let Ok(project) = db.get_project(project_id) {
        has_working_directory = project
            .settings
            .working_directory
            .as_ref()
            .map(|path| !path.trim().is_empty())
            .unwrap_or(false);
        system_parts.push(format!(
            "Project: {}\nDescription: {}\nAutonomy mode: {:?}",
            project.name,
            project.description,
            project.settings.autonomy_mode,
        ));
    }

    // Capability snapshot — surfaces available engines, working-dir state,
    // runtime config, verification support, and the latest goal-run failure.
    system_parts.push(capability::render_capability_section(&snapshot));

    let pieces_list = db.list_pieces(project_id).unwrap_or_default();

    if !pieces_list.is_empty() {
        let piece_desc: Vec<String> = pieces_list
            .iter()
            .map(|p| {
                format!(
                    "  - [id={}] {} ({}, phase: {:?}): {}",
                    p.id, p.name, p.piece_type, p.phase, p.responsibilities
                )
            })
            .collect();
        system_parts.push(format!("Pieces:\n{}", piece_desc.join("\n")));

        // Refactor nudge: when pieces already exist, bias the CTO toward updating them
        system_parts.push(
            "REFACTOR RULE: The project already has pieces (listed above). When the user asks \
            to modernize, upgrade, rewrite, refactor, or change the stack, you MUST inspect the \
            existing pieces and produce actions that UPDATE existing pieces (updatePiece + \
            runPiece) rather than creating parallel new ones. Only use createPiece when the goal \
            genuinely introduces a new component with NO existing counterpart. Creating a \
            duplicate of an existing piece is a bug."
                .to_string(),
        );

        // Also surface source files if the working directory has them
        if snapshot.working_directory.exists
            && !snapshot.working_directory.existing_source_files.is_empty()
        {
            system_parts.push(format!(
                "Existing source files (update these, do not recreate from scratch): {}",
                snapshot.working_directory.existing_source_files.join(", ")
            ));
        }
    } else if has_working_directory {
        system_parts.push(
            "There are no pieces yet, but the project has a working directory. Create one implementation piece with a concrete agentPrompt, outputMode, and executionEngine, then run it so code is actually written into the repo. Infer what to build from the project description and the user's message — do not ask clarifying questions."
                .to_string(),
        );
    }

    if let Ok(connections) = db.list_connections(project_id) {
        if !connections.is_empty() {
            let name_of = |id: &str| -> String {
                pieces_list
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.to_string())
            };

            let conn_desc: Vec<String> = connections
                .iter()
                .map(|c| {
                    format!(
                        "  - [id={}] {} -> {} ({})",
                        c.id,
                        name_of(&c.source_piece_id),
                        name_of(&c.target_piece_id),
                        c.label
                    )
                })
                .collect();
            system_parts.push(format!("Connections:\n{}", conn_desc.join("\n")));
        }
    }

    // Inject current work plan context if one exists
    if let Ok(Some(plan)) = db.get_latest_work_plan(project_id) {
        let task_desc: Vec<String> = plan
            .tasks
            .iter()
            .map(|t| {
                format!(
                    "  - [taskId={}] \"{}\" for {} (priority: {}, status: {:?}, phase: {})",
                    t.id, t.title, t.piece_name,
                    serde_json::to_string(&t.priority).unwrap_or_default().trim_matches('"').to_string(),
                    t.status, t.suggested_phase
                )
            })
            .collect();
        let mut plan_section = format!(
            "Current Work Plan [id={}] (v{}, status: {:?}):\n  Summary: {}",
            plan.id, plan.version, plan.status, plan.summary
        );
        if !task_desc.is_empty() {
            plan_section.push_str(&format!("\n  Tasks:\n{}", task_desc.join("\n")));
        }
        plan_section.push_str("\n\nDo not approve/reject plans that are still generating.");
        system_parts.push(plan_section);
    }

    if let Ok(goal_runs) = db.list_goal_runs(project_id) {
        if let Some(goal_run) = goal_runs.first() {
            let mut run_section = format!(
                "Latest Goal Run [id={}] (phase: {:?}, status: {:?}):\n  Prompt: {}\n  Current plan: {}\n  Runtime summary: {}\n  Verification summary: {}",
                goal_run.id,
                goal_run.phase,
                goal_run.status,
                goal_run.prompt,
                goal_run.current_plan_id.clone().unwrap_or_else(|| "none".to_string()),
                goal_run.runtime_status_summary.clone().unwrap_or_else(|| "none".to_string()),
                goal_run.verification_summary.clone().unwrap_or_else(|| "none".to_string()),
            );
            // Append richer failure context when available.
            if let Some(f) = &snapshot.latest_failure {
                run_section.push_str(&format!(
                    "\n  Failure: phase={}, status={}, retry_count={}, fingerprint={}, attention_required={}",
                    f.phase,
                    f.status,
                    f.retry_count,
                    f.fingerprint.as_deref().unwrap_or("none"),
                    f.attention_required,
                ));
                if let Some(summary) = &f.summary {
                    run_section.push_str(&format!("\n  Failure summary: {}", summary));
                }
                if let Some(blocker) = &f.blocker_reason {
                    run_section.push_str(&format!("\n  Blocker: {}", blocker));
                }
            } else if let Some(summary) = &goal_run.last_failure_summary {
                run_section.push_str(&format!("\n  Last failure: {}", summary));
            }
            system_parts.push(run_section);
        }
    }

    system_parts.push(r#"Make changes by including action blocks in your response. Your response is reviewed before anything executes.
Wrap each action in a fenced code block with the language tag "action" and put only a single JSON object inside the fence.
Do not claim the change has already been applied. Do not wrap the JSON in markdown prose.

Available actions (diagram):
- {"action": "updatePiece", "pieceId": "<id>", "updates": {...}}
  Fields: name, pieceType, phase (design|review|approved|implementing), responsibilities, notes
- {"action": "createPiece", "ref": "frontend", "name": "...", "pieceType": "...", "responsibilities": "...", "agentPrompt": "...", "outputMode": "code-only", "executionEngine": "codex", "phase": "implementing"}
  Optional fields: parentRef/parentPieceId, notes, outputMode (docs-only|code-only|both), executionEngine (built-in|claude-code|codex)
  Always include "phase": "implementing" when you plan to run the piece immediately. Pieces default to Design phase, which instructs the agent to write docs instead of code.
- {"action": "runPiece", "pieceRef": "frontend"}
  Immediately run an existing piece or one created earlier in the same response. The piece will be set to implementing phase automatically.
- {"action": "createConnection", "sourceRef": "frontend", "targetRef": "api", "label": "..."}
  Use sourceRef/targetRef for pieces created earlier in the same response.
- {"action": "createConnection", "sourcePieceId": "<existing id>", "targetPieceId": "<existing id>", "label": "..."}
  Use existing piece IDs only for pieces already listed above.
- {"action": "updateConnection", "connectionId": "<id>", "updates": {...}}
  Fields: label, notes

Available actions (work plan):
- {"action": "generatePlan", "guidance": "optional guidance text"}
  Generate a new work plan from the current diagram. Supersedes any existing draft.
- {"action": "approvePlan", "planId": "<id>"}
- {"action": "rejectPlan", "planId": "<id>"}
- {"action": "runAllTasks", "planId": "<id>"}
  Execute all pending tasks sequentially. Only works on approved plans.
- {"action": "mergeBranches", "planId": "<id>"}
  Merge all piece branches back to main. Only after all tasks are complete.

Available actions (runtime / delivery):
- {"action": "configureRuntime", "spec": {"runCommand": "...", "installCommand": "...", "appUrl": "...", "portHint": 3000, "readinessCheck": {"kind": "http"}, "stopBehavior": {"kind": "kill"}}}
  Configure how the generated project is installed, started, and verified.
- {"action": "runProject"}
  Start the configured project runtime.
- {"action": "stopProject"}
  Stop the current project runtime.
- {"action": "retryGoalStep", "goalRunId": "<id>"}
  Retry the latest blocked or failed goal run.

Rules:
- Briefly explain what you're doing, then include the action block
- When you create multiple pieces and then connect them in the same response, give each new piece a unique ref and connect them with sourceRef/targetRef. Do not invent UUIDs.
- Use piece/connection/plan/task IDs from the lists above only for entities that already exist
- Use runtime actions only when they directly help complete the current goal run
- When the user wants a real app scaffold or code written into the working directory, create a concrete implementation piece with an agentPrompt, outputMode, and executionEngine, then run it with `runPiece` or through an approved work plan — do not describe what you would do, do it
- Fenced action blocks are the primary contract; the app may recover a simple inline `action { ... }` fallback, but you should not rely on that.
- If you are proposing a `generatePlan`, include only the JSON object for the action and keep the guidance concise"#.to_string());

    trace!(prompt_length = system_parts.join("\n\n").len(), "CTO prompt built");

    vec![Message {
        role: "system".to_string(),
        content: system_parts.join("\n\n"),
    }]
}

/// Build the Leader Agent prompt with full diagram context
pub fn build_leader_prompt(db: &Database, project_id: &str, user_guidance: &str) -> Vec<Message> {
    debug!(project_id, guidance_len = user_guidance.len(), "Building leader prompt");

    let mut system_parts = vec![
        "You are the Leader Agent for this project. Your job is to analyze the full project diagram and produce a structured work plan. You have complete visibility into every piece, connection, interface, and constraint.".to_string(),
        "You must output ONLY valid JSON — no markdown fences, no explanation, no text before or after the JSON object.".to_string(),
    ];

    if let Ok(project) = db.get_project(project_id) {
        system_parts.push(format!(
            "Project: {}\nDescription: {}",
            project.name, project.description
        ));
    }

    if let Ok(pieces) = db.list_pieces(project_id) {
        if !pieces.is_empty() {
            let piece_descs: Vec<String> = pieces
                .iter()
                .map(|p| {
                    let mut desc = format!(
                        "  - [id={}] {} (type: {}, phase: {:?})\n    Responsibilities: {}",
                        p.id, p.name, p.piece_type, p.phase, p.responsibilities
                    );
                    if !p.interfaces.is_empty() {
                        let ifaces: Vec<String> = p
                            .interfaces
                            .iter()
                            .map(|i| format!("{} ({:?}): {}", i.name, i.direction, i.description))
                            .collect();
                        desc.push_str(&format!("\n    Interfaces: {}", ifaces.join("; ")));
                    }
                    if !p.constraints.is_empty() {
                        let constraints: Vec<String> = p
                            .constraints
                            .iter()
                            .map(|c| format!("[{}] {}", c.category, c.description))
                            .collect();
                        desc.push_str(&format!("\n    Constraints: {}", constraints.join("; ")));
                    }
                    if !p.notes.is_empty() {
                        desc.push_str(&format!("\n    Notes: {}", p.notes));
                    }
                    desc
                })
                .collect();
            system_parts.push(format!("Pieces:\n{}", piece_descs.join("\n")));
        }
    }

    if let Ok(connections) = db.list_connections(project_id) {
        if !connections.is_empty() {
            // Build a piece ID→name lookup
            let pieces = db.list_pieces(project_id).unwrap_or_default();
            let name_of = |id: &str| -> String {
                pieces
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.to_string())
            };

            let conn_descs: Vec<String> = connections
                .iter()
                .map(|c| {
                    let mut desc = format!(
                        "  - {} -> {} (label: {}, direction: {:?})",
                        name_of(&c.source_piece_id),
                        name_of(&c.target_piece_id),
                        c.label,
                        c.direction,
                    );
                    if !c.constraints.is_empty() {
                        let constraints: Vec<String> = c
                            .constraints
                            .iter()
                            .map(|con| format!("[{}] {}", con.category, con.description))
                            .collect();
                        desc.push_str(&format!(" | Constraints: {}", constraints.join("; ")));
                    }
                    if !c.notes.is_empty() {
                        desc.push_str(&format!(" | Notes: {}", c.notes));
                    }
                    desc
                })
                .collect();
            system_parts.push(format!("Connections:\n{}", conn_descs.join("\n")));
        }
    }

    system_parts.push(
        r#"Output the following JSON structure:
{
  "summary": "2-3 sentence overview for non-technical users",
  "tasks": [
    {
      "pieceId": "<uuid of the piece>",
      "pieceName": "Piece Name",
      "title": "Short actionable title",
      "description": "Detailed explanation of the work",
      "priority": "critical|high|medium|low",
      "suggestedPhase": "design|review|approved|implementing",
      "dependsOn": ["Other Piece Name"],
      "order": 1
    }
  ]
}

Rules:
- Write for non-technical users
- Each task targets exactly one piece (use the piece ID and name from the list above)
- Strongly prefer `implementing` as suggestedPhase — the user's goal is working code written to disk, and that only happens in the implementing phase. For pieces in `Approved` phase, always use `implementing`. For pieces in `Design` phase, use `implementing` if the piece has concrete responsibilities defined; only use `design` when the piece is a stub with no responsibilities or interfaces.
- Never suggest work for pieces already in the implementing phase unless there is a concrete problem to fix (failed validation, missing feature, bug).
- Order tasks so dependencies come first
- Output ONLY valid JSON, no markdown fences"#
            .to_string(),
    );

    let mut messages = vec![Message {
        role: "system".to_string(),
        content: system_parts.join("\n\n"),
    }];

    let user_content = if user_guidance.is_empty() {
        "Analyze the project diagram and create a work plan covering all pieces.".to_string()
    } else {
        format!(
            "Analyze the project diagram and create a work plan. User guidance: {}",
            user_guidance
        )
    };

    messages.push(Message {
        role: "user".to_string(),
        content: user_content,
    });

    trace!(system_length = messages[0].content.len(), "Leader prompt built");

    messages
}

/// Build the runtime detection agent prompt.
/// The agent is asked to output a single JSON object matching `ProjectRuntimeSpec`.
pub fn build_runtime_detection_prompt(
    project_name: &str,
    file_listing: &[String],
    file_contents: &[(String, String)],
) -> Vec<Message> {
    let schema = r#"JSON schema (use exactly these shapes — no variation):

readinessCheck must be ONE of:
  {"kind":"none"}
  {"kind":"http","path":"/","expectedStatus":200,"timeoutSeconds":90,"pollIntervalMs":500}
  {"kind":"tcpPort","timeoutSeconds":90,"pollIntervalMs":500}

stopBehavior must be ONE of:
  {"kind":"kill"}
  {"kind":"graceful","timeoutSeconds":5}

Full object shape:
{
  "installCommand": "string or null",
  "runCommand": "string (REQUIRED)",
  "verifyCommand": "string or null",
  "readinessCheck": <one of the above readinessCheck options>,
  "stopBehavior": <one of the above stopBehavior options>,
  "appUrl": "http://127.0.0.1:PORT or null",
  "portHint": <integer or null>
}"#;

    let system = format!(
        "You are a runtime detection agent. Given project files, determine how to run the project locally.\n\
Output ONLY a single JSON object matching the schema below — no markdown, no explanation, no extra text.\n\
If you cannot determine how to run this project, output exactly: null\n\
\n\
Schema:\n{schema}\n\
\n\
Rules:\n\
- runCommand MUST be present and non-empty\n\
- Use \"http\" readiness for web servers, \"tcpPort\" for non-HTTP TCP servers, \"none\" for CLI tools\n\
- timeoutSeconds should be 90+ for projects that compile (Java, Rust, TypeScript) — they take time to start\n\
- stopBehavior is \"kill\" unless the framework has graceful shutdown (Spring Boot, Rails use \"kill\" too)\n\
- installCommand only if there is a clear dependency install step\n\
\n\
Examples of correct output:\n\
Node/Vite: {{\"installCommand\":\"npm install\",\"runCommand\":\"npm run dev\",\"verifyCommand\":\"npm run build\",\"readinessCheck\":{{\"kind\":\"http\",\"path\":\"/\",\"expectedStatus\":200,\"timeoutSeconds\":90,\"pollIntervalMs\":500}},\"stopBehavior\":{{\"kind\":\"kill\"}},\"appUrl\":\"http://127.0.0.1:5173\",\"portHint\":5173}}\n\
Python/FastAPI: {{\"installCommand\":\"pip install -r requirements.txt\",\"runCommand\":\"python3 main.py\",\"verifyCommand\":null,\"readinessCheck\":{{\"kind\":\"http\",\"path\":\"/\",\"expectedStatus\":200,\"timeoutSeconds\":30,\"pollIntervalMs\":500}},\"stopBehavior\":{{\"kind\":\"kill\"}},\"appUrl\":\"http://127.0.0.1:8000\",\"portHint\":8000}}\n\
Spring Boot: {{\"installCommand\":\"mvn install -DskipTests\",\"runCommand\":\"mvn spring-boot:run\",\"verifyCommand\":\"mvn test -q\",\"readinessCheck\":{{\"kind\":\"http\",\"path\":\"/\",\"expectedStatus\":200,\"timeoutSeconds\":120,\"pollIntervalMs\":500}},\"stopBehavior\":{{\"kind\":\"kill\"}},\"appUrl\":\"http://127.0.0.1:8080\",\"portHint\":8080}}\n\
CLI tool: {{\"installCommand\":\"cargo build\",\"runCommand\":\"cargo run\",\"verifyCommand\":\"cargo check\",\"readinessCheck\":{{\"kind\":\"none\"}},\"stopBehavior\":{{\"kind\":\"kill\"}},\"appUrl\":null,\"portHint\":null}}"
    );

    let listing_text = if file_listing.is_empty() {
        "(no files found)".to_string()
    } else {
        file_listing.join("\n")
    };

    let mut user_parts = vec![
        format!("Project: {project_name}"),
        format!("## File listing\n{listing_text}"),
    ];

    for (name, content) in file_contents {
        user_parts.push(format!("## {name}\n```\n{content}\n```"));
    }

    user_parts.push("Analyze the files above and output the JSON runtime spec.".to_string());

    vec![
        Message { role: "system".to_string(), content: system },
        Message { role: "user".to_string(), content: user_parts.join("\n\n") },
    ]
}

/// Build a (system_prompt, user_prompt) pair for external tool execution.
/// Reuses the same context as `build_agent_prompt` but returns plain strings
/// instead of LLM Message structs.
pub fn build_external_prompt(piece: &Piece, context: &PieceContext) -> (String, String) {
    debug!(piece = %piece.name, "Building external prompt");
    let messages = build_agent_prompt(piece, context);
    let system = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    let user = messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    (system, user)
}

/// Phase-specific instructions injected into the agent's system prompt.
fn phase_instructions(phase: &Phase) -> String {
    match phase {
        Phase::Design => "You are in the DESIGN phase. Focus on writing clear specifications, defining interfaces, identifying constraints, and producing a design document. Do NOT write implementation code. Be explicit about: what this piece does, how it interacts with connected pieces, what APIs or events it exposes, and any constraints or tradeoffs. Your output will be captured as a design document for the rest of the team.".into(),
        Phase::Review => "You are in the REVIEW phase. Review this piece's design for completeness and consistency with connected pieces. List any problems found and suggest fixes. If the design looks good, say so explicitly.".into(),
        Phase::Approved => "This piece's design is APPROVED and ready for implementation. Break down the work into specific coding tasks, identify files to create or modify, and list acceptance criteria.".into(),
        Phase::Implementing => {
            r#"You are in the IMPLEMENTING phase. Write the actual code to implement this piece according to its design. Be thorough and complete.

After implementing, write a file named `runtime.json` to the root of the working directory describing how to run the project. Keep this file accurate — update it any time you change the entry point, port, dependencies, or start command. This is the project's live runtime record.

runtime.json format (camelCase JSON, no comments):
{
  "installCommand": "npm install",
  "runCommand": "npm run dev",
  "verifyCommand": "npm test",
  "readinessCheck": {"kind": "http", "path": "/", "expectedStatus": 200, "timeoutSeconds": 30, "pollIntervalMs": 500},
  "stopBehavior": {"kind": "kill"},
  "appUrl": "http://127.0.0.1:5173",
  "portHint": 5173
}

Use null for fields that don't apply. readinessCheck kind options: "none" (CLI tools), "http" (web apps), "tcpPort" (non-HTTP servers). Common examples:
- Static HTML: runCommand "python3 -m http.server 8080", portHint 8080, readinessCheck http on port 8080
- Flask/FastAPI: runCommand "python3 app.py", portHint 5000 or 8000, readinessCheck http
- Node/Vite: installCommand "npm install", runCommand "npm run dev", portHint 5173, readinessCheck http
- CLI tool: runCommand "./binary", readinessCheck {"kind": "none"}"#.into()
        }
    }
}

/// Returns the next phase in the workflow, or None if no auto-advance applies.
pub fn next_phase(current: &Phase) -> Option<Phase> {
    match current {
        Phase::Design => Some(Phase::Review),
        Phase::Review => Some(Phase::Approved),
        Phase::Approved => Some(Phase::Implementing),
        Phase::Implementing => None,
    }
}

/// Soft validation: returns a warning if the transition skips phases.
pub fn validate_phase_transition(from: &Phase, to: &Phase) -> Option<String> {
    match (from, to) {
        (Phase::Design, Phase::Approved) => {
            Some("Skipping Review — design hasn't been checked.".into())
        }
        (Phase::Design, Phase::Implementing) => {
            Some("Skipping Review and Approval — design hasn't been reviewed.".into())
        }
        (Phase::Review, Phase::Implementing) => {
            Some("Skipping Approval — not formally approved yet.".into())
        }
        _ => None,
    }
}

/// Replace @PieceName references with piece details
fn resolve_references(prompt: &str, pieces: &[Piece]) -> String {
    let mut result = prompt.to_string();
    for piece in pieces {
        let reference = format!("@{}", piece.name);
        if result.contains(&reference) {
            let replacement = format!(
                "[{}({}): {}]",
                piece.name, piece.piece_type, piece.responsibilities
            );
            result = result.replace(&reference, &replacement);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProjectSettings;
    use std::fs;
    use std::path::PathBuf;

    fn temp_db_path(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-cto-prompt-{case}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp test directory");
        dir.join("data.db")
    }

    fn cleanup(db_path: &PathBuf) {
        if let Some(parent) = db_path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[allow(dead_code)]
    fn make_repo_backed_project_fixture(db: &Database, case: &str) -> (crate::models::Project, crate::models::Piece, crate::models::GoalRun) {
        use crate::models::{GoalRunStatus, GoalRunUpdate, GoalRunPhase, ProjectRuntimeSpec, ProjectSettings};
        use crate::models::runtime::{RuntimeReadinessCheck, RuntimeStopBehavior};

        // Create a temp working directory that exists on disk with a .git folder
        let wd = std::env::temp_dir().join(format!("project-builder-cto-fixture-{case}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&wd).expect("create fixture working dir");
        std::fs::create_dir_all(wd.join(".git")).expect("create .git dir");
        let wd_path = wd.to_string_lossy().to_string();

        let settings = ProjectSettings {
            working_directory: Some(wd_path.clone()),
            runtime_spec: Some(ProjectRuntimeSpec {
                run_command: "npm run dev".to_string(),
                install_command: Some("npm install".to_string()),
                app_url: Some("http://127.0.0.1:5173".to_string()),
                port_hint: Some(5173),
                readiness_check: RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 30,
                    poll_interval_ms: 500,
                },
                verify_command: Some("npm test".to_string()),
                stop_behavior: RuntimeStopBehavior::Kill,
            }),
            ..ProjectSettings::default()
        };

        let project = db
            .create_project_with_settings("Fixture project", "Repo-backed CTO prompt fixture", settings)
            .expect("create fixture project");

        let piece = db
            .create_piece(&project.id, None, "Implementation", 0.0, 0.0)
            .expect("create fixture piece");

        let goal_run = db
            .create_goal_run(&project.id, "Build a todo web app")
            .expect("create fixture goal run");

        db.update_goal_run(
            &goal_run.id,
            &GoalRunUpdate {
                phase: Some(GoalRunPhase::Implementation),
                status: Some(GoalRunStatus::Failed),
                retry_count: Some(2),
                last_failure_summary: Some(Some("npm run dev exited with code 1".to_string())),
                last_failure_fingerprint: Some(Some("implementation:npm-exit-1".to_string())),
                attention_required: Some(true),
                blocker_reason: Some(Some("exit code 1".to_string())),
                ..Default::default()
            },
        )
        .expect("update fixture goal run");

        let goal_run = db.get_goal_run(&goal_run.id).expect("get updated goal run");
        (project, piece, goal_run)
    }

    #[test]
    fn cto_prompt_reflects_review_first_contract() {
        let db_path = temp_db_path("review-first");
        let db = Database::new_at_path(&db_path).expect("open temp db");
        let project = db
            .create_project_with_settings(
                "Prompt project",
                "Validate the CTO prompt",
                ProjectSettings::default(),
            )
            .expect("create project");

        let system_prompt = build_cto_prompt(&db, &project.id)
            .into_iter()
            .find(|message| message.role == "system")
            .map(|message| message.content)
            .expect("system prompt");

        assert!(system_prompt.contains("reviewed before execution"));
        assert!(system_prompt.contains("fenced code block"));
        assert!(system_prompt.contains("inline `action { ... }` fallback"));
        assert!(!system_prompt.contains("applied automatically"));

        cleanup(&db_path);
    }

    #[test]
    fn cto_prompt_biases_non_empty_repo_toward_refactor() {
        let db_path = temp_db_path("non-empty-repo-refactor");
        let db = Database::new_at_path(&db_path).expect("open temp db");

        let project = db
            .create_project("Non-empty Repo Project", "Existing project with pieces")
            .expect("create project");
        // Give project a working directory with source files (path may not exist on disk)
        let settings = ProjectSettings {
            working_directory: Some("/tmp/existing-repo".to_string()),
            ..ProjectSettings::default()
        };
        db.update_project(&project.id, None, None, None, Some(&settings))
            .expect("update settings");

        // Create an existing piece (simulating existing work)
        let piece = db
            .create_piece(&project.id, None, "Frontend", 0.0, 0.0)
            .expect("create piece");
        db.update_piece(
            &piece.id,
            &crate::db::PieceUpdate {
                responsibilities: Some("React frontend with routing".to_string()),
                ..Default::default()
            },
        )
        .expect("update piece");

        let system_prompt = build_cto_prompt(&db, &project.id)
            .into_iter()
            .find(|message| message.role == "system")
            .map(|message| message.content)
            .expect("system prompt");

        // The prompt should contain the refactor rule since pieces exist
        assert!(
            system_prompt.contains("REFACTOR RULE")
                || system_prompt.contains("UPDATE existing pieces")
                || system_prompt.contains("updatePiece"),
            "CTO prompt for non-empty repo should contain refactor guidance; got: {}",
            &system_prompt[..system_prompt.len().min(500)]
        );

        cleanup(&db_path);
    }

    #[test]
    fn cto_prompt_biases_empty_repo_toward_implementation_runs() {
        let db_path = temp_db_path("empty-repo-run");
        let db = Database::new_at_path(&db_path).expect("open temp db");
        let settings = ProjectSettings {
            working_directory: Some("/tmp/repo".to_string()),
            ..ProjectSettings::default()
        };
        let project = db
            .create_project_with_settings(
                "Prompt project",
                "Generate an app from scratch",
                settings,
            )
            .expect("create project");

        let system_prompt = build_cto_prompt(&db, &project.id)
            .into_iter()
            .find(|message| message.role == "system")
            .map(|message| message.content)
            .expect("system prompt");

        assert!(system_prompt.contains("There are no pieces yet"));
        assert!(system_prompt.contains("run it so code is actually written into the repo"));

        cleanup(&db_path);
    }

    #[test]
    fn cto_prompt_includes_capability_snapshot_for_repo_backed_project() {
        let db_path = temp_db_path("repo-backed-capability");
        let db = Database::new_at_path(&db_path).expect("open temp db");

        let (project, _piece, _goal_run) = make_repo_backed_project_fixture(&db, "cap-test");

        let system_prompt = build_cto_prompt(&db, &project.id)
            .into_iter()
            .find(|message| message.role == "system")
            .map(|message| message.content)
            .expect("system prompt");

        // Capability section is present
        assert!(system_prompt.contains("Capabilities:"), "missing Capabilities section");
        assert!(system_prompt.contains("Execution engines:"), "missing execution engines");

        // Working directory exists and is a git repo
        assert!(system_prompt.contains("exists: true"), "wd should exist");
        assert!(system_prompt.contains("git repo: true"), "wd should be a git repo");

        // Runtime is configured with the expected run command
        assert!(system_prompt.contains("npm run dev"), "missing run_command");
        assert!(system_prompt.contains("npm test"), "missing verify_command");

        // Latest goal-run failure surfaces retry_count and fingerprint
        assert!(system_prompt.contains("retry_count=2"), "missing retry_count");
        assert!(system_prompt.contains("implementation:npm-exit-1"), "missing failure fingerprint");
        assert!(system_prompt.contains("attention_required=true"), "missing attention_required");

        // built-in engine is listed
        assert!(system_prompt.contains("built-in"), "missing built-in engine entry");

        cleanup(&db_path);
        // Clean up the working directory created by the fixture
        if let Some(wd) = project.settings.working_directory.as_deref() {
            let _ = std::fs::remove_dir_all(wd);
        }
    }
}
