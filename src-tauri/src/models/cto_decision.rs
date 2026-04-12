use crate::models::{Connection, Piece, PlanStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CtoDecisionStatus {
    Rejected,
    Executed,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecisionReview {
    pub assistant_text: String,
    pub cleaned_content: String,
    pub actions: Vec<Value>,
    pub validation_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecisionExecution {
    pub executed: i64,
    pub errors: Vec<String>,
    pub steps: Vec<CtoDecisionExecutionStep>,
    pub switch_to_tab: Option<String>,
    pub reload_current_project: bool,
    pub rollback: CtoRollbackPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CtoDecisionExecutionStepStatus {
    Executed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecisionExecutionStep {
    pub index: i64,
    pub action: String,
    pub description: String,
    pub status: CtoDecisionExecutionStepStatus,
    pub error: Option<String>,
    pub rollback: Option<CtoRollbackStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoRollbackPlan {
    pub supported: bool,
    pub reason: Option<String>,
    pub steps: Vec<CtoRollbackStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoRollbackStep {
    pub index: i64,
    pub action: String,
    pub description: String,
    pub supported: bool,
    pub reason: Option<String>,
    pub kind: Option<CtoRollbackKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CtoRollbackKind {
    RestorePiece {
        piece: Piece,
    },
    DeletePiece {
        #[serde(rename = "pieceId", alias = "piece_id")]
        piece_id: String,
    },
    RestoreConnection {
        connection: Connection,
    },
    DeleteConnection {
        #[serde(rename = "connectionId", alias = "connection_id")]
        connection_id: String,
    },
    RestorePlanStatus {
        #[serde(rename = "planId", alias = "plan_id")]
        plan_id: String,
        status: PlanStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecisionRecordInput {
    pub summary: String,
    pub review: CtoDecisionReview,
    pub execution: Option<CtoDecisionExecution>,
    pub status: CtoDecisionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecision {
    pub id: String,
    pub project_id: String,
    pub summary: String,
    pub review: CtoDecisionReview,
    pub execution: Option<CtoDecisionExecution>,
    pub rollback: Option<CtoRollbackResult>,
    pub status: CtoDecisionStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoRollbackResult {
    pub applied_at: String,
    pub steps: Vec<CtoRollbackResultStep>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoRollbackResultStep {
    pub index: i64,
    pub action: String,
    pub description: String,
    pub status: CtoRollbackResultStepStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CtoRollbackResultStepStatus {
    Applied,
    Failed,
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rollback_kind_accepts_camel_case_payloads() {
        let delete_piece: CtoRollbackKind = serde_json::from_value(json!({
            "kind": "deletePiece",
            "pieceId": "piece-123",
        }))
        .expect("deserialize deletePiece");
        match delete_piece {
            CtoRollbackKind::DeletePiece { piece_id } => assert_eq!(piece_id, "piece-123"),
            other => panic!("expected deletePiece, got {other:?}"),
        }

        let delete_connection: CtoRollbackKind = serde_json::from_value(json!({
            "kind": "deleteConnection",
            "connectionId": "conn-456",
        }))
        .expect("deserialize deleteConnection");
        match delete_connection {
            CtoRollbackKind::DeleteConnection { connection_id } => {
                assert_eq!(connection_id, "conn-456")
            }
            other => panic!("expected deleteConnection, got {other:?}"),
        }

        let restore_plan_status: CtoRollbackKind = serde_json::from_value(json!({
            "kind": "restorePlanStatus",
            "planId": "plan-789",
            "status": "approved",
        }))
        .expect("deserialize restorePlanStatus");
        match restore_plan_status {
            CtoRollbackKind::RestorePlanStatus { plan_id, status } => {
                assert_eq!(plan_id, "plan-789");
                assert_eq!(status, PlanStatus::Approved);
            }
            other => panic!("expected restorePlanStatus, got {other:?}"),
        }
    }
}
