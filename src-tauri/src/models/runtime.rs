use serde::{Deserialize, Serialize};

fn default_http_path() -> String {
    "/".to_string()
}

fn default_http_expected_status() -> u16 {
    200
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_poll_interval_ms() -> u64 {
    500
}

fn default_graceful_timeout_seconds() -> u64 {
    5
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeSessionStatus {
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeReadinessCheck {
    None,
    Http {
        #[serde(default = "default_http_path")]
        path: String,
        #[serde(rename = "expectedStatus", alias = "expected_status")]
        #[serde(default = "default_http_expected_status")]
        expected_status: u16,
        #[serde(rename = "timeoutSeconds", alias = "timeout_seconds")]
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
        #[serde(rename = "pollIntervalMs", alias = "poll_interval_ms")]
        #[serde(default = "default_poll_interval_ms")]
        poll_interval_ms: u64,
    },
    TcpPort {
        #[serde(rename = "timeoutSeconds", alias = "timeout_seconds")]
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
        #[serde(rename = "pollIntervalMs", alias = "poll_interval_ms")]
        #[serde(default = "default_poll_interval_ms")]
        poll_interval_ms: u64,
    },
}

impl Default for RuntimeReadinessCheck {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeStopBehavior {
    Kill,
    Graceful {
        #[serde(rename = "timeoutSeconds", alias = "timeout_seconds")]
        #[serde(default = "default_graceful_timeout_seconds")]
        timeout_seconds: u64,
    },
}

impl Default for RuntimeStopBehavior {
    fn default() -> Self {
        Self::Kill
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeSpec {
    pub install_command: Option<String>,
    pub run_command: String,
    #[serde(default)]
    pub readiness_check: RuntimeReadinessCheck,
    pub verify_command: Option<String>,
    #[serde(default)]
    pub stop_behavior: RuntimeStopBehavior,
    pub app_url: Option<String>,
    pub port_hint: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeSession {
    pub session_id: String,
    pub status: RuntimeSessionStatus,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub ended_at: Option<String>,
    pub url: Option<String>,
    pub port_hint: Option<u16>,
    pub log_path: Option<String>,
    pub recent_logs: Vec<String>,
    pub last_error: Option<String>,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeStatus {
    pub project_id: String,
    pub spec: Option<ProjectRuntimeSpec>,
    pub session: Option<ProjectRuntimeSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLogTail {
    pub path: Option<String>,
    pub lines: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn runtime_spec_accepts_camel_case_payloads() {
        let spec: ProjectRuntimeSpec = serde_json::from_value(json!({
            "installCommand": "npm install",
            "runCommand": "npm run dev",
            "readinessCheck": {
                "kind": "http",
                "path": "/",
                "expectedStatus": 200,
                "timeoutSeconds": 5,
                "pollIntervalMs": 250
            },
            "verifyCommand": "npm test",
            "stopBehavior": { "kind": "graceful", "timeoutSeconds": 7 },
            "appUrl": "http://127.0.0.1:3000",
            "portHint": 3000
        }))
        .expect("deserialize runtime spec");

        assert_eq!(spec.install_command.as_deref(), Some("npm install"));
        assert_eq!(spec.run_command, "npm run dev");
        assert_eq!(spec.verify_command.as_deref(), Some("npm test"));
        assert_eq!(spec.app_url.as_deref(), Some("http://127.0.0.1:3000"));
        assert_eq!(spec.port_hint, Some(3000));
        match spec.readiness_check {
            RuntimeReadinessCheck::Http {
                path,
                expected_status,
                timeout_seconds,
                poll_interval_ms,
            } => {
                assert_eq!(path, "/");
                assert_eq!(expected_status, 200);
                assert_eq!(timeout_seconds, 5);
                assert_eq!(poll_interval_ms, 250);
            }
            other => panic!("expected http readiness check, got {other:?}"),
        }
        match spec.stop_behavior {
            RuntimeStopBehavior::Graceful { timeout_seconds } => {
                assert_eq!(timeout_seconds, 7);
            }
            other => panic!("expected graceful stop behavior, got {other:?}"),
        }
    }
}
