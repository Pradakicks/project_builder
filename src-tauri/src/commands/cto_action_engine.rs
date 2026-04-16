use crate::agent::{merge, runner};
use crate::commands::{goal_run_commands, goal_run_executor, runtime_commands};
use crate::db::{ConnectionUpdate, Database, PieceUpdate};
use crate::models::{
    AgentConfig, CtoDecisionExecution, CtoDecisionExecutionStep, CtoDecisionExecutionStepStatus,
    CtoDecisionReview, CtoRollbackKind, CtoRollbackPlan, CtoRollbackStep, GoalRunStatus,
    OutputMode, Phase, PlanStatus, ProjectRuntimeSpec, WorkPlanUpdate,
};
use crate::AppState;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

struct ActionBlockCandidate {
    start: usize,
    end: usize,
    raw: String,
}

fn is_plain_object(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

fn read_string(value: Option<&Value>, field: &str, action_index: usize) -> Result<String, String> {
    let Some(value) = value else {
        return Err(format!("Action {}: \"{field}\" is required", action_index + 1));
    };
    match value {
        Value::String(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        Value::String(_) => Err(format!("Action {}: \"{field}\" cannot be empty", action_index + 1)),
        _ => Err(format!("Action {}: \"{field}\" must be a string", action_index + 1)),
    }
}

fn read_optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
        }
        _ => Err("Optional string fields must be strings".to_string()),
    }
}

fn read_optional_object(
    value: Option<&Value>,
    field: &str,
    action_index: usize,
) -> Result<Option<Map<String, Value>>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map.clone())),
        _ => Err(format!("Action {}: \"{field}\" must be an object", action_index + 1)),
    }
}

fn ensure_allowed_keys(
    raw: &Map<String, Value>,
    allowed: &[&str],
    action_index: usize,
) -> Result<(), String> {
    let allowed: HashSet<&str> = allowed.iter().copied().collect();
    let extras: Vec<&str> = raw
        .keys()
        .map(String::as_str)
        .filter(|key| !allowed.contains(key))
        .collect();
    if extras.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Action {}: unsupported field(s): {}",
            action_index + 1,
            extras.join(", ")
        ))
    }
}

fn build_action_error(action_index: usize, action_name: &str, reason: &str) -> String {
    format!("Action {} ({}): {}", action_index + 1, action_name, reason)
}

