use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtoDecision {
    pub id: String,
    pub project_id: String,
    pub summary: String,
    pub actions_json: String,
    pub created_at: String,
}
