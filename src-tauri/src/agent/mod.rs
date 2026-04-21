pub mod capability;
pub mod external;
pub mod git_ops;
pub mod merge;
pub mod runner;
pub mod team_brief;

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
    /// Briefs produced by OTHER teams in this project. Empty when the piece
    /// has no team tag or when no other teams have briefs yet. Capped and
    /// staleness-filtered by `load_piece_context`.
    pub cross_team_briefs: Vec<CrossTeamBrief>,
}

#[derive(Debug, Clone)]
pub struct CrossTeamBrief {
    pub team: String,
    pub content: String,
    pub updated_at: String,
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

    if !context.cross_team_briefs.is_empty() {
        let mut section = String::from(
            "Cross-team context (what other teams in this project are working on). \
             Use this to stay consistent with their contracts. Flag cross-team concerns explicitly if the diff would break them.\n",
        );
        for brief in &context.cross_team_briefs {
            section.push_str(&format!(
                "\n### team: {} — updated {}\n{}\n",
                brief.team,
                brief.updated_at,
                snip_cross_team_brief(&brief.content, 1500),
            ));
        }
        system_parts.push(section);
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

/// Truncate a prior-role output blob so it fits in a prompt without blowing
/// up the token budget. Mirrors the helper in repair_prompt.rs.
fn snip_role_output(s: &str, max: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max).collect();
    format!("{kept}…[truncated {} more chars]", total - max)
}

/// Same shape, explicit name so grep finds both call sites separately.
fn snip_cross_team_brief(s: &str, max: usize) -> String {
    snip_role_output(s, max)
}

/// Outputs from earlier role(s) in the same piece run that later roles should
/// see. Empty on the first role (Implementation).
#[derive(Debug, Default, Clone)]
pub struct RolePriorOutputs {
    pub implementation_diff: Option<String>,
    pub implementation_summary: Option<String>,
    pub tests_summary: Option<String>,
    pub tests_passed: Option<bool>,
    pub tests_output_tail: Option<String>,
}

const ROLE_PRIOR_MAX_CHARS: usize = 2_000;

/// Implementation-role prompt. Today's `build_agent_prompt` exactly — this
/// thin wrapper exists so every caller goes through the role-aware API and
/// future tweaks to the impl-specific framing land in one place.
pub fn build_implementation_prompt(piece: &Piece, context: &PieceContext) -> Vec<Message> {
    build_agent_prompt(piece, context)
}

/// Testing-role prompt. Wraps the base piece prompt with a directive to write
/// tests and attaches the implementation agent's diff as context.
pub fn build_testing_prompt(
    piece: &Piece,
    context: &PieceContext,
    prior: &RolePriorOutputs,
) -> Vec<Message> {
    let mut messages = build_agent_prompt(piece, context);

    let mut addendum = String::from(
        "You are the TESTING agent for this piece. The implementation agent just finished its \
         pass against the responsibilities and interfaces above. Your job:\n\
         1. Write tests that verify the implementation matches the piece's responsibilities and \
            any interface contracts. Prefer the simplest framework already present in the repo.\n\
         2. Cover at least the happy path plus one failure or edge case implied by the \
            constraints.\n\
         3. Commit test files to the piece's branch; do NOT modify implementation code.\n\
         4. Keep output concise — you are not writing implementation, only tests.",
    );

    if let Some(diff) = prior.implementation_diff.as_deref() {
        addendum.push_str("\n\nImplementation diff:\n");
        addendum.push_str(&snip_role_output(diff, ROLE_PRIOR_MAX_CHARS));
    }
    if let Some(summary) = prior.implementation_summary.as_deref() {
        addendum.push_str("\n\nImplementation summary:\n");
        addendum.push_str(&snip_role_output(summary, ROLE_PRIOR_MAX_CHARS));
    }

    messages.push(Message {
        role: "user".to_string(),
        content: addendum,
    });
    messages
}

/// Review-role prompt. Wraps the base piece prompt with an explicit critique
/// directive, attaches the impl diff + test outcome, and pins the verdict
/// format the orchestrator parses.
pub fn build_review_prompt(
    piece: &Piece,
    context: &PieceContext,
    prior: &RolePriorOutputs,
) -> Vec<Message> {
    let mut messages = build_agent_prompt(piece, context);

    let mut addendum = String::from(
        "You are the REVIEW agent for this piece. Do NOT write code or tests. Review what the \
         implementation and testing agents produced and decide whether the piece is acceptable.\n\n\
         Review for:\n\
         (1) does the diff satisfy the responsibilities and interfaces stated above?\n\
         (2) are there obvious correctness, safety, or security issues?\n\
         (3) do the tests cover meaningful behavior, not just smoke pings?\n\n\
         End your response with a SINGLE LINE verdict in one of these exact shapes:\n\
         APPROVED\n\
         REJECTED: <one-line reason>",
    );

    if let Some(diff) = prior.implementation_diff.as_deref() {
        addendum.push_str("\n\nImplementation diff:\n");
        addendum.push_str(&snip_role_output(diff, ROLE_PRIOR_MAX_CHARS));
    }
    if let Some(summary) = prior.implementation_summary.as_deref() {
        addendum.push_str("\n\nImplementation summary:\n");
        addendum.push_str(&snip_role_output(summary, ROLE_PRIOR_MAX_CHARS));
    }
    if let Some(tests) = prior.tests_summary.as_deref() {
        let passed_note = match prior.tests_passed {
            Some(true) => " (PASSED)",
            Some(false) => " (FAILED)",
            None => "",
        };
        addendum.push_str(&format!("\n\nTests outcome{passed_note}:\n"));
        addendum.push_str(&snip_role_output(tests, ROLE_PRIOR_MAX_CHARS));
    }
    if let Some(tail) = prior.tests_output_tail.as_deref() {
        addendum.push_str("\n\nTest runner tail:\n");
        addendum.push_str(&snip_role_output(tail, ROLE_PRIOR_MAX_CHARS));
    }

    messages.push(Message {
        role: "user".to_string(),
        content: addendum,
    });
    messages
}

/// External-CLI flavour of the role prompts — same layering, flat strings.
pub fn build_role_external_prompt(
    piece: &Piece,
    context: &PieceContext,
    role: crate::models::AgentRole,
    prior: &RolePriorOutputs,
) -> (String, String) {
    let messages = match role {
        crate::models::AgentRole::Testing => build_testing_prompt(piece, context, prior),
        crate::models::AgentRole::Review => build_review_prompt(piece, context, prior),
        _ => build_implementation_prompt(piece, context),
    };
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

/// Parse the single-line verdict from a review agent's output. Lenient:
/// - case-insensitive
/// - accepts "approved", "✅"
/// - accepts "rejected: <reason>", "❌", "reject"
/// Returns (approved, reason) where reason is the trimmed text after the
/// colon for REJECTED, or empty for APPROVED / unknown.
pub fn parse_review_verdict(output: &str) -> (bool, String) {
    // Scan lines from the end — verdict is expected on the last non-empty line.
    for line in output.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("approved") || trimmed == "✅" {
            return (true, String::new());
        }
        if lower.starts_with("rejected") || lower.starts_with("reject") || trimmed == "❌" {
            let reason = trimmed
                .splitn(2, ':')
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            return (false, reason);
        }
        // Not a verdict line — keep scanning upward in case there's trailing
        // prose after the verdict.
        break;
    }
    // No clear verdict — treat as rejected with an explanatory reason so the
    // operator doesn't silently get a green light from an ambiguous output.
    (false, "review agent did not produce a clear APPROVED / REJECTED verdict".to_string())
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
mod role_prompt_tests {
    use super::*;
    use crate::models::piece::Interface as PieceInterface;
    use crate::models::{AgentConfig, Constraint, OutputMode, Phase, Piece};

    fn sample_piece() -> Piece {
        Piece {
            id: "p1".to_string(),
            project_id: "proj".to_string(),
            parent_id: None,
            name: "Server".to_string(),
            piece_type: "implementation".to_string(),
            color: None,
            icon: None,
            responsibilities: "Return JSON on GET /".to_string(),
            interfaces: vec![PieceInterface {
                name: "GET /".to_string(),
                direction: crate::models::piece::InterfaceDirection::Out,
                description: "200 ok".to_string(),
            }],
            constraints: vec![Constraint {
                category: "security".to_string(),
                description: "no secrets in logs".to_string(),
            }],
            notes: String::new(),
            agent_prompt: "build it".to_string(),
            agent_config: AgentConfig::default(),
            output_mode: OutputMode::CodeOnly,
            phase: Phase::Implementing,
            position_x: 0.0,
            position_y: 0.0,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        }
    }

    fn sample_context() -> PieceContext {
        PieceContext {
            connected_pieces: vec![],
            parent: None,
            connected_summaries: vec![],
            cross_team_briefs: vec![],
        }
    }

    #[test]
    fn implementation_prompt_is_the_legacy_prompt() {
        let piece = sample_piece();
        let ctx = sample_context();
        let legacy = build_agent_prompt(&piece, &ctx);
        let impl_messages = build_implementation_prompt(&piece, &ctx);
        assert_eq!(legacy.len(), impl_messages.len());
        for (a, b) in legacy.iter().zip(impl_messages.iter()) {
            assert_eq!(a.role, b.role);
            assert_eq!(a.content, b.content);
        }
    }

    #[test]
    fn testing_prompt_appends_directive_and_impl_diff() {
        let piece = sample_piece();
        let ctx = sample_context();
        let prior = RolePriorOutputs {
            implementation_diff: Some("server.js | 10 +++++-----".to_string()),
            implementation_summary: Some("added GET handler".to_string()),
            ..Default::default()
        };
        let messages = build_testing_prompt(&piece, &ctx, &prior);
        // At least one extra user message beyond the legacy prompt.
        let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == "user").collect();
        let testing_addendum = user_msgs
            .last()
            .expect("testing prompt has a user turn")
            .content
            .as_str();
        assert!(testing_addendum.contains("TESTING agent"));
        assert!(testing_addendum.contains("Implementation diff"));
        assert!(testing_addendum.contains("server.js | 10"));
        assert!(testing_addendum.contains("Implementation summary"));
        assert!(testing_addendum.contains("added GET handler"));
    }

    #[test]
    fn review_prompt_requires_verdict_and_includes_test_outcome() {
        let piece = sample_piece();
        let ctx = sample_context();
        let prior = RolePriorOutputs {
            implementation_summary: Some("impl body".to_string()),
            tests_summary: Some("wrote server.test.js".to_string()),
            tests_passed: Some(false),
            tests_output_tail: Some("1 failed, 0 passed".to_string()),
            ..Default::default()
        };
        let messages = build_review_prompt(&piece, &ctx, &prior);
        let last_user = messages
            .iter()
            .filter(|m| m.role == "user")
            .last()
            .expect("review user turn")
            .content
            .clone();
        assert!(last_user.contains("REVIEW agent"));
        assert!(last_user.contains("APPROVED"));
        assert!(last_user.contains("REJECTED"));
        assert!(last_user.contains("Tests outcome (FAILED)"));
        assert!(last_user.contains("1 failed, 0 passed"));
    }

    #[test]
    fn review_verdict_parser_accepts_approved_variants() {
        assert_eq!(parse_review_verdict("lgtm\nAPPROVED"), (true, String::new()));
        assert_eq!(parse_review_verdict("approved"), (true, String::new()));
        assert_eq!(parse_review_verdict("...\n✅"), (true, String::new()));
    }

    #[test]
    fn review_verdict_parser_accepts_rejected_variants() {
        let (ok, reason) = parse_review_verdict("REJECTED: tests are stubbed out");
        assert!(!ok);
        assert_eq!(reason, "tests are stubbed out");

        let (ok, _) = parse_review_verdict("rejected");
        assert!(!ok);

        let (ok, _) = parse_review_verdict("❌");
        assert!(!ok);
    }

    #[test]
    fn review_verdict_parser_treats_ambiguous_output_as_rejected() {
        let (ok, reason) = parse_review_verdict("I think it's probably fine?");
        assert!(!ok);
        assert!(reason.contains("did not produce a clear"));
    }

    #[test]
    fn cross_team_briefs_appear_in_system_prompt_when_present() {
        let piece = sample_piece();
        let ctx = PieceContext {
            connected_pieces: vec![],
            parent: None,
            connected_summaries: vec![],
            cross_team_briefs: vec![
                super::CrossTeamBrief {
                    team: "payments".to_string(),
                    content: "## Owned surface\nCharge + refund API".to_string(),
                    updated_at: "2026-04-20T01:23:45Z".to_string(),
                },
                super::CrossTeamBrief {
                    team: "auth".to_string(),
                    content: "## Owned surface\nLogin + session tokens".to_string(),
                    updated_at: "2026-04-20T00:10:00Z".to_string(),
                },
            ],
        };
        let messages = build_agent_prompt(&piece, &ctx);
        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .expect("system msg")
            .content
            .clone();
        assert!(
            system.contains("Cross-team context"),
            "system prompt must include the cross-team section"
        );
        assert!(system.contains("team: payments"));
        assert!(system.contains("Charge + refund API"));
        assert!(system.contains("team: auth"));
        assert!(system.contains("Login + session tokens"));
    }

    #[test]
    fn cross_team_briefs_section_is_omitted_when_empty() {
        let piece = sample_piece();
        let ctx = sample_context();
        let messages = build_agent_prompt(&piece, &ctx);
        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .expect("system msg")
            .content
            .clone();
        assert!(
            !system.contains("Cross-team context"),
            "no cross-team section when briefs empty"
        );
    }

    #[test]
    fn cross_team_briefs_snip_to_cap() {
        let piece = sample_piece();
        let huge = "x".repeat(5_000);
        let ctx = PieceContext {
            connected_pieces: vec![],
            parent: None,
            connected_summaries: vec![],
            cross_team_briefs: vec![super::CrossTeamBrief {
                team: "payments".to_string(),
                content: huge,
                updated_at: "2026-04-20T01:00:00Z".to_string(),
            }],
        };
        let messages = build_agent_prompt(&piece, &ctx);
        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .unwrap()
            .content
            .clone();
        assert!(system.contains("…[truncated"));
    }

    #[test]
    fn role_prior_snips_large_blobs() {
        let piece = sample_piece();
        let ctx = sample_context();
        let huge = "x".repeat(10_000);
        let prior = RolePriorOutputs {
            implementation_diff: Some(huge.clone()),
            ..Default::default()
        };
        let messages = build_testing_prompt(&piece, &ctx, &prior);
        let content = messages
            .iter()
            .filter(|m| m.role == "user")
            .last()
            .unwrap()
            .content
            .clone();
        assert!(content.contains("…[truncated"));
        assert!(content.len() < 6_000, "prompt blew past snip cap: {}", content.len());
    }
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
                acceptance_suite: None,
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