fn extract_balanced_json_object(source: &str, open_brace_index: usize) -> Option<String> {
    if source.as_bytes().get(open_brace_index).copied() != Some(b'{') {
        return None;
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in source[open_brace_index..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = open_brace_index + offset + ch.len_utf8();
                    return Some(source[open_brace_index..end].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

pub(crate) fn collect_action_blocks(markdown: &str) -> Vec<ActionBlockCandidate> {
    let mut candidates = Vec::new();
    let mut fenced_ranges = Vec::new();
    let mut search_from = 0usize;

    while let Some(start_offset) = markdown[search_from..].find("```action") {
        let start = search_from + start_offset;
        let after_start = start + "```action".len();
        let Some(newline_offset) = markdown[after_start..].find('\n') else {
            break;
        };
        let content_start = after_start + newline_offset + 1;
        let Some(end_offset) = markdown[content_start..].find("\n```") else {
            break;
        };
        let end = content_start + end_offset + 4;
        fenced_ranges.push((start, end));
        candidates.push(ActionBlockCandidate {
            start,
            end,
            raw: markdown[content_start..content_start + end_offset].trim().to_string(),
        });
        search_from = end;
    }

    search_from = 0;
    while let Some(offset) = markdown[search_from..].find("action") {
        let start = search_from + offset;
        let brace_search = &markdown[start..];
        let Some(brace_offset) = brace_search.find('{') else {
            break;
        };
        let brace_index = start + brace_offset;
        if fenced_ranges
            .iter()
            .any(|(range_start, range_end)| brace_index >= *range_start && brace_index < *range_end)
        {
            search_from = brace_index + 1;
            continue;
        }
        if let Some(raw) = extract_balanced_json_object(markdown, brace_index) {
            candidates.push(ActionBlockCandidate {
                start,
                end: brace_index + raw.len(),
                raw,
            });
            search_from = brace_index + 1;
        } else {
            search_from = brace_index + 1;
        }
    }

    candidates.sort_by_key(|candidate| candidate.start);
    candidates
}

pub(crate) fn strip_action_blocks(markdown: &str) -> String {
    let mut cleaned = markdown.to_string();
    let mut blocks = collect_action_blocks(markdown);
    blocks.sort_by(|a, b| b.start.cmp(&a.start));
    for block in blocks {
        cleaned.replace_range(block.start..block.end, "");
    }
    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }
    cleaned.trim().to_string()
}

pub(crate) fn normalize_action(raw: Value, action_index: usize) -> Result<Value, String> {
    if !is_plain_object(&raw) {
        return Err(format!("Action {}: expected a JSON object", action_index + 1));
    }
    let raw = raw.as_object().expect("checked object");
    let action_name = read_string(raw.get("action"), "action", action_index)?;

    let result = match action_name.as_str() {
        "updatePiece" => {
            ensure_allowed_keys(raw, &["action", "pieceId", "updates"], action_index)?;
            let piece_id = read_string(raw.get("pieceId"), "pieceId", action_index)?;
            let updates = read_optional_object(raw.get("updates"), "updates", action_index)?
                .filter(|updates| !updates.is_empty())
                .ok_or_else(|| "updates must contain at least one field".to_string())?;
            json!({ "action": "updatePiece", "pieceId": piece_id, "updates": updates })
        }
        "createPiece" => {
            ensure_allowed_keys(
                raw,
                &[
                    "action",
                    "ref",
                    "name",
                    "parentRef",
                    "parentPieceId",
                    "pieceType",
                    "responsibilities",
                    "agentPrompt",
                    "notes",
                    "phase",
                    "outputMode",
                    "executionEngine",
                ],
                action_index,
            )?;
            let mut normalized = Map::new();
            normalized.insert("action".to_string(), Value::String("createPiece".to_string()));
            normalized.insert(
                "name".to_string(),
                Value::String(read_string(raw.get("name"), "name", action_index)?),
            );
            if let Some(value) = read_optional_string(raw.get("ref"))? {
                normalized.insert("ref".to_string(), Value::String(value));
            }
            if let Some(value) = read_optional_string(raw.get("parentRef"))?
                .or(read_optional_string(raw.get("parentPieceId"))?)
            {
                normalized.insert("parentRef".to_string(), Value::String(value));
            }
            for field in [
                "pieceType",
                "responsibilities",
                "agentPrompt",
                "notes",
                "phase",
                "outputMode",
                "executionEngine",
            ] {
                if let Some(value) = read_optional_string(raw.get(field))? {
                    normalized.insert(field.to_string(), Value::String(value));
                }
            }
            Value::Object(normalized)
        }
        "runPiece" => {
            ensure_allowed_keys(raw, &["action", "pieceRef", "pieceId", "feedback"], action_index)?;
            let piece_ref = read_optional_string(raw.get("pieceRef"))?
                .or(read_optional_string(raw.get("pieceId"))?)
                .ok_or_else(|| "pieceRef or pieceId is required".to_string())?;
            let mut normalized = Map::new();
            normalized.insert("action".to_string(), Value::String("runPiece".to_string()));
            normalized.insert("pieceRef".to_string(), Value::String(piece_ref));
            if let Some(value) = read_optional_string(raw.get("feedback"))? {
                normalized.insert("feedback".to_string(), Value::String(value));
            }
            Value::Object(normalized)
        }
        "createConnection" => {
            ensure_allowed_keys(
                raw,
                &["action", "sourceRef", "sourcePieceId", "targetRef", "targetPieceId", "label"],
                action_index,
            )?;
            let source_ref = read_optional_string(raw.get("sourceRef"))?
                .or(read_optional_string(raw.get("sourcePieceId"))?)
                .ok_or_else(|| "sourceRef or sourcePieceId is required".to_string())?;
            let target_ref = read_optional_string(raw.get("targetRef"))?
                .or(read_optional_string(raw.get("targetPieceId"))?)
                .ok_or_else(|| "targetRef or targetPieceId is required".to_string())?;
            let mut normalized = Map::new();
            normalized.insert("action".to_string(), Value::String("createConnection".to_string()));
            normalized.insert("sourceRef".to_string(), Value::String(source_ref));
            normalized.insert("targetRef".to_string(), Value::String(target_ref));
            if let Some(value) = read_optional_string(raw.get("label"))? {
                normalized.insert("label".to_string(), Value::String(value));
            }
            Value::Object(normalized)
        }
        "updateConnection" => {
            ensure_allowed_keys(raw, &["action", "connectionId", "updates"], action_index)?;
            let connection_id = read_string(raw.get("connectionId"), "connectionId", action_index)?;
            let updates = read_optional_object(raw.get("updates"), "updates", action_index)?
                .filter(|updates| !updates.is_empty())
                .ok_or_else(|| "updates must contain at least one field".to_string())?;
            json!({ "action": "updateConnection", "connectionId": connection_id, "updates": updates })
        }
        "generatePlan" => {
            ensure_allowed_keys(raw, &["action", "guidance"], action_index)?;
            json!({
                "action": "generatePlan",
                "guidance": read_string(raw.get("guidance"), "guidance", action_index)?,
            })
        }
        "approvePlan" | "rejectPlan" | "runAllTasks" | "mergeBranches" => {
            ensure_allowed_keys(raw, &["action", "planId"], action_index)?;
            json!({
                "action": action_name,
                "planId": read_string(raw.get("planId"), "planId", action_index)?,
            })
        }
        "configureRuntime" => {
            ensure_allowed_keys(raw, &["action", "spec"], action_index)?;
            let spec = read_optional_object(raw.get("spec"), "spec", action_index)?
                .ok_or_else(|| "spec is required".to_string())?;
            json!({ "action": "configureRuntime", "spec": spec })
        }
        "runProject" | "stopProject" => {
            ensure_allowed_keys(raw, &["action"], action_index)?;
            json!({ "action": action_name })
        }
        "retryGoalStep" => {
            ensure_allowed_keys(raw, &["action", "goalRunId"], action_index)?;
            let mut normalized = Map::new();
            normalized.insert("action".to_string(), Value::String("retryGoalStep".to_string()));
            if let Some(goal_run_id) = read_optional_string(raw.get("goalRunId"))? {
                normalized.insert("goalRunId".to_string(), Value::String(goal_run_id));
            }
            Value::Object(normalized)
        }
        _ => return Err(build_action_error(action_index, &action_name, "unsupported action")),
    };

    Ok(result)
}

pub(crate) fn review_cto_actions_impl(assistant_text: &str) -> Result<CtoDecisionReview, String> {
    let mut actions = Vec::new();
    let mut validation_errors = Vec::new();

    for (index, candidate) in collect_action_blocks(assistant_text).into_iter().enumerate() {
        match serde_json::from_str::<Value>(&candidate.raw) {
            Ok(parsed) => match normalize_action(parsed, index) {
                Ok(action) => actions.push(action),
                Err(error) => validation_errors.push(error),
            },
            Err(error) => validation_errors.push(format!(
                "Action {}: invalid JSON ({})",
                index + 1,
                error
            )),
        }
    }

    Ok(CtoDecisionReview {
        assistant_text: assistant_text.trim().to_string(),
        cleaned_content: strip_action_blocks(assistant_text),
        actions,
        validation_errors,
    })
}

async fn execute_piece_run<R: tauri::Runtime>(
    state: &AppState,
    app_handle: &AppHandle<R>,
    piece_id: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    {
        let mut running = state.running_pieces.lock().map_err(|e| e.to_string())?;
        if !running.insert(piece_id.to_string()) {
            return Err("An agent is already running for this piece. Wait for it to finish.".to_string());
        }
    }

    let result = runner::run_piece_agent(piece_id, feedback, &state.db, app_handle, None).await;

    {
        let mut running = state.running_pieces.lock().map_err(|e| e.to_string())?;
        running.remove(piece_id);
    }

    result.map(|_| ())
}

fn piece_to_update(piece: &crate::models::Piece) -> PieceUpdate {
    PieceUpdate {
        name: Some(piece.name.clone()),
        piece_type: Some(piece.piece_type.clone()),
        color: piece.color.clone(),
        icon: piece.icon.clone(),
        responsibilities: Some(piece.responsibilities.clone()),
        interfaces: Some(piece.interfaces.clone()),
        constraints: Some(piece.constraints.clone()),
        notes: Some(piece.notes.clone()),
        agent_prompt: Some(piece.agent_prompt.clone()),
        agent_config: Some(piece.agent_config.clone()),
        output_mode: Some(piece.output_mode.clone()),
        phase: Some(piece.phase.clone()),
        position_x: Some(piece.position_x),
        position_y: Some(piece.position_y),
    }
}

fn connection_to_update(connection: &crate::models::Connection) -> ConnectionUpdate {
    ConnectionUpdate {
        label: Some(connection.label.clone()),
        direction: Some(connection.direction.clone()),
        data_type: connection.data_type.clone(),
        protocol: connection.protocol.clone(),
        constraints: Some(connection.constraints.clone()),
        notes: Some(connection.notes.clone()),
        metadata: Some(connection.metadata.clone()),
    }
}

async fn resolve_piece_reference(
    db: &Mutex<Database>,
    project_id: &str,
    reference: Option<&str>,
    created_piece_refs: &HashMap<String, String>,
) -> Result<String, String> {
    let trimmed = reference.map(str::trim).filter(|value| !value.is_empty());
    let Some(trimmed) = trimmed else {
        return Err("piece reference is required".to_string());
    };

    if let Some(created_piece_id) = created_piece_refs.get(trimmed) {
        return Ok(created_piece_id.clone());
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let pieces = db.list_pieces(project_id)?;
    if pieces.iter().any(|piece| piece.id == trimmed) {
        return Ok(trimmed.to_string());
    }

    let exact_name_matches: Vec<_> = pieces.iter().filter(|piece| piece.name == trimmed).collect();
    match exact_name_matches.len() {
        0 => Err(format!("Unknown piece reference: {trimmed}")),
        1 => Ok(exact_name_matches[0].id.clone()),
        _ => Err(format!("Ambiguous piece reference: {trimmed}")),
    }
}

async fn execute_cto_actions_impl_inner<R: tauri::Runtime>(
    state: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    project_id: String,
    review: CtoDecisionReview,
) -> Result<CtoDecisionExecution, String> {
    let app_state = app_handle.state::<AppState>();
    let mut executed = 0i64;
    let mut errors = Vec::new();
    let mut steps = Vec::new();
    let mut rollback_steps = Vec::new();
    let mut created_piece_refs = HashMap::new();
    let mut switch_to_tab = None;
    let mut reload_current_project = false;
    let total_actions = review.actions.len();

    for (index, action) in review.actions.iter().enumerate() {
        let action_name = action
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let description = action_name.clone();
        let mut rollback_step: Option<CtoRollbackStep> = None;

        let _ = app_handle.emit("cto-action-step", json!({
            "projectId": &project_id,
            "step": index + 1,
            "total": total_actions,
            "action": &action_name,
            "status": "started"
        }));

        let action_result: Result<(), String> = match action_name.as_str() {
            "updatePiece" => {
                let piece_id = action["pieceId"].as_str().ok_or("pieceId is required")?;
                let previous_piece = {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.get_piece(piece_id)?
                };
                let updates: PieceUpdate = serde_json::from_value(action["updates"].clone())
                    .map_err(|e| e.to_string())?;
                {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.update_piece(piece_id, &updates)?;
                }
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: true,
                    reason: None,
                    kind: Some(CtoRollbackKind::RestorePiece {
                        piece: previous_piece,
                    }),
                });
                reload_current_project = true;
                Ok(())
            }
            "createPiece" => {
                let parent_id = resolve_piece_reference(
                    state,
                    &project_id,
                    action.get("parentRef").and_then(Value::as_str),
                    &created_piece_refs,
                )
                .await
                .ok();
                let name = action["name"].as_str().unwrap_or("New Component");
                let piece = {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.create_piece(&project_id, parent_id.as_deref(), name, 260.0, 180.0)?
                };
                let mut update = PieceUpdate::default();
                if let Some(piece_type) = action.get("pieceType").and_then(Value::as_str) {
                    update.piece_type = Some(piece_type.to_string());
                }
                if let Some(resp) = action.get("responsibilities").and_then(Value::as_str) {
                    update.responsibilities = Some(resp.to_string());
                }
                if let Some(prompt) = action.get("agentPrompt").and_then(Value::as_str) {
                    update.agent_prompt = Some(prompt.to_string());
                }
                if let Some(notes) = action.get("notes").and_then(Value::as_str) {
                    update.notes = Some(notes.to_string());
                }
                if let Some(phase) = action.get("phase").and_then(Value::as_str) {
                    update.phase = Some(serde_json::from_value(Value::String(phase.to_string())).map_err(|e| e.to_string())?);
                }
                if let Some(output_mode) = action.get("outputMode").and_then(Value::as_str) {
                    update.output_mode = Some(serde_json::from_value(Value::String(output_mode.to_string())).map_err(|e| e.to_string())?);
                }
                if let Some(engine) = action.get("executionEngine").and_then(Value::as_str) {
                    update.agent_config = Some(AgentConfig {
                        execution_engine: Some(engine.to_string()),
                        ..Default::default()
                    });
                }
                let has_initial_updates = update.name.is_some()
                    || update.piece_type.is_some()
                    || update.color.is_some()
                    || update.icon.is_some()
                    || update.responsibilities.is_some()
                    || update.interfaces.is_some()
                    || update.constraints.is_some()
                    || update.notes.is_some()
                    || update.agent_prompt.is_some()
                    || update.agent_config.is_some()
                    || update.output_mode.is_some()
                    || update.phase.is_some()
                    || update.position_x.is_some()
                    || update.position_y.is_some();
                if has_initial_updates {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.update_piece(&piece.id, &update)?;
                }
                if let Some(reference) = action.get("ref").and_then(Value::as_str) {
                    created_piece_refs.insert(reference.trim().to_string(), piece.id.clone());
                }
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: true,
                    reason: None,
                    kind: Some(CtoRollbackKind::DeletePiece {
                        piece_id: piece.id.clone(),
                    }),
                });
                reload_current_project = true;
                Ok(())
            }
            "runPiece" => {
                let piece_id = resolve_piece_reference(
                    state,
                    &project_id,
                    action.get("pieceRef").and_then(Value::as_str),
                    &created_piece_refs,
                )
                .await?;
                {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.update_piece(
                        &piece_id,
                        &PieceUpdate {
                            phase: Some(Phase::Implementing),
                            ..Default::default()
                        },
                    )?;
                }
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Piece execution changes workspace state and is not rollback-safe".to_string()),
                    kind: None,
                });
                execute_piece_run(
                    &app_state,
                    app_handle,
                    &piece_id,
                    action.get("feedback").and_then(Value::as_str),
                )
                .await?;
                reload_current_project = true;
                Ok(())
            }
            "createConnection" => {
                let source_piece_id = resolve_piece_reference(
                    state,
                    &project_id,
                    action.get("sourceRef").and_then(Value::as_str),
                    &created_piece_refs,
                )
                .await?;
                let target_piece_id = resolve_piece_reference(
                    state,
                    &project_id,
                    action.get("targetRef").and_then(Value::as_str),
                    &created_piece_refs,
                )
                .await?;
                let connection = {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.create_connection(
                        &project_id,
                        &source_piece_id,
                        &target_piece_id,
                        action.get("label").and_then(Value::as_str).unwrap_or(""),
                    )?
                };
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: true,
                    reason: None,
                    kind: Some(CtoRollbackKind::DeleteConnection {
                        connection_id: connection.id,
                    }),
                });
                reload_current_project = true;
                Ok(())
            }
            "updateConnection" => {
                let connection_id = action["connectionId"].as_str().ok_or("connectionId is required")?;
                let previous_connection = {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.get_connection(connection_id)?
                };
                let updates: ConnectionUpdate =
                    serde_json::from_value(action["updates"].clone()).map_err(|e| e.to_string())?;
                {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.update_connection(connection_id, &updates)?;
                }
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: true,
                    reason: None,
                    kind: Some(CtoRollbackKind::RestoreConnection {
                        connection: previous_connection,
                    }),
                });
                reload_current_project = true;
                Ok(())
            }
            "generatePlan" => {
                let guidance = action["guidance"].as_str().unwrap_or_default();
                runner::run_leader_agent(&project_id, guidance, state, app_handle).await?;
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Generated plans are not rollback-safe yet".to_string()),
                    kind: None,
                });
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "approvePlan" | "rejectPlan" => {
                let plan_id = action["planId"].as_str().ok_or("planId is required")?;
                let previous_plan = {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.get_work_plan(plan_id)?
                };
                let next_status = if action_name == "approvePlan" {
                    PlanStatus::Approved
                } else {
                    PlanStatus::Rejected
                };
                {
                    let db = state.lock().map_err(|e| e.to_string())?;
                    db.update_work_plan(
                        plan_id,
                        &WorkPlanUpdate {
                            status: Some(next_status),
                            ..Default::default()
                        },
                    )?;
                }
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: true,
                    reason: None,
                    kind: Some(CtoRollbackKind::RestorePlanStatus {
                        plan_id: previous_plan.id,
                        status: previous_plan.status,
                    }),
                });
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "runAllTasks" => {
                let plan_id = action["planId"].as_str().ok_or("planId is required")?;
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Task execution changes workspace state and is not rollback-safe".to_string()),
                    kind: None,
                });
                runner::run_all_plan_tasks(plan_id, None, state, &app_state.running_pieces, app_handle, None).await?;
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "mergeBranches" => {
                let plan_id = action["planId"].as_str().ok_or("planId is required")?;
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Git merges are not rollback-safe from the audit log".to_string()),
                    kind: None,
                });
                let summary = merge::merge_plan_branches(plan_id, state, app_handle).await?;
                if summary.conflict.is_none() {
                    let _ = merge::run_integration_review(plan_id, state, app_handle).await?;
                }
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "configureRuntime" => {
                let spec: ProjectRuntimeSpec = serde_json::from_value(action["spec"].clone())
                    .map_err(|e| e.to_string())?;
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Runtime configuration rollback is not implemented".to_string()),
                    kind: None,
                });
                runtime_commands::configure_runtime_impl(
                    state,
                    &app_state.runtime_sessions,
                    project_id.clone(),
                    spec,
                )
                .await?;
                Ok(())
            }
            "runProject" => {
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Runtime process control is not rollback-safe".to_string()),
                    kind: None,
                });
                runtime_commands::start_runtime_impl(
                    state,
                    &app_state.runtime_sessions,
                    project_id.clone(),
                )
                .await?;
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "stopProject" => {
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Runtime stop is not rollback-safe".to_string()),
                    kind: None,
                });
                runtime_commands::stop_runtime_impl(
                    state,
                    &app_state.runtime_sessions,
                    project_id.clone(),
                )
                .await?;
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            "retryGoalStep" => {
                let goal_run_id = action
                    .get("goalRunId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        state
                            .lock()
                            .ok()
                            .and_then(|db| db.list_goal_runs(&project_id).ok())
                            .and_then(|runs| runs.into_iter().next().map(|run| run.id))
                    })
                    .ok_or("No goal run is available to retry")?;
                rollback_step = Some(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some("Goal-run retries are not rollback-safe".to_string()),
                    kind: None,
                });
                goal_run_commands::update_goal_run_impl(
                    state,
                    goal_run_id.clone(),
                    crate::models::GoalRunUpdate {
                        status: Some(GoalRunStatus::Running),
                        stop_requested: Some(false),
                        blocker_reason: Some(None),
                        ..Default::default()
                    },
                )?;
                goal_run_executor::spawn_goal_run_executor(app_handle.clone(), goal_run_id);
                switch_to_tab = Some("plan".to_string());
                Ok(())
            }
            _ => Err(format!("Unknown action: {action_name}")),
        };

        match action_result {
            Ok(()) => {
                executed += 1;
                let rollback = rollback_step.clone();
                if let Some(rollback_step) = rollback_step {
                    rollback_steps.push(rollback_step);
                }
                steps.push(CtoDecisionExecutionStep {
                    index: index as i64,
                    action: action_name,
                    description,
                    status: CtoDecisionExecutionStepStatus::Executed,
                    error: None,
                    rollback,
                });
                let _ = app_handle.emit("cto-action-step", json!({
                    "projectId": &project_id,
                    "step": index + 1,
                    "total": total_actions,
                    "action": &steps.last().unwrap().action,
                    "status": "completed"
                }));
            }
            Err(error) => {
                let message = format!("{action_name} failed: {error}");
                errors.push(message.clone());
                let fallback_rollback = rollback_step.clone().unwrap_or(CtoRollbackStep {
                    index: index as i64,
                    action: action_name.clone(),
                    description: description.clone(),
                    supported: false,
                    reason: Some(message.clone()),
                    kind: None,
                });
                rollback_steps.push(fallback_rollback.clone());
                steps.push(CtoDecisionExecutionStep {
                    index: index as i64,
                    action: action_name,
                    description,
                    status: CtoDecisionExecutionStepStatus::Failed,
                    error: Some(message),
                    rollback: Some(fallback_rollback),
                });
                let _ = app_handle.emit("cto-action-step", json!({
                    "projectId": &project_id,
                    "step": index + 1,
                    "total": total_actions,
                    "action": &steps.last().unwrap().action,
                    "status": "failed"
                }));
            }
        }
    }

    let rollback_supported = errors.is_empty() && rollback_steps.iter().all(|step| step.supported);
    let rollback_reason = if rollback_supported {
        None
    } else if errors.is_empty() {
        Some("This decision includes non-reversible action(s).".to_string())
    } else {
        Some("One or more CTO actions failed during execution.".to_string())
    };

    Ok(CtoDecisionExecution {
        executed,
        errors,
        steps,
        switch_to_tab,
        reload_current_project,
        rollback: CtoRollbackPlan {
            supported: rollback_supported,
            reason: rollback_reason,
            steps: rollback_steps,
        },
    })
}

