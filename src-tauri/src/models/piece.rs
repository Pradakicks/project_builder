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
    #[serde(default)]
    pub active_agents: Vec<String>,
    /// Execution engine: None or "built-in" = LLM API, "claude-code", "codex"
    pub execution_engine: Option<String>,
    /// Timeout in seconds for external tool runs (default 300)
    pub timeout: Option<u64>,
    /// Team tag — lowercase kebab-case slug. Pieces sharing a team get
    /// cross-team briefs summarised together and consumed by pieces in
    /// OTHER teams. `None` = piece is not in a team and stays on today's
    /// single-piece context behaviour. Set via `normalize_team_name`.
    #[serde(default)]
    pub team: Option<String>,
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
            team: None,
        }
    }
}

/// Canonicalise a team name so "Payments", "payments", "  payments " all
/// resolve to the same team row. Lowercase, trim, collapse whitespace, cap at
/// 40 chars, drop anything not [a-z0-9-]. Empty input → `None` (caller treats
/// as "no team").
pub fn normalize_team_name(raw: &str) -> Option<String> {
    let lowered: String = raw.trim().to_lowercase();
    if lowered.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(lowered.len());
    let mut prev_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if ch == '-' || ch.is_whitespace() {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    // Strip a trailing dash introduced by whitespace at the end.
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        return None;
    }
    if out.chars().count() > 40 {
        out = out.chars().take(40).collect();
        while out.ends_with('-') {
            out.pop();
        }
    }
    Some(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    DocsOnly,
    CodeOnly,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Design,
    Review,
    Approved,
    Implementing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_team_name_lowercases_and_trims() {
        assert_eq!(normalize_team_name("Payments"), Some("payments".to_string()));
        assert_eq!(normalize_team_name("  auth  "), Some("auth".to_string()));
        assert_eq!(normalize_team_name("Auth Service"), Some("auth-service".to_string()));
    }

    #[test]
    fn normalize_team_name_collapses_whitespace_and_dashes() {
        assert_eq!(
            normalize_team_name("payments  and   refunds"),
            Some("payments-and-refunds".to_string())
        );
        assert_eq!(
            normalize_team_name("payments---refunds"),
            Some("payments-refunds".to_string())
        );
    }

    #[test]
    fn normalize_team_name_strips_nonalnum_dashes() {
        assert_eq!(
            normalize_team_name("payments / refunds!"),
            Some("payments-refunds".to_string())
        );
    }

    #[test]
    fn normalize_team_name_rejects_empty_or_whitespace_only() {
        assert!(normalize_team_name("").is_none());
        assert!(normalize_team_name("   ").is_none());
        assert!(normalize_team_name("///").is_none());
    }

    #[test]
    fn normalize_team_name_caps_length_without_trailing_dash() {
        let long = "a".repeat(60);
        let result = normalize_team_name(&long).expect("not empty");
        assert_eq!(result.chars().count(), 40);
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn agent_config_default_has_no_team() {
        let config = AgentConfig::default();
        assert!(config.team.is_none());
    }

    #[test]
    fn agent_config_serde_roundtrips_team_field() {
        let original = AgentConfig {
            team: Some("payments".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"team\":\"payments\""));
        let parsed: AgentConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.team, Some("payments".to_string()));
    }

    #[test]
    fn agent_config_deserializes_missing_team_as_none() {
        // Legacy rows written before Phase A must decode cleanly.
        let legacy = r#"{"provider":null,"model":null,"tokenBudget":null,"activeAgents":[],"executionEngine":null,"timeout":null}"#;
        let parsed: AgentConfig = serde_json::from_str(legacy).expect("deserialize legacy");
        assert!(parsed.team.is_none());
    }
}
