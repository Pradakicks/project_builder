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
    /// How to handle merge conflicts when combining piece branches
    pub conflict_resolution: ConflictResolutionPolicy,
    /// Path to a git repository for external tool execution
    pub working_directory: Option<String>,
    /// Default execution engine for new pieces ("built-in", "claude-code", "codex")
    pub default_execution_engine: Option<String>,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            llm_configs: vec![],
            default_token_budget: 100_000,
            phase_control: PhaseControlPolicy::Manual,
            conflict_resolution: ConflictResolutionPolicy::AiAssisted,
            working_directory: None,
            default_execution_engine: None,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictResolutionPolicy {
    /// Flag conflict, user resolves externally
    Manual,
    /// Flag conflict, offer "Resolve with AI" button (default)
    AiAssisted,
    /// AI silently resolves conflicts without user approval
    AutoResolve,
}

/// Full project export format including all pieces and connections
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub project: Project,
    pub pieces: Vec<super::piece::Piece>,
    pub connections: Vec<super::connection::Connection>,
}
