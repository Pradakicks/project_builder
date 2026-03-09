pub mod runner;

use crate::db::Database;
use crate::llm::Message;
use crate::models::Piece;

/// Context about connected pieces for prompt building
pub struct PieceContext {
    pub connected_pieces: Vec<Piece>,
    pub parent: Option<Piece>,
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

    if let Ok(pieces) = db.list_pieces(project_id) {
        if !pieces.is_empty() {
            let piece_desc: Vec<String> = pieces
                .iter()
                .map(|p| {
                    format!(
                        "  - {} ({}, phase: {:?}): {}",
                        p.name, p.piece_type, p.phase, p.responsibilities
                    )
                })
                .collect();
            system_parts.push(format!("Pieces:\n{}", piece_desc.join("\n")));
        }
    }

    if let Ok(connections) = db.list_connections(project_id) {
        if !connections.is_empty() {
            let conn_desc: Vec<String> = connections
                .iter()
                .map(|c| format!("  - {} -> {} ({})", c.source_piece_id, c.target_piece_id, c.label))
                .collect();
            system_parts.push(format!("Connections:\n{}", conn_desc.join("\n")));
        }
    }

    vec![Message {
        role: "system".to_string(),
        content: system_parts.join("\n\n"),
    }]
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
