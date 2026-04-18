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

fn default_acceptance_http_timeout_seconds() -> u64 {
    10
}

fn default_acceptance_shell_timeout_seconds() -> u64 {
    120
}

fn default_acceptance_http_min_status() -> u16 {
    200
}

fn default_acceptance_http_max_status() -> u16 {
    399
}

fn default_log_scan_last_n_lines() -> usize {
    200
}

fn default_log_scan_mode() -> LogScanMode {
    LogScanMode::MustNotMatch
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogScanMode {
    MustMatch,
    MustNotMatch,
}

/// One check in the post-start acceptance suite. Independent of readiness —
/// readiness gates "runtime is alive", acceptance gates "runtime is behaving".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AcceptanceCheck {
    HttpProbe {
        name: String,
        #[serde(default = "default_http_path")]
        path: String,
        #[serde(default = "default_acceptance_http_min_status")]
        expected_status_min: u16,
        #[serde(default = "default_acceptance_http_max_status")]
        expected_status_max: u16,
        #[serde(default)]
        expected_body_contains: Option<String>,
        #[serde(default)]
        expected_content_type: Option<String>,
        #[serde(default = "default_acceptance_http_timeout_seconds")]
        timeout_seconds: u64,
    },
    Shell {
        name: String,
        command: String,
        #[serde(default = "default_acceptance_shell_timeout_seconds")]
        timeout_seconds: u64,
    },
    LogScan {
        name: String,
        /// Regex source strings; invalid regexes are surfaced as a failed check.
        patterns: Vec<String>,
        #[serde(default = "default_log_scan_mode")]
        mode: LogScanMode,
        #[serde(default = "default_log_scan_last_n_lines")]
        last_n_lines: usize,
    },
    TcpPort {
        name: String,
        port: u16,
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceSuite {
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
    /// If true, the driver stops on the first failing check. Default false —
    /// running the whole suite gives the repair agent a richer signal to act on.
    #[serde(default)]
    pub stop_on_first_failure: bool,
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
    /// Optional post-start acceptance suite. When absent, `verify_runtime_impl`
    /// derives a sensible default from `verify_command` + `app_url`/`port_hint`
    /// plus a panic-pattern log scan.
    #[serde(default)]
    pub acceptance_suite: Option<AcceptanceSuite>,
}

impl Default for ProjectRuntimeSpec {
    fn default() -> Self {
        Self {
            install_command: None,
            run_command: String::new(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::default(),
            app_url: None,
            port_hint: None,
            acceptance_suite: None,
        }
    }
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
    fn acceptance_suite_roundtrips_all_variants() {
        let raw = json!({
            "stopOnFirstFailure": false,
            "checks": [
                { "kind": "httpProbe", "name": "root ok", "path": "/", "expectedStatusMin": 200, "expectedStatusMax": 399 },
                { "kind": "shell", "name": "tests", "command": "npm test" },
                { "kind": "logScan", "name": "panic", "patterns": ["panic!?", "FATAL"], "mode": "mustNotMatch", "lastNLines": 200 },
                { "kind": "tcpPort", "name": "db port", "port": 5432 }
            ]
        });
        let suite: AcceptanceSuite = serde_json::from_value(raw).expect("deserialize suite");
        assert_eq!(suite.checks.len(), 4);
        match &suite.checks[0] {
            AcceptanceCheck::HttpProbe { name, path, expected_status_min, expected_status_max, .. } => {
                assert_eq!(name, "root ok");
                assert_eq!(path, "/");
                assert_eq!(*expected_status_min, 200);
                assert_eq!(*expected_status_max, 399);
            }
            other => panic!("expected HttpProbe, got {other:?}"),
        }
        match &suite.checks[2] {
            AcceptanceCheck::LogScan { mode, patterns, .. } => {
                assert_eq!(*mode, LogScanMode::MustNotMatch);
                assert_eq!(patterns.len(), 2);
            }
            other => panic!("expected LogScan, got {other:?}"),
        }
    }

    #[test]
    fn acceptance_suite_fills_defaults_when_fields_omitted() {
        let raw = json!({
            "checks": [
                { "kind": "httpProbe", "name": "root", "path": "/" },
                { "kind": "logScan", "name": "fatals", "patterns": ["FATAL"] }
            ]
        });
        let suite: AcceptanceSuite = serde_json::from_value(raw).expect("deserialize");
        match &suite.checks[0] {
            AcceptanceCheck::HttpProbe { expected_status_min, expected_status_max, timeout_seconds, .. } => {
                assert_eq!(*expected_status_min, 200);
                assert_eq!(*expected_status_max, 399);
                assert_eq!(*timeout_seconds, 10);
            }
            _ => panic!("expected HttpProbe"),
        }
        match &suite.checks[1] {
            AcceptanceCheck::LogScan { mode, last_n_lines, .. } => {
                assert_eq!(*mode, LogScanMode::MustNotMatch);
                assert_eq!(*last_n_lines, 200);
            }
            _ => panic!("expected LogScan"),
        }
        assert!(!suite.stop_on_first_failure);
    }

    #[test]
    fn runtime_spec_accepts_optional_acceptance_suite_omitted() {
        let spec: ProjectRuntimeSpec = serde_json::from_value(json!({
            "runCommand": "npm run dev"
        }))
        .expect("deserialize minimal spec");
        assert!(spec.acceptance_suite.is_none());
    }

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
