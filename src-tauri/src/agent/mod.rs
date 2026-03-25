pub mod external;
pub mod git_ops;
pub mod runner;

use crate::db::Database;
use crate::llm::Message;
use crate::models::piece::Phase;
use crate::models::Piece;

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

    messages
}

/// Build a CTO system prompt from full project context
pub fn build_cto_prompt(db: &Database, project_id: &str) -> Vec<Message> {
    let mut system_parts = vec![
        "You are the CTO Agent for this project. You have full visibility into the project's architecture and can advise on design decisions, implementation strategy, and technical tradeoffs.".to_string(),
    ];

    if let Ok(project) = db.get_project(project_id) {
        system_parts.push(format!("Project: {}\nDescription: {}", project.name, project.description));
    }

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

    system_parts.push(r#"You can make changes to the project by including action blocks in your response.
Wrap each action in a fenced code block with the language tag "action".

Available actions:
- {"action": "updatePiece", "pieceId": "<id>", "updates": {...}}
  Fields: name, pieceType, phase (design|review|approved|implementing), responsibilities, notes
- {"action": "createPiece", "name": "...", "pieceType": "...", "responsibilities": "..."}
- {"action": "createConnection", "sourcePieceId": "<id>", "targetPieceId": "<id>", "label": "..."}
- {"action": "updateConnection", "connectionId": "<id>", "updates": {...}}
  Fields: label, notes

Rules:
- Explain what you're doing BEFORE each action block
- Use piece/connection IDs from the list above
- You may include zero or more action blocks per response"#.to_string());

    vec![Message {
        role: "system".to_string(),
        content: system_parts.join("\n\n"),
    }]
}

/// Build the Leader Agent prompt with full diagram context
pub fn build_leader_prompt(db: &Database, project_id: &str, user_guidance: &str) -> Vec<Message> {
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
- Consider the current phase of each piece — don't suggest work for pieces already implementing unless there's a problem
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

    messages
}

/// Build a (system_prompt, user_prompt) pair for external tool execution.
/// Reuses the same context as `build_agent_prompt` but returns plain strings
/// instead of LLM Message structs.
pub fn build_external_prompt(piece: &Piece, context: &PieceContext) -> (String, String) {
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
        Phase::Design => "You are in the DESIGN phase. Focus on writing clear specifications, defining interfaces, identifying constraints, and producing a design document. Do NOT write implementation code.".into(),
        Phase::Review => "You are in the REVIEW phase. Review this piece's design for completeness and consistency with connected pieces. List any problems found and suggest fixes. If the design looks good, say so explicitly.".into(),
        Phase::Approved => "This piece's design is APPROVED and ready for implementation. Break down the work into specific coding tasks, identify files to create or modify, and list acceptance criteria.".into(),
        Phase::Implementing => "You are in the IMPLEMENTING phase. Write the actual code to implement this piece according to its design. Be thorough and complete.".into(),
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
