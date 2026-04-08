use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkPlan {
    pub id: String,
    pub project_id: String,
    pub version: i64,
    pub status: PlanStatus,
    pub summary: String,
    pub user_guidance: String,
    pub tasks: Vec<PlanTask>,
    pub raw_output: String,
    pub tokens_used: i64,
    pub integration_review: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanStatus {
    Generating,
    Draft,
    Approved,
    Rejected,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTask {
    pub id: String,
    pub piece_id: String,
    pub piece_name: String,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub suggested_phase: String,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Complete,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkPlanUpdate {
    pub status: Option<PlanStatus>,
    pub summary: Option<String>,
    pub tasks: Option<Vec<PlanTask>>,
    pub raw_output: Option<String>,
    pub tokens_used: Option<i64>,
    pub integration_review: Option<String>,
}
