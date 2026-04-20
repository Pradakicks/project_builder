use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Leader,
    Implementation,
    Testing,
    Review,
    Custom,
}

/// The three roles that run against a piece. Ordered — this is the execution
/// order when `active_agents` is the default full set.
pub const PIECE_AGENT_ROLES: [AgentRole; 3] = [
    AgentRole::Implementation,
    AgentRole::Testing,
    AgentRole::Review,
];

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Leader => "leader",
            AgentRole::Implementation => "implementation",
            AgentRole::Testing => "testing",
            AgentRole::Review => "review",
            AgentRole::Custom => "custom",
        }
    }
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "leader" => Ok(AgentRole::Leader),
            "implementation" => Ok(AgentRole::Implementation),
            "testing" => Ok(AgentRole::Testing),
            "review" => Ok(AgentRole::Review),
            "custom" => Ok(AgentRole::Custom),
            other => Err(format!("unknown agent role: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    Idle,
    Working,
    WaitingForApproval,
    Blocked,
    Error,
}

impl AgentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::WaitingForApproval => "waiting-for-approval",
            AgentState::Blocked => "blocked",
            AgentState::Error => "error",
        }
    }
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "idle" => Ok(AgentState::Idle),
            "working" => Ok(AgentState::Working),
            "waiting-for-approval" | "waitingforapproval" => Ok(AgentState::WaitingForApproval),
            "blocked" => Ok(AgentState::Blocked),
            "error" => Ok(AgentState::Error),
            other => Err(format!("unknown agent state: {other}")),
        }
    }
}

/// Row from the `agents` table — one per (piece_id, role) pair. Exposed to the
/// frontend so the Agents panel and canvas node can show per-role state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    pub id: String,
    pub piece_id: String,
    pub role: AgentRole,
    pub state: AgentState,
    pub token_budget: i64,
    pub token_usage: i64,
    pub provider: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Claude,
    OpenAI,
    Local,
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_role_roundtrips_through_as_str_and_from_str() {
        for role in [
            AgentRole::Leader,
            AgentRole::Implementation,
            AgentRole::Testing,
            AgentRole::Review,
            AgentRole::Custom,
        ] {
            let s = role.as_str();
            let parsed: AgentRole = s.parse().expect("parse");
            assert_eq!(
                role as u8, parsed as u8,
                "role {s} must roundtrip through FromStr"
            );
        }
    }

    #[test]
    fn agent_role_serde_matches_ts_strings() {
        // The TS union is lowercase; serde must emit matching values.
        let json = serde_json::to_string(&AgentRole::Implementation).unwrap();
        assert_eq!(json, "\"implementation\"");
        let parsed: AgentRole = serde_json::from_str("\"review\"").unwrap();
        assert!(matches!(parsed, AgentRole::Review));
    }

    #[test]
    fn agent_state_roundtrips() {
        for state in [
            AgentState::Idle,
            AgentState::Working,
            AgentState::WaitingForApproval,
            AgentState::Blocked,
            AgentState::Error,
        ] {
            let s = state.as_str();
            let parsed: AgentState = s.parse().expect("parse");
            assert_eq!(state as u8, parsed as u8);
        }
    }

    #[test]
    fn piece_agent_roles_in_execution_order() {
        assert_eq!(PIECE_AGENT_ROLES[0], AgentRole::Implementation);
        assert_eq!(PIECE_AGENT_ROLES[1], AgentRole::Testing);
        assert_eq!(PIECE_AGENT_ROLES[2], AgentRole::Review);
    }
}
