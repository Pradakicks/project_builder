use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub piece_id: String,
    pub agent_id: Option<String>,
    pub artifact_type: String,
    pub title: String,
    pub content: String,
    pub review_status: ReviewStatus,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Draft,
    InReview,
    Approved,
    Rejected,
}
