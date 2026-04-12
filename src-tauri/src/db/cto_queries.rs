use crate::models::*;
use rusqlite::params;
use serde_json::Value;
use tracing::debug;

use super::Database;

fn json_string<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn parse_review_record(
    summary: &str,
    actions_json: &str,
    review_json: Option<String>,
) -> Result<CtoDecisionReview, String> {
    if let Some(review_json) = review_json {
        if !review_json.trim().is_empty() && review_json.trim() != "{}" {
            return serde_json::from_str(&review_json).map_err(|e| e.to_string());
        }
    }

    let actions: Vec<Value> = serde_json::from_str(actions_json).unwrap_or_default();
    Ok(CtoDecisionReview {
        assistant_text: summary.to_string(),
        cleaned_content: summary.to_string(),
        actions,
        validation_errors: vec![],
    })
}

fn parse_optional_json<T: serde::de::DeserializeOwned>(
    json: Option<String>,
) -> Result<Option<T>, String> {
    match json {
        Some(json) if !json.trim().is_empty() => {
            serde_json::from_str(&json).map(Some).map_err(|e| e.to_string())
        }
        _ => Ok(None),
    }
}

fn parse_status(value: &str) -> CtoDecisionStatus {
    serde_json::from_str(&format!("\"{}\"", value)).unwrap_or(CtoDecisionStatus::Rejected)
}

fn cto_decision_from_row(
    id: String,
    project_id: String,
    summary: String,
    actions_json: String,
    review_json: Option<String>,
    execution_json: Option<String>,
    rollback_json: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
) -> Result<CtoDecision, String> {
    Ok(CtoDecision {
        id,
        project_id,
        summary: summary.clone(),
        review: parse_review_record(&summary, &actions_json, review_json)?,
        execution: parse_optional_json(execution_json)?,
        rollback: parse_optional_json(rollback_json)?,
        status: parse_status(&status),
        created_at,
        updated_at,
    })
}

impl Database {
    pub fn insert_cto_decision(
        &self,
        project_id: &str,
        decision: &CtoDecisionRecordInput,
    ) -> Result<CtoDecision, String> {
        debug!(project_id, summary = %decision.summary, "Logging CTO decision");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let actions_json = json_string(&decision.review.actions)?;
        let review_json = json_string(&decision.review)?;
        let execution_json = match &decision.execution {
            Some(execution) => Some(json_string(execution)?),
            None => None,
        };
        let status_json = json_string(&decision.status)?;

        self.conn
            .execute(
                "INSERT INTO cto_decisions (id, project_id, summary, actions_json, review_json, execution_json, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, project_id, decision.summary, actions_json, review_json, execution_json, status_json.trim_matches('"'), now, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(CtoDecision {
            id,
            project_id: project_id.to_string(),
            summary: decision.summary.clone(),
            review: decision.review.clone(),
            execution: decision.execution.clone(),
            rollback: None,
            status: decision.status.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn get_cto_decision(&self, decision_id: &str) -> Result<CtoDecision, String> {
        debug!(decision_id, "Getting CTO decision");
        self.conn
            .query_row(
                "SELECT id, project_id, summary, actions_json, review_json, execution_json, rollback_json, status, created_at, updated_at FROM cto_decisions WHERE id = ?1",
                params![decision_id],
                |row| {
                    let id: String = row.get(0)?;
                    let project_id: String = row.get(1)?;
                    let summary: String = row.get(2)?;
                    let actions_json: String = row.get(3)?;
                    let review_json: Option<String> = row.get(4)?;
                    let execution_json: Option<String> = row.get(5)?;
                    let rollback_json: Option<String> = row.get(6)?;
                    let status: String = row.get(7)?;
                    let created_at: String = row.get(8)?;
                    let updated_at: String = row.get(9)?;

                    cto_decision_from_row(
                        id,
                        project_id,
                        summary,
                        actions_json,
                        review_json,
                        execution_json,
                        rollback_json,
                        status,
                        created_at,
                        updated_at,
                    )
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))))
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn list_cto_decisions(&self, project_id: &str) -> Result<Vec<CtoDecision>, String> {
        debug!(project_id, "Listing CTO decisions");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, project_id, summary, actions_json, review_json, execution_json, rollback_json, status, created_at, updated_at FROM cto_decisions WHERE project_id = ?1 ORDER BY updated_at DESC, created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![project_id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let summary: String = row.get(2)?;
                let actions_json: String = row.get(3)?;
                let review_json: Option<String> = row.get(4)?;
                let execution_json: Option<String> = row.get(5)?;
                let rollback_json: Option<String> = row.get(6)?;
                let status: String = row.get(7)?;
                let created_at: String = row.get(8)?;
                let updated_at: String = row.get(9)?;

                cto_decision_from_row(
                    id,
                    project_id,
                    summary,
                    actions_json,
                    review_json,
                    execution_json,
                    rollback_json,
                    status,
                    created_at,
                    updated_at,
                )
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))))
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn record_cto_decision_rollback(
        &self,
        decision_id: &str,
        rollback: &CtoRollbackResult,
        status: CtoDecisionStatus,
    ) -> Result<CtoDecision, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let rollback_json = json_string(rollback)?;
        let status_json = json_string(&status)?;
        self.conn
            .execute(
                "UPDATE cto_decisions SET rollback_json = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
                params![rollback_json, status_json.trim_matches('"'), now, decision_id],
            )
            .map_err(|e| e.to_string())?;

        self.get_cto_decision(decision_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_db_path(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-cto-{case}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp test directory");
        dir.join("data.db")
    }

    fn cleanup(db_path: &Path) {
        if let Some(parent) = db_path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn cto_decision_roundtrips_and_records_rollback() {
        let db_path = temp_db_path("roundtrip");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Audit project", "Testing CTO audit rows")
            .expect("create project");

        let review = CtoDecisionReview {
            assistant_text: "Create a plan".to_string(),
            cleaned_content: "Create a plan".to_string(),
            actions: vec![json!({
                "action": "generatePlan",
                "guidance": "Build the thing"
            })],
            validation_errors: vec![],
        };
        let execution = CtoDecisionExecution {
            executed: 1,
            errors: vec![],
            steps: vec![],
            switch_to_tab: Some("plan".to_string()),
            reload_current_project: true,
            rollback: CtoRollbackPlan {
                supported: true,
                reason: None,
                steps: vec![],
            },
        };
        let decision = CtoDecisionRecordInput {
            summary: "Create a plan".to_string(),
            review: review.clone(),
            execution: Some(execution.clone()),
            status: CtoDecisionStatus::Executed,
        };

        let inserted = db
            .insert_cto_decision(&project.id, &decision)
            .expect("insert decision");
        assert_eq!(inserted.review.actions.len(), 1);
        assert_eq!(inserted.execution.as_ref().unwrap().executed, 1);
        assert_eq!(inserted.status, CtoDecisionStatus::Executed);

        let listed = db.list_cto_decisions(&project.id).expect("list decisions");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].review.cleaned_content, "Create a plan");

        let rollback = CtoRollbackResult {
            applied_at: chrono::Utc::now().to_rfc3339(),
            steps: vec![],
            errors: vec![],
        };
        let updated = db
            .record_cto_decision_rollback(&inserted.id, &rollback, CtoDecisionStatus::RolledBack)
            .expect("record rollback");
        assert_eq!(updated.status, CtoDecisionStatus::RolledBack);
        assert!(updated.rollback.is_some());

        cleanup(&db_path);
    }
}
