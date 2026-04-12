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
    RestorePiece { piece: Piece },
    DeletePiece { piece_id: String },
    RestoreConnection { connection: Connection },
    DeleteConnection { connection_id: String },
    RestorePlanStatus { plan_id: String, status: PlanStatus },
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