pub(crate) async fn execute_cto_actions_impl<R: tauri::Runtime>(
    state: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    project_id: String,
    review: CtoDecisionReview,
) -> Result<CtoDecisionExecution, String> {
    execute_cto_actions_impl_inner(state, app_handle, project_id, review).await
}

#[tracing::instrument(skip(_state))]
#[tauri::command]
pub fn review_cto_actions(
    _state: State<'_, AppState>,
    assistant_text: String,
) -> Result<CtoDecisionReview, String> {
    review_cto_actions_impl(&assistant_text)
}

#[tracing::instrument(skip(state, app_handle, review))]
#[tauri::command]
pub async fn execute_cto_actions(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    review: CtoDecisionReview,
    execution_mode: Option<String>,
) -> Result<CtoDecisionExecution, String> {
    let _ = execution_mode;
    execute_cto_actions_impl(&state.db, &app_handle, project_id, review).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_rejects_malformed_action_json() {
        let review = review_cto_actions_impl(
            "Build app\n```action\n`{\"action\":\"generatePlan\",\"guidance\":\"Build app\"}\n```",
        )
        .expect("review");
        assert!(review.actions.is_empty());
        assert_eq!(review.validation_errors.len(), 1);
        assert!(review.validation_errors[0].contains("invalid JSON"));
    }

    #[test]
    fn review_normalizes_create_and_run_actions() {
        let review = review_cto_actions_impl(
            "```action\n{\"action\":\"createPiece\",\"ref\":\"frontend\",\"name\":\"Todo App\",\"agentPrompt\":\"Create app\",\"outputMode\":\"code-only\",\"executionEngine\":\"codex\"}\n```\n```action\n{\"action\":\"runPiece\",\"pieceRef\":\"frontend\"}\n```",
        )
        .expect("review");
        assert_eq!(review.validation_errors.len(), 0);
        assert_eq!(review.actions.len(), 2);
        assert_eq!(review.actions[0]["action"], "createPiece");
        assert_eq!(review.actions[1]["action"], "runPiece");
        assert_eq!(review.cleaned_content, "");
    }
}
