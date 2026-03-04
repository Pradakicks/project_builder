use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::piece::Constraint;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub project_id: String,
    pub source_piece_id: String,
    pub target_piece_id: String,
    pub direction: Direction,
    pub label: String,
    pub data_type: Option<String>,
    pub protocol: Option<String>,
    pub constraints: Vec<Constraint>,
    pub notes: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Unidirectional,
    Bidirectional,
}
