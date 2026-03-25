use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub piece_type: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub responsibilities: String,
    pub interfaces: Vec<Interface>,
    pub constraints: Vec<Constraint>,
    pub notes: String,
    pub agent_prompt: String,
    pub agent_config: AgentConfig,
    pub output_mode: OutputMode,
    pub phase: Phase,
    pub position_x: f64,
    pub position_y: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub name: String,
    pub direction: InterfaceDirection,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterfaceDirection {
    In,
    Out,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    pub category: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_budget: Option<i64>,
    pub active_agents: Vec<String>,
    /// Execution engine: None or "built-in" = LLM API, "claude-code", "codex"
    pub execution_engine: Option<String>,
    /// Timeout in seconds for external tool runs (default 300)
    pub timeout: Option<u64>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            token_budget: None,
            active_agents: vec![],
            execution_engine: None,
            timeout: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    DocsOnly,
    CodeOnly,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Design,
    Review,
    Approved,
    Implementing,
}
