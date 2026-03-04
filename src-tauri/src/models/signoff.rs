use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignoffPolicy {
    pub per_piece: bool,
    pub approvers: Vec<Approver>,
    pub gates: Vec<Gate>,
}

impl Default for SignoffPolicy {
    fn default() -> Self {
        Self {
            per_piece: false,
            approvers: vec![Approver::User],
            gates: vec![Gate::DesignApproval, Gate::PreImplementation],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Approver {
    User,
    PeerAgents,
    SpecificRole(String),
    AutoAdvance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    DesignApproval,
    PreImplementation,
    PrReview,
    FinalSignoff,
}
