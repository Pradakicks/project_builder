use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub root_piece_id: Option<String>,
    pub settings: ProjectSettings,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub llm_configs: Vec<LlmConfig>,
    pub default_token_budget: i64,
    pub phase_control: PhaseControlPolicy,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            llm_configs: vec![],
            default_token_budget: 100_000,
            phase_control: PhaseControlPolicy::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhaseControlPolicy {
    Manual,
    GatedAutoAdvance,
    FullyAutonomous,
}

/// Full project export format including all pieces and connections
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub project: Project,
    pub pieces: Vec<super::piece::Piece>,
    pub connections: Vec<super::connection::Connection>,
}
