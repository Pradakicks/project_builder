use crate::agent::build_runtime_detection_prompt;
use crate::agent::runner::resolve_llm_config;
use crate::db::Database;
use crate::llm::{self, LlmConfig};
use crate::models::{
    AcceptanceCheck, AcceptanceSuite, CheckKind, LogScanMode, Project, ProjectRuntimeSession,
    ProjectRuntimeSpec, ProjectRuntimeStatus, RuntimeLogTail, RuntimeReadinessCheck,
    RuntimeSessionStatus, RuntimeStopBehavior, VerificationCheck, VerificationResult,
};
use tokio_util::sync::CancellationToken;
use crate::AppState;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, info, warn};

const RUNTIME_ROOT_DIR: &str = "project-builder-dashboard-runtime";
const INSTALL_TIMEOUT_SECS: u64 = 900;
const DEFAULT_START_GRACE_MS: u64 = 250;

pub type RuntimeSessions = HashMap<String, Arc<RuntimeSessionHandle>>;

pub struct RuntimeSessionHandle {
    session: AsyncMutex<ProjectRuntimeSession>,
    child: AsyncMutex<Option<tokio::process::Child>>,
    log_path: PathBuf,
    log_file: AsyncMutex<tokio::fs::File>,
    recent_logs: AsyncMutex<VecDeque<String>>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn runtime_root() -> PathBuf {
    std::env::temp_dir().join(RUNTIME_ROOT_DIR)
}

fn runtime_session_dir(project_id: &str, session_id: &str) -> PathBuf {
    runtime_root().join(project_id).join(session_id)
}

fn runtime_log_path(project_id: &str, session_id: &str) -> PathBuf {
    runtime_session_dir(project_id, session_id).join("runtime.log")
}

fn trim_command(command: &str) -> String {
    command.trim().to_string()
}

fn detect_runtime_spec_from_working_dir(working_dir: &Path) -> Result<Option<ProjectRuntimeSpec>, String> {
    // 1. runtime.json — keep existing logic exactly as-is
    let runtime_json = working_dir.join("runtime.json");
    if runtime_json.exists() {
        let raw = std::fs::read_to_string(&runtime_json).map_err(|e| e.to_string())?;
        if let Ok(mut spec) = serde_json::from_str::<ProjectRuntimeSpec>(&raw) {
            if validate_runtime_spec(&spec).is_ok() {
                // Enforce a minimum readiness timeout — agents often write short
                // values (e.g. 30s) that don't account for npm install + compile time.
                const MIN_READINESS_TIMEOUT_SECS: u64 = 90;
                match &mut spec.readiness_check {
                    RuntimeReadinessCheck::Http { timeout_seconds, .. }
                    | RuntimeReadinessCheck::TcpPort { timeout_seconds, .. }
                        if *timeout_seconds < MIN_READINESS_TIMEOUT_SECS =>
                    {
                        debug!(
                            "Bumping agent-authored readiness timeout from {}s to {}s",
                            timeout_seconds, MIN_READINESS_TIMEOUT_SECS
                        );
                        *timeout_seconds = MIN_READINESS_TIMEOUT_SECS;
                    }
                    _ => {}
                }
                debug!("Using agent-authored runtime.json");
                return Ok(Some(spec));
            }
        }
        // Malformed or invalid spec — fall through to pattern matching
        debug!("runtime.json exists but is invalid, falling back to pattern detection");
    }

    // 2. Node.js — package.json exists
    let package_json = working_dir.join("package.json");
    if package_json.exists() {
        let raw = std::fs::read_to_string(&package_json).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let scripts = json
            .get("scripts")
            .and_then(|value| value.as_object())
            .cloned()
            .unwrap_or_default();

        // Detect package manager from lockfiles (priority order)
        let (pm_run, pm_install) = if working_dir.join("bun.lockb").exists() {
            ("bun run", "bun install")
        } else if working_dir.join("pnpm-lock.yaml").exists() {
            ("pnpm", "pnpm install")
        } else if working_dir.join("yarn.lock").exists() {
            ("yarn", "yarn install")
        } else {
            ("npm run", "npm install")
        };

        // Script detection: prefer "dev" over "start"
        let script_key = if scripts.contains_key("dev") {
            "dev"
        } else if scripts.contains_key("start") {
            "start"
        } else {
            return Ok(None);
        };

        let run_command = if pm_run == "npm run" {
            if script_key == "dev" {
                "npm run dev".to_string()
            } else {
                "npm run start".to_string()
            }
        } else if pm_run == "bun run" {
            format!("bun run {script_key}")
        } else {
            // pnpm and yarn: "pnpm dev" / "yarn dev" etc.
            format!("{} {script_key}", pm_run)
        };

        // verify_command uses the detected package manager
        let verify_command = if scripts.contains_key("test") {
            Some(if pm_run == "npm run" {
                "npm test".to_string()
            } else if pm_run == "bun run" {
                "bun run test".to_string()
            } else {
                format!("{} test", pm_run)
            })
        } else if scripts.contains_key("build") {
            Some(if pm_run == "npm run" {
                "npm run build".to_string()
            } else if pm_run == "bun run" {
                "bun run build".to_string()
            } else {
                format!("{} build", pm_run)
            })
        } else {
            None
        };

        let dependencies = json
            .get("dependencies")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let dev_script_str = scripts
            .get("dev")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Port detection: Next.js first, then Vite, then default
        let (app_url, port_hint, readiness_check) = if dependencies.contains_key("next") {
            (
                Some("http://127.0.0.1:3000".to_string()),
                Some(3000u16),
                RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 90,
                    poll_interval_ms: 500,
                },
            )
        } else if dev_script_str.contains("vite") {
            (
                Some("http://127.0.0.1:5173".to_string()),
                Some(5173u16),
                RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 90,
                    poll_interval_ms: 500,
                },
            )
        } else {
            (
                Some("http://127.0.0.1:3000".to_string()),
                Some(3000u16),
                RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 90,
                    poll_interval_ms: 500,
                },
            )
        };

        return Ok(Some(ProjectRuntimeSpec {
            install_command: Some(pm_install.to_string()),
            run_command,
            readiness_check,
            verify_command,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url,
            port_hint,
            acceptance_suite: None,
        }));
    }

    // 3. Maven/Java — pom.xml exists
    let pom_xml = working_dir.join("pom.xml");
    if pom_xml.exists() {
        let pom_content = read_file_head(&pom_xml, 300).unwrap_or_default();
        if pom_content.contains("spring-boot") {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("mvn install -DskipTests".to_string()),
                run_command: "mvn spring-boot:run".to_string(),
                readiness_check: RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 120,
                    poll_interval_ms: 500,
                },
                verify_command: Some("mvn test -q".to_string()),
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: Some("http://127.0.0.1:8080".to_string()),
                port_hint: Some(8080),
                acceptance_suite: None,
            }));
        } else {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("mvn install -DskipTests".to_string()),
                run_command: "mvn exec:java".to_string(),
                readiness_check: RuntimeReadinessCheck::None,
                verify_command: Some("mvn test -q".to_string()),
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: None,
                port_hint: None,
                acceptance_suite: None,
            }));
        }
    }

    // 4. Gradle/Java — build.gradle or build.gradle.kts exists
    let build_gradle = working_dir.join("build.gradle");
    let build_gradle_kts = working_dir.join("build.gradle.kts");
    let gradle_path = if build_gradle.exists() {
        Some(build_gradle)
    } else if build_gradle_kts.exists() {
        Some(build_gradle_kts)
    } else {
        None
    };
    if let Some(gradle_path) = gradle_path {
        let gradle_content = read_file_head(&gradle_path, 100).unwrap_or_default();
        if gradle_content.contains("org.springframework.boot") {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("./gradlew build -x test".to_string()),
                run_command: "./gradlew bootRun".to_string(),
                readiness_check: RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 120,
                    poll_interval_ms: 500,
                },
                verify_command: Some("./gradlew test".to_string()),
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: Some("http://127.0.0.1:8080".to_string()),
                port_hint: Some(8080),
                acceptance_suite: None,
            }));
        } else {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("./gradlew build -x test".to_string()),
                run_command: "./gradlew run".to_string(),
                readiness_check: RuntimeReadinessCheck::None,
                verify_command: Some("./gradlew test".to_string()),
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: None,
                port_hint: None,
                acceptance_suite: None,
            }));
        }
    }

    // 5. Docker Compose — check all four filenames
    let compose_files = ["docker-compose.yml", "docker-compose.yaml", "compose.yaml", "compose.yml"];
    if compose_files.iter().any(|f| working_dir.join(f).exists()) {
        return Ok(Some(ProjectRuntimeSpec {
            install_command: Some("docker compose pull".to_string()),
            run_command: "docker compose up".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Graceful { timeout_seconds: 15 },
            app_url: None,
            port_hint: None,
            acceptance_suite: None,
        }));
    }

    // 6. Rust project — Cargo.toml exists
    let cargo_toml = working_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        return Ok(Some(ProjectRuntimeSpec {
            install_command: Some("cargo build".to_string()),
            run_command: "cargo run".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: Some("cargo check".to_string()),
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: None,
            port_hint: None,
            acceptance_suite: None,
        }));
    }

    // 7. Go module — go.mod exists
    let go_mod = working_dir.join("go.mod");
    if go_mod.exists() {
        return Ok(Some(ProjectRuntimeSpec {
            install_command: None,
            run_command: "go run .".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: Some("go build ./...".to_string()),
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: None,
            port_hint: None,
            acceptance_suite: None,
        }));
    }

    // 8. Ruby — Gemfile exists
    let gemfile = working_dir.join("Gemfile");
    if gemfile.exists() {
        let gemfile_content = std::fs::read_to_string(&gemfile).unwrap_or_default();
        if gemfile_content.contains("rails") {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("bundle install".to_string()),
                run_command: "bundle exec rails server".to_string(),
                readiness_check: RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 60,
                    poll_interval_ms: 500,
                },
                verify_command: None,
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: Some("http://127.0.0.1:3000".to_string()),
                port_hint: Some(3000),
                acceptance_suite: None,
            }));
        } else if gemfile_content.contains("sinatra") {
            let ruby_entrypoints = ["app.rb", "server.rb", "main.rb"];
            let entry = ruby_entrypoints
                .iter()
                .find(|f| working_dir.join(f).exists())
                .copied()
                .unwrap_or("app.rb");
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("bundle install".to_string()),
                run_command: format!("bundle exec ruby {entry}"),
                readiness_check: RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 30,
                    poll_interval_ms: 500,
                },
                verify_command: None,
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: Some("http://127.0.0.1:4567".to_string()),
                port_hint: Some(4567),
                acceptance_suite: None,
            }));
        } else {
            let ruby_entrypoints = ["app.rb", "server.rb", "main.rb"];
            let entry = ruby_entrypoints
                .iter()
                .find(|f| working_dir.join(f).exists())
                .copied()
                .unwrap_or("app.rb");
            return Ok(Some(ProjectRuntimeSpec {
                install_command: Some("bundle install".to_string()),
                run_command: format!("bundle exec ruby {entry}"),
                readiness_check: RuntimeReadinessCheck::None,
                verify_command: None,
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: None,
                port_hint: None,
                acceptance_suite: None,
            }));
        }
    }

    // 9. Python Django — manage.py exists (check BEFORE requirements.txt/pyproject.toml)
    let manage_py = working_dir.join("manage.py");
    if manage_py.exists() {
        let has_requirements = working_dir.join("requirements.txt").exists();
        let install_command = if has_requirements {
            Some("pip install -r requirements.txt".to_string())
        } else {
            None
        };
        return Ok(Some(ProjectRuntimeSpec {
            install_command,
            run_command: "python3 manage.py runserver 0.0.0.0:8000".to_string(),
            readiness_check: RuntimeReadinessCheck::Http {
                path: "/".to_string(),
                expected_status: 200,
                timeout_seconds: 30,
                poll_interval_ms: 500,
            },
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:8000".to_string()),
            port_hint: Some(8000),
            acceptance_suite: None,
        }));
    }

    // 10. Python — requirements.txt OR pyproject.toml exists
    let has_requirements = working_dir.join("requirements.txt").exists();
    let has_pyproject = working_dir.join("pyproject.toml").exists();
    if has_requirements || has_pyproject {
        // Look for a common entrypoint
        let entrypoints = ["app.py", "main.py", "server.py", "run.py"];
        if let Some(entry) = entrypoints.iter().find(|f| working_dir.join(f).exists()) {
            let content = std::fs::read_to_string(working_dir.join(entry)).unwrap_or_default();

            // Check for Django keyword
            if content.contains("django") || content.contains("Django") {
                let install_command = if has_requirements {
                    Some("pip install -r requirements.txt".to_string())
                } else {
                    None
                };
                return Ok(Some(ProjectRuntimeSpec {
                    install_command,
                    run_command: "python3 manage.py runserver 0.0.0.0:8000".to_string(),
                    readiness_check: RuntimeReadinessCheck::Http {
                        path: "/".to_string(),
                        expected_status: 200,
                        timeout_seconds: 30,
                        poll_interval_ms: 500,
                    },
                    verify_command: None,
                    stop_behavior: RuntimeStopBehavior::Kill,
                    app_url: Some("http://127.0.0.1:8000".to_string()),
                    port_hint: Some(8000),
                    acceptance_suite: None,
                }));
            }

            let is_web = content.contains("flask")
                || content.contains("fastapi")
                || content.contains("uvicorn")
                || content.contains("http.server")
                || content.contains("Flask")
                || content.contains("FastAPI");

            // Port extraction: regex on entrypoint content
            let port = {
                let re = regex::Regex::new(r"port\s*[=:]\s*(\d{4,5})").unwrap();
                if let Some(caps) = re.captures(&content) {
                    caps.get(1)
                        .and_then(|m| m.as_str().parse::<u16>().ok())
                        .unwrap_or_else(|| {
                            if content.contains("fastapi") || content.contains("uvicorn") || content.contains("FastAPI") {
                                8000u16
                            } else {
                                5000u16
                            }
                        })
                } else if content.contains("fastapi") || content.contains("uvicorn") || content.contains("FastAPI") {
                    8000u16
                } else {
                    5000u16
                }
            };

            let readiness_check = if is_web {
                RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 30,
                    poll_interval_ms: 500,
                }
            } else {
                RuntimeReadinessCheck::None
            };
            let app_url = if is_web {
                Some(format!("http://127.0.0.1:{port}"))
            } else {
                None
            };
            let port_hint = if is_web { Some(port) } else { None };

            // Install: prefer requirements.txt, else check pyproject.toml for [build-system]
            let install_command = if has_requirements {
                Some("pip install -r requirements.txt".to_string())
            } else if has_pyproject {
                let pyproject_content = std::fs::read_to_string(working_dir.join("pyproject.toml")).unwrap_or_default();
                if pyproject_content.contains("[build-system]") {
                    Some("pip install -e .".to_string())
                } else {
                    None
                }
            } else {
                None
            };

            return Ok(Some(ProjectRuntimeSpec {
                install_command,
                run_command: format!("python3 {entry}"),
                readiness_check,
                verify_command: None,
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url,
                port_hint,
                acceptance_suite: None,
            }));
        }
    }

    // 11. Static HTML site — index.html exists, no package.json
    let index_html = working_dir.join("index.html");
    if index_html.exists() {
        return Ok(Some(ProjectRuntimeSpec {
            install_command: None,
            run_command: "python3 -m http.server 8080".to_string(),
            readiness_check: RuntimeReadinessCheck::Http {
                path: "/".to_string(),
                expected_status: 200,
                timeout_seconds: 15,
                poll_interval_ms: 250,
            },
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:8080".to_string()),
            port_hint: Some(8080),
            acceptance_suite: None,
        }));
    }

    // 12. Makefile fallback — only if nothing else matched
    let makefile = working_dir.join("Makefile");
    if makefile.exists() {
        let makefile_content = read_file_head(&makefile, 100).unwrap_or_default();
        // Check for targets in priority order: dev, run, start, serve
        let target = ["dev", "run", "start", "serve"].iter().find(|&&t| {
            makefile_content
                .lines()
                .any(|line| line.starts_with(&format!("{t}:")))
        });
        if let Some(target) = target {
            return Ok(Some(ProjectRuntimeSpec {
                install_command: None,
                run_command: format!("make {target}"),
                readiness_check: RuntimeReadinessCheck::None,
                verify_command: None,
                stop_behavior: RuntimeStopBehavior::Kill,
                app_url: None,
                port_hint: None,
                acceptance_suite: None,
            }));
        }
    }

    Ok(None)
}

fn validate_runtime_spec(spec: &ProjectRuntimeSpec) -> Result<(), String> {
    if trim_command(&spec.run_command).is_empty() {
        return Err("Runtime run command cannot be empty".to_string());
    }
    if spec
        .install_command
        .as_ref()
        .map(|command| trim_command(command).is_empty())
        .unwrap_or(false)
    {
        return Err("Runtime install command cannot be blank".to_string());
    }
    if spec
        .verify_command
        .as_ref()
        .map(|command| trim_command(command).is_empty())
        .unwrap_or(false)
    {
        return Err("Runtime verify command cannot be blank".to_string());
    }
    if spec
        .app_url
        .as_ref()
        .map(|url| trim_command(url).is_empty())
        .unwrap_or(false)
    {
        return Err("Runtime app URL cannot be blank".to_string());
    }
    Ok(())
}

fn enforce_min_readiness_timeout(spec: &mut ProjectRuntimeSpec) {
    const MIN_SECS: u64 = 90;
    match &mut spec.readiness_check {
        RuntimeReadinessCheck::Http { timeout_seconds, .. }
        | RuntimeReadinessCheck::TcpPort { timeout_seconds, .. }
            if *timeout_seconds < MIN_SECS =>
        {
            debug!("Bumping LLM-detected readiness timeout from {}s to {}s", timeout_seconds, MIN_SECS);
            *timeout_seconds = MIN_SECS;
        }
        _ => {}
    }
}

fn resolve_runtime_url(spec: &ProjectRuntimeSpec) -> Option<String> {
    spec.app_url
        .as_ref()
        .map(|url| url.trim_end_matches('/').to_string())
        .or_else(|| spec.port_hint.map(|port| format!("http://127.0.0.1:{port}")))
}

fn is_managed_runtime_status(status: &RuntimeSessionStatus) -> bool {
    matches!(
        status,
        RuntimeSessionStatus::Starting
            | RuntimeSessionStatus::Running
            | RuntimeSessionStatus::Stopping
    )
}

fn runtime_session_detail(session: &ProjectRuntimeSession) -> String {
    session.last_error.clone().unwrap_or_else(|| match session.status {
        RuntimeSessionStatus::Orphaned => {
            "Runtime session is orphaned after the app restarted".to_string()
        }
        RuntimeSessionStatus::Failed => {
            "Runtime session failed without a recorded error".to_string()
        }
        _ => "Runtime session has no recorded error".to_string(),
    })
}

fn verification_blocker_message(session: Option<&ProjectRuntimeSession>) -> String {
    match session {
        None => "No runtime session exists for this project".to_string(),
        Some(session) => match session.status {
            RuntimeSessionStatus::Starting => {
                "Runtime is still starting; wait until it is running before verification".to_string()
            }
            RuntimeSessionStatus::Running => {
                "Runtime must be running under the current app session before verification".to_string()
            }
            RuntimeSessionStatus::Stopping => {
                "Runtime is stopping; start it again before verification".to_string()
            }
            RuntimeSessionStatus::Stopped => {
                "Runtime is stopped; start it before verification".to_string()
            }
            RuntimeSessionStatus::Failed => {
                format!(
                    "Runtime failed before verification: {}",
                    runtime_session_detail(session)
                )
            }
            RuntimeSessionStatus::Orphaned => {
                format!(
                    "Runtime session is orphaned and unmanaged: {}",
                    runtime_session_detail(session)
                )
            }
            RuntimeSessionStatus::Idle => {
                "Runtime is idle; start it before verification".to_string()
            }
        },
    }
}

async fn port_is_occupied(port: u16) -> Result<bool, String> {
    match timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    {
        Ok(Ok(_)) => Ok(true),
        Ok(Err(_)) => Ok(false),
        Err(_) => Ok(true),
    }
}

fn port_conflict_message(
    port: u16,
    latest_session: Option<&ProjectRuntimeSession>,
) -> String {
    match latest_session {
        Some(session) if session.port_hint == Some(port) && is_managed_runtime_status(&session.status) => {
            format!(
                "Port {port} is already in use by runtime session {}. Stop the existing runtime or free the port before starting again.",
                session.session_id
            )
        }
        Some(session)
            if session.port_hint == Some(port) && matches!(session.status, RuntimeSessionStatus::Orphaned) =>
        {
            format!(
                "Port {port} is still held by orphaned runtime session {}. Stop or release the orphaned process before starting again.",
                session.session_id
            )
        }
        Some(session) if session.port_hint == Some(port) => {
            format!(
                "Port {port} is already in use and the latest runtime session {} is {:?}.",
                session.session_id,
                session.status
            )
        }
        _ => format!(
            "Port {port} is already in use. Free the port or change the runtime portHint before starting."
        ),
    }
}

async fn preflight_runtime_port(
    state_db: &std::sync::Mutex<Database>,
    project_id: &str,
    port: u16,
) -> Result<(), String> {
    if !port_is_occupied(port).await? {
        return Ok(());
    }

    let latest_session = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.latest_runtime_session(project_id)?
            .map(|record| record.session)
    };

    Err(port_conflict_message(port, latest_session.as_ref()))
}

async fn normalize_orphaned_runtime_session(
    state_db: &std::sync::Mutex<Database>,
    project_id: &str,
    mut session: ProjectRuntimeSession,
) -> Result<ProjectRuntimeSession, String> {
    if is_managed_runtime_status(&session.status) {
        session.status = RuntimeSessionStatus::Orphaned;
        session.updated_at = now();
        if session.last_error.is_none() {
            session.last_error = Some("Runtime session became orphaned when the app restarted".to_string());
        }

        let db = state_db.lock().map_err(|e| e.to_string())?;
        let _ = db.upsert_runtime_session(project_id, None, &session)?;
    }

    Ok(session)
}

fn shell_command(shell_cmd: &str, working_dir: &Path) -> Command {
    let mut cmd = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(shell_cmd);
        command
    } else {
        let mut command = Command::new("sh");
        command.arg("-lc").arg(shell_cmd);
        command
    };

    cmd.current_dir(working_dir);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    // Unix: put the shell and all its descendants in their own process group
    // so `stop_runtime` can reap the whole tree via killpg. Without this, the
    // `sh -lc "npm start"` wrapper spawns `node server.js` in its own PGID,
    // and killing the shell leaves node orphaned and holding its port.
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    cmd
}

/// SIGTERM the entire process group, wait up to 2s for it to exit, then SIGKILL
/// any stragglers. Mirror of `agent::external::terminate_process_group`. The
/// short grace (2s vs 3s) matches our runtime `stop_behavior.graceful` default.
#[cfg(unix)]
async fn terminate_runtime_process_group(pid: u32) {
    let pgid = pid as libc::pid_t;
    unsafe {
        libc::killpg(pgid, libc::SIGTERM);
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
async fn terminate_runtime_process_group(_pid: u32) {}

async fn append_runtime_log(
    handle: &Arc<RuntimeSessionHandle>,
    source: &str,
    line: &str,
) -> Result<(), String> {
    let entry = format!("[{source}] {line}");
    {
        let mut file = handle.log_file.lock().await;
        file.write_all(entry.as_bytes()).await.map_err(|e| e.to_string())?;
        file.write_all(b"\n").await.map_err(|e| e.to_string())?;
        file.flush().await.map_err(|e| e.to_string())?;
    }

    let mut recent = handle.recent_logs.lock().await;
    recent.push_back(entry);
    while recent.len() > 200 {
        recent.pop_front();
    }
    Ok(())
}

async fn pump_output<R>(
    reader: R,
    handle: Arc<RuntimeSessionHandle>,
    source: &'static str,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let _ = append_runtime_log(&handle, source, &line).await;
    }
}

async fn run_shell_command_to_completion(
    command: &str,
    working_dir: &Path,
    handle: Arc<RuntimeSessionHandle>,
    label: &str,
    timeout_secs: u64,
) -> Result<i32, String> {
    append_runtime_log(&handle, "runtime", &format!("{label} command: {command}")).await?;
    let mut child = shell_command(command, working_dir)
        .spawn()
        .map_err(|e| format!("Failed to spawn {label} command: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Failed to capture {label} stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Failed to capture {label} stderr"))?;

    let stdout_handle = tokio::spawn(pump_output(stdout, handle.clone(), "stdout"));
    let stderr_handle = tokio::spawn(pump_output(stderr, handle.clone(), "stderr"));

    let status = match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(format!("{label} command failed: {e}")),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout_handle.await;
            let _ = stderr_handle.await;
            return Err(format!("{label} command timed out after {timeout_secs}s"));
        }
    };

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    let exit_code = status.code().unwrap_or(-1);
    append_runtime_log(
        &handle,
        "runtime",
        &format!("{label} command exited with code {exit_code}"),
    )
    .await?;
    Ok(exit_code)
}

async fn create_runtime_handle(
    project_id: &str,
    spec: &ProjectRuntimeSpec,
) -> Result<Arc<RuntimeSessionHandle>, String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_dir = runtime_session_dir(project_id, &session_id);
    tokio::fs::create_dir_all(&session_dir)
        .await
        .map_err(|e| e.to_string())?;

    let log_path = runtime_log_path(project_id, &session_id);
    if let Some(parent) = log_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    let log_file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await
        .map_err(|e| e.to_string())?;

    let session = ProjectRuntimeSession {
        session_id,
        status: RuntimeSessionStatus::Starting,
        started_at: Some(now()),
        updated_at: now(),
        ended_at: None,
        url: resolve_runtime_url(spec),
        port_hint: spec.port_hint,
        log_path: Some(log_path.display().to_string()),
        recent_logs: Vec::new(),
        last_error: None,
        exit_code: None,
        pid: None,
    };

    Ok(Arc::new(RuntimeSessionHandle {
        session: AsyncMutex::new(session),
        child: AsyncMutex::new(None),
        log_path,
        log_file: AsyncMutex::new(log_file),
        recent_logs: AsyncMutex::new(VecDeque::new()),
    }))
}

async fn refresh_runtime_session(
    handle: &Arc<RuntimeSessionHandle>,
) -> Result<ProjectRuntimeSession, String> {
    let mut child_guard = handle.child.lock().await;
    if let Some(child) = child_guard.as_mut() {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let ended_at = now();
            let exit_code = status.code();
            let mut session = handle.session.lock().await;
            session.updated_at = ended_at.clone();
            session.ended_at = Some(ended_at);
            session.exit_code = exit_code;
            session.pid = None;
            if status.success() {
                session.status = RuntimeSessionStatus::Stopped;
                session.last_error = None;
            } else {
                session.status = RuntimeSessionStatus::Failed;
                session.last_error = Some(format!(
                    "Runtime process exited with status {}",
                    exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ));
            }
            *child_guard = None;
        }
    }

    let mut session = handle.session.lock().await.clone();
    session.recent_logs = handle
        .recent_logs
        .lock()
        .await
        .iter()
        .cloned()
        .collect();
    Ok(session)
}

async fn mark_runtime_failed(handle: &Arc<RuntimeSessionHandle>, error: String) -> String {
    let mut session = handle.session.lock().await;
    session.status = RuntimeSessionStatus::Failed;
    session.updated_at = now();
    session.ended_at = Some(now());
    session.last_error = Some(error.clone());
    error
}

pub(crate) async fn current_runtime_status(
    db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: &str,
) -> Result<ProjectRuntimeStatus, String> {
    let spec = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(project_id)?;
        project.settings.runtime_spec.clone()
    };

    let handle = {
        let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.get(project_id).cloned()
    };

    let session = match handle {
        Some(handle) => {
            let session = refresh_runtime_session(&handle).await?;
            {
                let db = db.lock().map_err(|e| e.to_string())?;
                let _ = db.upsert_runtime_session(project_id, None, &session);
            }
            Some(session)
        }
        None => {
            let record = {
                let db = db.lock().map_err(|e| e.to_string())?;
                db.latest_runtime_session(project_id)?
            };

            match record {
                Some(record) => {
                    let session = normalize_orphaned_runtime_session(
                        db,
                        project_id,
                        record.session,
                    )
                    .await?;
                    Some(session)
                }
                None => None,
            }
        }
    };

    Ok(ProjectRuntimeStatus {
        project_id: project_id.to_string(),
        spec,
        session,
    })
}

pub(crate) async fn start_runtime_session(
    state_db: &std::sync::Mutex<Database>,
    project: &Project,
    spec: &ProjectRuntimeSpec,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
) -> Result<ProjectRuntimeStatus, String> {
    validate_runtime_spec(spec)?;

    let working_directory = project
        .settings
        .working_directory
        .clone()
        .ok_or_else(|| "Project runtime requires a configured working directory".to_string())?;
    let working_directory = PathBuf::from(working_directory);
    if !working_directory.exists() {
        return Err(format!(
            "Working directory does not exist: {}",
            working_directory.display()
        ));
    }

    let existing_handle = {
        let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.get(&project.id).cloned()
    };

    if let Some(handle) = &existing_handle {
        let session = refresh_runtime_session(handle).await?;
        if matches!(
            session.status,
            RuntimeSessionStatus::Starting
                | RuntimeSessionStatus::Running
                | RuntimeSessionStatus::Stopping
        ) {
            return Ok(ProjectRuntimeStatus {
                project_id: project.id.clone(),
                spec: Some(spec.clone()),
                session: Some(session),
            });
        }
    }

    if let Some(port) = spec.port_hint {
        preflight_runtime_port(state_db, &project.id, port).await?;
    }

    let handle = create_runtime_handle(&project.id, spec).await?;
    {
        let mut sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(project.id.clone(), handle.clone());
    }

    if let Some(install_command) = spec.install_command.as_deref() {
        append_runtime_log(&handle, "runtime", "running install command").await?;
        let exit_code = match run_shell_command_to_completion(
            install_command,
            &working_directory,
            handle.clone(),
            "install",
            INSTALL_TIMEOUT_SECS,
        )
        .await
        {
            Ok(code) => code,
            Err(error) => return Err(mark_runtime_failed(&handle, error).await),
        };
        if exit_code != 0 {
            let error = format!("Install command exited with code {exit_code}");
            return Err(mark_runtime_failed(&handle, error).await);
        }
    }

    append_runtime_log(&handle, "runtime", "spawning run command").await?;
    let mut child = match shell_command(&spec.run_command, &working_directory).spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("Failed to spawn runtime command: {error}");
            return Err(mark_runtime_failed(&handle, message).await);
        }
    };

    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture runtime stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture runtime stderr".to_string())?;

    {
        let mut session = handle.session.lock().await;
        session.pid = pid;
    }
    {
        let mut child_guard = handle.child.lock().await;
        *child_guard = Some(child);
    }

    tokio::spawn(pump_output(stdout, handle.clone(), "stdout"));
    tokio::spawn(pump_output(stderr, handle.clone(), "stderr"));

    match &spec.readiness_check {
        RuntimeReadinessCheck::None => {
            sleep(Duration::from_millis(DEFAULT_START_GRACE_MS)).await;
            let snapshot = refresh_runtime_session(&handle).await?;
            if matches!(snapshot.status, RuntimeSessionStatus::Failed | RuntimeSessionStatus::Stopped)
            {
                return Err(snapshot
                    .last_error
                    .unwrap_or_else(|| "Runtime process exited before it became ready".to_string()));
            }
        }
        RuntimeReadinessCheck::Http {
            path,
            expected_status,
            timeout_seconds,
            poll_interval_ms,
        } => {
            let base_url = resolve_runtime_url(spec).ok_or_else(|| {
                "HTTP readiness check requires appUrl or portHint".to_string()
            })?;
            let target = format!(
                "{}/{}",
                base_url.trim_end_matches('/'),
                path.trim_start_matches('/')
            );
            let client = reqwest::Client::new();
            let deadline_dur = Duration::from_secs(*timeout_seconds);
            let interval = Duration::from_millis(*poll_interval_ms);
            let deadline = tokio::time::Instant::now() + deadline_dur;

            loop {
                // Check process liveness before every poll attempt.
                let snapshot = refresh_runtime_session(&handle).await?;
                if matches!(snapshot.status, RuntimeSessionStatus::Failed | RuntimeSessionStatus::Stopped)
                {
                    return Err(snapshot
                        .last_error
                        .unwrap_or_else(|| "Runtime process exited before readiness".to_string()));
                }

                if tokio::time::Instant::now() > deadline {
                    let error = format!("Timed out waiting for runtime readiness at {target}");
                    let _ = mark_runtime_failed(&handle, error.clone()).await;
                    let mut child_guard = handle.child.lock().await;
                    if let Some(child) = child_guard.as_mut() {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    return Err(error);
                }

                match client.get(&target).send().await {
                    Ok(response) if response.status().as_u16() == *expected_status => break,
                    Ok(_) | Err(_) => {}
                }

                sleep(interval).await;
            }
        }
        RuntimeReadinessCheck::TcpPort {
            timeout_seconds,
            poll_interval_ms,
        } => {
            let port = spec.port_hint.ok_or_else(|| {
                "TCP port readiness check requires portHint".to_string()
            })?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(*timeout_seconds);
            let interval = Duration::from_millis(*poll_interval_ms);

            loop {
                if tokio::time::Instant::now() > deadline {
                    let error = format!("Timed out waiting for TCP port {port} to become ready");
                    let _ = mark_runtime_failed(&handle, error.clone()).await;
                    let mut child_guard = handle.child.lock().await;
                    if let Some(child) = child_guard.as_mut() {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    return Err(error);
                }

                match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                    Ok(_) => break,
                    Err(_) => {}
                }

                let snapshot = refresh_runtime_session(&handle).await?;
                if matches!(snapshot.status, RuntimeSessionStatus::Failed | RuntimeSessionStatus::Stopped)
                {
                    return Err(snapshot
                        .last_error
                        .unwrap_or_else(|| "Runtime process exited before readiness".to_string()));
                }

                sleep(interval).await;
            }
        }
    }

    {
        let mut session = handle.session.lock().await;
        session.status = RuntimeSessionStatus::Running;
        session.updated_at = now();
        session.last_error = None;
    }

    let session = refresh_runtime_session(&handle).await?;
    Ok(ProjectRuntimeStatus {
        project_id: project.id.clone(),
        spec: Some(spec.clone()),
        session: Some(session),
    })
}

pub(crate) async fn stop_runtime_session(
    project_id: &str,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    stop_behavior: RuntimeStopBehavior,
) -> Result<Option<ProjectRuntimeSession>, String> {
    let handle = {
        let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.get(project_id).cloned()
    };

    let Some(handle) = handle else {
        return Ok(None);
    };

    let mut child_guard = handle.child.lock().await;
    if child_guard.is_none() {
        drop(child_guard);
        let session = refresh_runtime_session(&handle).await?;
        return Ok(Some(session));
    }

    {
        let mut session = handle.session.lock().await;
        session.status = RuntimeSessionStatus::Stopping;
        session.updated_at = now();
    }

    if let Some(child) = child_guard.as_mut() {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let ended_at = now();
                let mut session = handle.session.lock().await;
                session.status = if status.success() {
                    RuntimeSessionStatus::Stopped
                } else {
                    RuntimeSessionStatus::Failed
                };
                session.ended_at = Some(ended_at.clone());
                session.updated_at = ended_at;
                session.exit_code = status.code();
                session.pid = None;
                session.last_error = if status.success() {
                    None
                } else {
                    Some(format!(
                        "Runtime process exited with status {}",
                        status
                            .code()
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    ))
                };
            }
            None => {
                // Capture the PID before we kill the direct child — after
                // kill() the child struct's id() becomes None, so we need to
                // reap the process group *before* calling child.kill() /
                // wait().
                let pgid = child.id();

                match stop_behavior {
                    RuntimeStopBehavior::Kill => {
                        if let Some(pid) = pgid {
                            terminate_runtime_process_group(pid).await;
                        }
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    RuntimeStopBehavior::Graceful { timeout_seconds } => {
                        // Send SIGTERM to the whole group first so children
                        // (e.g. `node server.js` spawned by `npm start`) get
                        // a chance to drain before we escalate.
                        #[cfg(unix)]
                        if let Some(pid) = pgid {
                            unsafe {
                                libc::killpg(pid as libc::pid_t, libc::SIGTERM);
                            }
                        }
                        if timeout(Duration::from_secs(timeout_seconds), child.wait())
                            .await
                            .is_err()
                        {
                            if let Some(pid) = pgid {
                                terminate_runtime_process_group(pid).await;
                            }
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                        }
                    }
                }

                let ended_at = now();
                let mut session = handle.session.lock().await;
                session.status = RuntimeSessionStatus::Stopped;
                session.ended_at = Some(ended_at.clone());
                session.updated_at = ended_at;
                session.pid = None;
                session.exit_code = None;
                session.last_error = None;
            }
        }
    }

    *child_guard = None;
    drop(child_guard);

    let session = refresh_runtime_session(&handle).await?;
    Ok(Some(session))
}

pub(crate) async fn configure_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
    spec: ProjectRuntimeSpec,
) -> Result<Project, String> {
    validate_runtime_spec(&spec)?;

    let project = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.get_project(&project_id)?
    };

    let handle = {
        let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.get(&project_id).cloned()
    };
    if let Some(handle) = handle {
        let session = refresh_runtime_session(&handle).await?;
        if matches!(
            session.status,
            RuntimeSessionStatus::Starting
                | RuntimeSessionStatus::Running
                | RuntimeSessionStatus::Stopping
        ) {
            return Err("Stop the runtime before reconfiguring it".to_string());
        }
    }

    let mut settings = project.settings.clone();
    settings.runtime_spec = Some(spec);

    let db = state_db.lock().map_err(|e| e.to_string())?;
    db.update_project(&project_id, None, None, None, Some(&settings))
}

pub(crate) async fn get_runtime_status_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    current_runtime_status(state_db, runtime_sessions, &project_id).await
}

pub(crate) async fn detect_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    project_id: String,
) -> Result<Option<ProjectRuntimeSpec>, String> {
    let project = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.get_project(&project_id)?
    };
    let working_directory = project
        .settings
        .working_directory
        .ok_or_else(|| "Project runtime requires a configured working directory".to_string())?;
    let working_directory = PathBuf::from(working_directory);
    if !working_directory.exists() {
        return Err(format!(
            "Working directory does not exist: {}",
            working_directory.display()
        ));
    }

    detect_runtime_spec_from_working_dir(&working_directory)
}

pub(crate) async fn start_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    let project = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.get_project(&project_id)?
    };
    let spec = project
        .settings
        .runtime_spec
        .clone()
        .ok_or_else(|| "Project runtime is not configured".to_string())?;

    debug!(project_id = %project_id, "Starting runtime session");
    let status = start_runtime_session(state_db, &project, &spec, runtime_sessions).await?;
    if let Some(ref session) = status.session {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        let _ = db.upsert_runtime_session(&project.id, None, session);
    }
    Ok(status)
}

pub(crate) async fn stop_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    let spec = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(&project_id)?;
        project.settings.runtime_spec
    };
    let stop_behavior = spec
        .as_ref()
        .map(|runtime| runtime.stop_behavior.clone())
        .unwrap_or_default();
    let session = stop_runtime_session(&project_id, runtime_sessions, stop_behavior).await?;
    if let Some(ref session) = session {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        let _ = db.upsert_runtime_session(&project_id, None, session);
    }
    Ok(ProjectRuntimeStatus {
        project_id,
        spec,
        session,
    })
}

pub(crate) async fn tail_runtime_logs_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
    limit: Option<usize>,
) -> Result<RuntimeLogTail, String> {
    let _ = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.get_project(&project_id)?
    };

    let handle = {
        let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
        sessions.get(&project_id).cloned()
    };

    let Some(handle) = handle else {
        let record = {
            let db = state_db.lock().map_err(|e| e.to_string())?;
            db.latest_runtime_session(&project_id)?
        };
        if let Some(record) = record {
            if let Some(path) = record.session.log_path.clone() {
                if Path::new(&path).exists() {
                    let raw = tokio::fs::read_to_string(&path)
                        .await
                        .map_err(|e| e.to_string())?;
                    let requested = limit.unwrap_or(120).max(1);
                    let lines = raw
                        .lines()
                        .rev()
                        .take(requested)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .map(str::to_string)
                        .collect();
                    return Ok(RuntimeLogTail {
                        path: Some(path),
                        lines,
                    });
                }
            }
        }
        return Ok(RuntimeLogTail {
            path: None,
            lines: Vec::new(),
        });
    };

    let session = refresh_runtime_session(&handle).await?;
    let path = session.log_path.clone().or_else(|| {
        Some(handle.log_path.display().to_string())
    });

    if let Some(path) = path {
        if !Path::new(&path).exists() {
            return Ok(RuntimeLogTail {
                path: Some(path),
                lines: Vec::new(),
            });
        }

        let raw = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| e.to_string())?;
        let requested = limit.unwrap_or(120).max(1);
        let lines = raw
            .lines()
            .rev()
            .take(requested)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(str::to_string)
            .collect();

        Ok(RuntimeLogTail {
            path: Some(path),
            lines,
        })
    } else {
        Ok(RuntimeLogTail {
            path: None,
            lines: session.recent_logs,
        })
    }
}

pub(crate) async fn verify_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
    cancel: CancellationToken,
) -> Result<VerificationResult, String> {
    let project = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        db.get_project(&project_id)?
    };
    let spec = project
        .settings
        .runtime_spec
        .clone()
        .ok_or_else(|| "Project runtime is not configured".to_string())?;
    let working_directory = project
        .settings
        .working_directory
        .clone()
        .ok_or_else(|| "Project runtime requires a configured working directory".to_string())?;
    let working_directory = PathBuf::from(working_directory);

    let status = current_runtime_status(state_db, runtime_sessions, &project_id).await?;
    if let Some(session) = status.session.as_ref() {
        if !matches!(session.status, RuntimeSessionStatus::Running) {
            return Err(verification_blocker_message(Some(session)));
        }
    } else {
        return Err(verification_blocker_message(None));
    }

    let suite = spec
        .acceptance_suite
        .clone()
        .unwrap_or_else(|| derive_default_suite(&spec));

    let started_at = chrono::Utc::now().to_rfc3339();

    if suite.checks.is_empty() {
        let finished_at = chrono::Utc::now().to_rfc3339();
        return Ok(VerificationResult {
            passed: true,
            message: "No verification configured — skipped".to_string(),
            checks: vec![VerificationCheck {
                name: "no verification configured".to_string(),
                kind: CheckKind::Skipped,
                passed: true,
                detail: "Runtime spec has no acceptance suite and none could be derived."
                    .to_string(),
                duration_ms: 0,
                expected: None,
                actual: None,
            }],
            started_at,
            finished_at,
        });
    }

    let mut checks: Vec<VerificationCheck> = Vec::with_capacity(suite.checks.len());
    for acceptance_check in &suite.checks {
        if cancel.is_cancelled() {
            return Err("Verification cancelled".to_string());
        }
        let check = run_acceptance_check(
            acceptance_check,
            &project_id,
            &spec,
            &working_directory,
            runtime_sessions,
            &cancel,
        )
        .await;
        let failed = !check.passed;
        checks.push(check);
        if failed && suite.stop_on_first_failure {
            break;
        }
    }

    let passed_count = checks.iter().filter(|c| c.passed).count();
    let total_count = checks.len();
    let passed = total_count > 0 && passed_count == total_count;

    let first_failure = checks.iter().find(|c| !c.passed).cloned();
    let message = if passed {
        format!("{passed_count}/{total_count} checks passed")
    } else if let Some(failed) = &first_failure {
        format!("{}: {}", failed.name, failed.detail)
    } else {
        "Verification failed".to_string()
    };

    let finished_at = chrono::Utc::now().to_rfc3339();
    Ok(VerificationResult {
        passed,
        message,
        checks,
        started_at,
        finished_at,
    })
}

/// When a project has no `acceptance_suite`, derive a safe default:
///   1. Log scan for fatal-looking patterns (`MustNotMatch`).
///   2. HTTP probe of `appUrl` / `portHint` if available (status 200–399).
///   3. Shell of `verify_command` if configured.
/// This preserves today's "try HTTP 200 + verify command" behavior while
/// closing the "app prints FATAL but returns 200" gap.
fn derive_default_suite(spec: &ProjectRuntimeSpec) -> AcceptanceSuite {
    let mut checks: Vec<AcceptanceCheck> = Vec::new();

    checks.push(AcceptanceCheck::LogScan {
        name: "log scan — fatal patterns".to_string(),
        patterns: vec![
            r"(?i)panic!?".to_string(),
            r"(?i)FATAL".to_string(),
            r"(?i)unhandled (rejection|exception)".to_string(),
            r"ECONNREFUSED".to_string(),
        ],
        mode: LogScanMode::MustNotMatch,
        last_n_lines: 200,
    });

    if resolve_runtime_url(spec).is_some() {
        checks.push(AcceptanceCheck::HttpProbe {
            name: "http probe — root".to_string(),
            path: "/".to_string(),
            expected_status_min: 200,
            expected_status_max: 399,
            expected_body_contains: None,
            expected_content_type: None,
            timeout_seconds: 10,
        });
    }

    if let Some(cmd) = spec.verify_command.as_deref() {
        checks.push(AcceptanceCheck::Shell {
            name: "verify command".to_string(),
            command: cmd.to_string(),
            timeout_seconds: 300,
        });
    }

    AcceptanceSuite {
        checks,
        stop_on_first_failure: false,
    }
}

async fn run_acceptance_check(
    check: &AcceptanceCheck,
    project_id: &str,
    spec: &ProjectRuntimeSpec,
    working_directory: &Path,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    cancel: &CancellationToken,
) -> VerificationCheck {
    match check {
        AcceptanceCheck::HttpProbe {
            name,
            path,
            expected_status_min,
            expected_status_max,
            expected_body_contains,
            expected_content_type,
            timeout_seconds,
        } => {
            run_http_probe_check(
                name,
                spec,
                path,
                *expected_status_min,
                *expected_status_max,
                expected_body_contains.as_deref(),
                expected_content_type.as_deref(),
                *timeout_seconds,
                cancel,
            )
            .await
        }
        AcceptanceCheck::Shell {
            name,
            command,
            timeout_seconds,
        } => {
            run_shell_acceptance_check(
                name,
                command,
                *timeout_seconds,
                project_id,
                working_directory,
                runtime_sessions,
                cancel,
            )
            .await
        }
        AcceptanceCheck::LogScan {
            name,
            patterns,
            mode,
            last_n_lines,
        } => {
            run_log_scan_check(
                name,
                patterns,
                mode,
                *last_n_lines,
                project_id,
                runtime_sessions,
            )
            .await
        }
        AcceptanceCheck::TcpPort {
            name,
            port,
            timeout_seconds,
        } => run_tcp_port_check(name, *port, *timeout_seconds, cancel).await,
    }
}

async fn run_http_probe_check(
    name: &str,
    spec: &ProjectRuntimeSpec,
    path: &str,
    status_min: u16,
    status_max: u16,
    body_contains: Option<&str>,
    content_type: Option<&str>,
    timeout_secs: u64,
    cancel: &CancellationToken,
) -> VerificationCheck {
    let base = match resolve_runtime_url(spec) {
        Some(url) => url,
        None => {
            return failed_check(
                name,
                CheckKind::Http,
                0,
                "No appUrl or portHint set; cannot run http probe",
                Some(format!("status in {status_min}..={status_max} at {path}")),
                Some("no runtime URL".to_string()),
            );
        }
    };
    let full_url = if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    };

    let mut expected_parts = vec![format!("status in {status_min}..={status_max}")];
    if let Some(ct) = content_type {
        expected_parts.push(format!("content-type contains \"{ct}\""));
    }
    if let Some(bc) = body_contains {
        expected_parts.push(format!("body contains \"{bc}\""));
    }
    let expected = Some(expected_parts.join("; "));

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return failed_check(
                name,
                CheckKind::Http,
                0,
                &format!("Failed to build HTTP client: {e}"),
                expected,
                Some(e.to_string()),
            );
        }
    };

    let started = std::time::Instant::now();
    let send = client.get(&full_url).send();
    let response = tokio::select! {
        r = send => r,
        _ = cancel.cancelled() => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::Http,
                duration_ms,
                "Cancelled mid-probe",
                expected,
                Some("cancelled".to_string()),
            );
        }
    };

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::Http,
                duration_ms,
                &format!("Request error: {e}"),
                expected,
                Some(format!("error: {e}")),
            );
        }
    };

    let status = response.status().as_u16();
    let actual_content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body_result = tokio::select! {
        r = response.text() => r,
        _ = cancel.cancelled() => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::Http,
                duration_ms,
                "Cancelled while reading body",
                expected,
                Some("cancelled".to_string()),
            );
        }
    };
    let body = body_result.unwrap_or_else(|e| format!("[failed to read body: {e}]"));
    let body_snippet: String = body.chars().take(512).collect();

    let duration_ms = started.elapsed().as_millis() as i64;
    let mut fail_reasons: Vec<String> = Vec::new();
    if status < status_min || status > status_max {
        fail_reasons.push(format!("status {status} not in {status_min}..={status_max}"));
    }
    if let Some(ct_want) = content_type {
        match actual_content_type.as_deref() {
            Some(ct) if ct.to_lowercase().contains(&ct_want.to_lowercase()) => {}
            Some(ct) => {
                fail_reasons.push(format!("content-type \"{ct}\" missing \"{ct_want}\""));
            }
            None => fail_reasons.push(format!("no content-type header (wanted \"{ct_want}\")")),
        }
    }
    if let Some(bc) = body_contains {
        if !body.contains(bc) {
            fail_reasons.push(format!("body did not contain \"{bc}\""));
        }
    }

    let passed = fail_reasons.is_empty();
    let actual = Some(format!(
        "status {status}; content-type {}; body: {}",
        actual_content_type.as_deref().unwrap_or("(none)"),
        if body_snippet.is_empty() { "(empty)" } else { &body_snippet },
    ));
    let detail = if passed {
        format!("HTTP {status} from {full_url}")
    } else {
        format!("{} — {}", fail_reasons.join("; "), full_url)
    };

    VerificationCheck {
        name: name.to_string(),
        kind: CheckKind::Http,
        passed,
        detail,
        duration_ms,
        expected,
        actual,
    }
}

async fn run_shell_acceptance_check(
    name: &str,
    command: &str,
    timeout_secs: u64,
    project_id: &str,
    working_directory: &Path,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    cancel: &CancellationToken,
) -> VerificationCheck {
    let expected = Some("exit code 0".to_string());

    let handle = {
        let sessions = match runtime_sessions.lock() {
            Ok(g) => g,
            Err(_) => {
                return failed_check(
                    name,
                    CheckKind::Shell,
                    0,
                    "Runtime sessions mutex poisoned",
                    expected,
                    Some("lock poisoned".to_string()),
                );
            }
        };
        match sessions.get(project_id).cloned() {
            Some(h) => h,
            None => {
                return failed_check(
                    name,
                    CheckKind::Shell,
                    0,
                    "Runtime session not found — is the runtime started?",
                    expected,
                    Some("no session".to_string()),
                );
            }
        }
    };

    let check_start = std::time::Instant::now();
    let run = run_shell_command_to_completion(command, working_directory, handle, "verify", timeout_secs);
    let result = tokio::select! {
        r = run => r,
        _ = cancel.cancelled() => {
            let duration_ms = check_start.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::Shell,
                duration_ms,
                "Cancelled mid-command",
                expected,
                Some("cancelled".to_string()),
            );
        }
    };
    let duration_ms = check_start.elapsed().as_millis() as i64;

    match result {
        Ok(exit_code) => {
            let passed = exit_code == 0;
            VerificationCheck {
                name: name.to_string(),
                kind: CheckKind::Shell,
                passed,
                detail: if passed {
                    format!("exited 0 via `{command}`")
                } else {
                    format!("exited {exit_code} via `{command}`")
                },
                duration_ms,
                expected,
                actual: Some(format!("exit code {exit_code}")),
            }
        }
        Err(e) => failed_check(
            name,
            CheckKind::Shell,
            duration_ms,
            &format!("Shell command failed: {e}"),
            expected,
            Some(e),
        ),
    }
}

async fn run_log_scan_check(
    name: &str,
    patterns: &[String],
    mode: &LogScanMode,
    last_n_lines: usize,
    project_id: &str,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
) -> VerificationCheck {
    let expected = Some(format!(
        "{} match for [{}] over last {} lines",
        match mode {
            LogScanMode::MustMatch => "at least one",
            LogScanMode::MustNotMatch => "no",
        },
        patterns
            .iter()
            .map(|p| format!("/{p}/"))
            .collect::<Vec<_>>()
            .join(", "),
        last_n_lines,
    ));

    let started = std::time::Instant::now();

    let compiled: Result<Vec<regex::Regex>, _> = patterns
        .iter()
        .map(|p| regex::Regex::new(p))
        .collect();
    let regexes = match compiled {
        Ok(r) => r,
        Err(e) => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::LogScan,
                duration_ms,
                &format!("Invalid regex in log-scan patterns: {e}"),
                expected,
                Some(format!("regex error: {e}")),
            );
        }
    };

    let handle = {
        let sessions = match runtime_sessions.lock() {
            Ok(g) => g,
            Err(_) => {
                let duration_ms = started.elapsed().as_millis() as i64;
                return failed_check(
                    name,
                    CheckKind::LogScan,
                    duration_ms,
                    "Runtime sessions mutex poisoned",
                    expected,
                    Some("lock poisoned".to_string()),
                );
            }
        };
        sessions.get(project_id).cloned()
    };
    let handle = match handle {
        Some(h) => h,
        None => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::LogScan,
                duration_ms,
                "Runtime session not found — is the runtime started?",
                expected,
                Some("no session".to_string()),
            );
        }
    };

    let lines: Vec<String> = {
        let recent = handle.recent_logs.lock().await;
        let start = recent.len().saturating_sub(last_n_lines);
        recent.iter().skip(start).cloned().collect()
    };

    // Find the first (pattern, line) match; for MustMatch we pass on the first,
    // for MustNotMatch we fail on the first.
    let hit = lines.iter().enumerate().find_map(|(idx, line)| {
        regexes
            .iter()
            .find(|rx| rx.is_match(line))
            .map(|rx| (idx, rx.as_str().to_string(), line.clone()))
    });
    let duration_ms = started.elapsed().as_millis() as i64;

    match (mode, hit) {
        (LogScanMode::MustMatch, Some((_, pattern, line))) => VerificationCheck {
            name: name.to_string(),
            kind: CheckKind::LogScan,
            passed: true,
            detail: format!("matched /{pattern}/ on line: {line}"),
            duration_ms,
            expected,
            actual: Some(format!("matched /{pattern}/")),
        },
        (LogScanMode::MustMatch, None) => VerificationCheck {
            name: name.to_string(),
            kind: CheckKind::LogScan,
            passed: false,
            detail: format!("no pattern matched in last {} lines", lines.len()),
            duration_ms,
            expected,
            actual: Some(format!("0 matches over {} lines", lines.len())),
        },
        (LogScanMode::MustNotMatch, None) => VerificationCheck {
            name: name.to_string(),
            kind: CheckKind::LogScan,
            passed: true,
            detail: format!("no forbidden pattern matched in last {} lines", lines.len()),
            duration_ms,
            expected,
            actual: Some(format!("0 matches over {} lines", lines.len())),
        },
        (LogScanMode::MustNotMatch, Some((idx, pattern, _line))) => {
            let start = idx.saturating_sub(3);
            let excerpt = lines[start..=idx].join("\n");
            VerificationCheck {
                name: name.to_string(),
                kind: CheckKind::LogScan,
                passed: false,
                detail: format!("matched forbidden /{pattern}/ on line {}", idx + 1),
                duration_ms,
                expected,
                actual: Some(format!(
                    "matched /{pattern}/ on line {}:\n{}",
                    idx + 1,
                    excerpt,
                )),
            }
        }
    }
}

async fn run_tcp_port_check(
    name: &str,
    port: u16,
    timeout_secs: u64,
    cancel: &CancellationToken,
) -> VerificationCheck {
    let expected = Some(format!("TCP connect to 127.0.0.1:{port} within {timeout_secs}s"));
    let started = std::time::Instant::now();
    let connect = tokio::net::TcpStream::connect(("127.0.0.1", port));
    let timed = tokio::time::timeout(Duration::from_secs(timeout_secs), connect);
    let outcome = tokio::select! {
        r = timed => r,
        _ = cancel.cancelled() => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return failed_check(
                name,
                CheckKind::TcpPort,
                duration_ms,
                "Cancelled mid-connect",
                expected,
                Some("cancelled".to_string()),
            );
        }
    };
    let duration_ms = started.elapsed().as_millis() as i64;
    match outcome {
        Ok(Ok(_)) => VerificationCheck {
            name: name.to_string(),
            kind: CheckKind::TcpPort,
            passed: true,
            detail: format!("connected to 127.0.0.1:{port}"),
            duration_ms,
            expected,
            actual: Some(format!("connected in {duration_ms}ms")),
        },
        Ok(Err(e)) => failed_check(
            name,
            CheckKind::TcpPort,
            duration_ms,
            &format!("connect failed: {e}"),
            expected,
            Some(e.to_string()),
        ),
        Err(_) => failed_check(
            name,
            CheckKind::TcpPort,
            duration_ms,
            &format!("timed out after {timeout_secs}s"),
            expected,
            Some(format!("timeout after {timeout_secs}s")),
        ),
    }
}

fn failed_check(
    name: &str,
    kind: CheckKind,
    duration_ms: i64,
    detail: &str,
    expected: Option<String>,
    actual: Option<String>,
) -> VerificationCheck {
    VerificationCheck {
        name: name.to_string(),
        kind,
        passed: false,
        detail: detail.to_string(),
        duration_ms,
        expected,
        actual,
    }
}

pub(crate) async fn detect_runtime_with_agent_impl<R: tauri::Runtime>(
    state_db: &std::sync::Mutex<Database>,
    app_handle: &AppHandle<R>,
    project_id: String,
) -> Result<Option<ProjectRuntimeSpec>, String> {
    let (project, working_directory, provider_name, api_key, model, base_url) = {
        let db = state_db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(&project_id)?;
        let wd = project
            .settings
            .working_directory
            .clone()
            .ok_or_else(|| "Project has no working directory configured".to_string())?;
        let (provider_name, api_key, model, base_url) = resolve_llm_config(&project.settings);
        (project, PathBuf::from(wd), provider_name, api_key, model, base_url)
    };

    if !working_directory.exists() {
        return Err(format!(
            "Working directory does not exist: {}",
            working_directory.display()
        ));
    }

    if api_key.is_empty() {
        warn!(project_id = %project_id, "No API key available for runtime detection agent");
        return Ok(None);
    }

    let file_listing = collect_file_listing(&working_directory, 3);
    let source_extensions = [
        ".rs", ".go", ".py", ".js", ".ts", ".tsx", ".jsx", ".java", ".rb",
        ".html", ".css", ".toml", ".json", ".yaml", ".yml",
    ];
    let config_names = [
        "package.json", "Cargo.toml", "go.mod", "requirements.txt",
        "pyproject.toml", "Makefile", "index.html",
    ];
    let has_recognizable_files = file_listing.iter().any(|f| {
        let lower = f.to_lowercase();
        source_extensions.iter().any(|ext| lower.ends_with(ext))
            || config_names.iter().any(|name| lower.ends_with(name))
    });
    if !has_recognizable_files {
        warn!(
            project_id = %project_id,
            file_count = file_listing.len(),
            "Skipping LLM runtime detection: no recognizable source or config files found"
        );
        return Ok(None);
    }

    let key_files = [
        ("package.json", 200usize),
        ("Cargo.toml", 200),
        ("go.mod", 100),
        ("requirements.txt", 100),
        ("pyproject.toml", 100),
        ("Makefile", 100),
        ("index.html", 200),
        ("app.py", 200),
        ("main.py", 200),
        ("server.py", 200),
        ("run.py", 200),
    ];

    let mut snippets = Vec::new();
    for (name, max_lines) in key_files {
        let path = working_directory.join(name);
        if path.exists() {
            if let Some(content) = read_file_head(&path, max_lines) {
                snippets.push((name.to_string(), content));
            }
        }
    }

    let messages = build_runtime_detection_prompt(&project.name, &file_listing, &snippets);
    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 1024,
    };

    let (tx, mut rx) = mpsc::channel::<String>(64);
    let project_id_for_chunks = project_id.clone();
    let app_handle_for_chunks = app_handle.clone();
    let full_output = Arc::new(AsyncMutex::new(String::new()));
    let full_output_writer = full_output.clone();
    let chunk_forwarder = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            full_output_writer.lock().await.push_str(&chunk);
            let _ = app_handle_for_chunks.emit(
                "runtime-detection-chunk",
                serde_json::json!({
                    "projectId": project_id_for_chunks,
                    "chunk": chunk,
                    "done": false
                }),
            );
        }
    });

    if let Err(e) = provider.chat_stream(&messages, &config, tx).await {
        warn!(project_id = %project_id, error = %e, "LLM runtime detection stream error — falling back");
        return Ok(None);
    }

    let _ = chunk_forwarder.await;
    let _ = app_handle.emit(
        "runtime-detection-chunk",
        serde_json::json!({
            "projectId": project_id,
            "chunk": "",
            "done": true
        }),
    );

    let raw_output = full_output.lock().await.clone();
    let cleaned = raw_output.trim();
    let cleaned = if cleaned.starts_with("```") {
        let after_fence = cleaned.find('\n').map(|i| &cleaned[i + 1..]).unwrap_or(cleaned);
        after_fence.rfind("```").map(|i| &after_fence[..i]).unwrap_or(after_fence)
    } else {
        cleaned
    };

    let start = cleaned.find('{');
    let end = cleaned.rfind('}');
    let (start, end) = match (start, end) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Ok(None),
    };
    let json_str = &cleaned[start..=end];

    if json_str.trim().is_empty() || json_str.trim().eq_ignore_ascii_case("null") {
        return Ok(None);
    }

    let mut spec = match serde_json::from_str::<ProjectRuntimeSpec>(json_str) {
        Ok(s) => s,
        Err(e) => {
            warn!(project_id = %project_id, error = %e, raw = json_str, "LLM runtime detection produced unparseable JSON");
            return Ok(None);
        }
    };
    if let Err(e) = validate_runtime_spec(&spec) {
        warn!(project_id = %project_id, error = %e, "LLM runtime detection produced invalid spec");
        return Ok(None);
    }
    enforce_min_readiness_timeout(&mut spec);
    Ok(Some(spec))
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn configure_runtime(
    state: tauri::State<'_, AppState>,
    project_id: String,
    spec: ProjectRuntimeSpec,
) -> Result<Project, String> {
    info!(project_id = %project_id, "IPC: configure_runtime");
    configure_runtime_impl(&state.db, &state.runtime_sessions, project_id, spec).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn get_runtime_status(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    info!(project_id = %project_id, "IPC: get_runtime_status");
    get_runtime_status_impl(&state.db, &state.runtime_sessions, project_id).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn detect_runtime(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<Option<ProjectRuntimeSpec>, String> {
    info!(project_id = %project_id, "IPC: detect_runtime");
    detect_runtime_impl(&state.db, project_id).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn start_runtime(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    info!(project_id = %project_id, "IPC: start_runtime");
    start_runtime_impl(&state.db, &state.runtime_sessions, project_id).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn stop_runtime(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    info!(project_id = %project_id, "IPC: stop_runtime");
    stop_runtime_impl(&state.db, &state.runtime_sessions, project_id).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn tail_runtime_logs(
    state: tauri::State<'_, AppState>,
    project_id: String,
    limit: Option<usize>,
) -> Result<RuntimeLogTail, String> {
    debug!(project_id = %project_id, limit = ?limit, "IPC: tail_runtime_logs");
    tail_runtime_logs_impl(&state.db, &state.runtime_sessions, project_id, limit).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn verify_runtime(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<VerificationResult, String> {
    info!(project_id = %project_id, "IPC: verify_runtime");
    // Manual IPC entry has no ambient cancel token; create a never-fired one.
    verify_runtime_impl(
        &state.db,
        &state.runtime_sessions,
        project_id,
        CancellationToken::new(),
    )
    .await
}

/// Walk `dir` to `max_depth`, collecting relative path strings, skipping common noise dirs.
fn collect_file_listing(dir: &Path, max_depth: usize) -> Vec<String> {
    let mut results = Vec::new();
    let skip_dirs = ["node_modules", "target", ".git", "dist", "build", ".next", "__pycache__"];

    fn walk(
        base: &Path,
        current: &Path,
        depth: usize,
        max_depth: usize,
        skip_dirs: &[&str],
        results: &mut Vec<String>,
    ) {
        if depth > max_depth || results.len() >= 50 {
            return;
        }
        let entries = match std::fs::read_dir(current) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if results.len() >= 50 {
                break;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                if skip_dirs.contains(&name_str.as_ref()) {
                    continue;
                }
                walk(base, &path, depth + 1, max_depth, skip_dirs, results);
            } else {
                if let Ok(rel) = path.strip_prefix(base) {
                    results.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }

    walk(dir, dir, 0, max_depth, &skip_dirs, &mut results);
    results
}

/// Read the first `max_lines` of a file as a string.
fn read_file_head(path: &Path, max_lines: usize) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().take(max_lines).collect();
    Some(lines.join("\n"))
}

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn detect_runtime_with_agent(
    state: tauri::State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
) -> Result<Option<ProjectRuntimeSpec>, String> {
    info!(project_id = %project_id, "IPC: detect_runtime_with_agent");
    detect_runtime_with_agent_impl(&state.db, &app_handle, project_id).await
}

/// Returns the heuristic-only detection hint for a project without saving it.
/// Used by the Quick Runtime Setup UI when a goal run is blocked at
/// RuntimeConfiguration — gives the operator a pre-populated starting point
/// without running an expensive LLM call.
/// Returns None if no spec is already configured, heuristic finds nothing, or
/// the working directory is not set.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn get_runtime_detection_hint(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<Option<ProjectRuntimeSpec>, String> {
    info!(project_id = %project_id, "IPC: get_runtime_detection_hint");
    let (working_directory, spec_already_configured) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(&project_id)?;
        let wd = project.settings.working_directory.clone();
        let has_spec = project.settings.runtime_spec.is_some();
        (wd, has_spec)
    };
    // If already configured, no hint needed
    if spec_already_configured {
        return Ok(None);
    }
    let working_directory = match working_directory {
        Some(wd) => std::path::PathBuf::from(wd),
        None => return Ok(None),
    };
    if !working_directory.exists() {
        return Ok(None);
    }
    // Run heuristic detection only (fast, offline, no tokens)
    detect_runtime_spec_from_working_dir(&working_directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::goal_run_commands::build_goal_run_delivery_snapshot_impl;
    use crate::db::Database;
    use crate::models::{
        AutonomyMode, CheckKind, ConflictResolutionPolicy, GoalRunStatus, PhaseControlPolicy,
        ProjectSettings, RuntimeReadinessCheck, RuntimeSessionStatus, RuntimeStopBehavior,
    };
    use crate::test_support::GoalRunScenarioFixture;
    use std::sync::Mutex;

    fn temp_dir(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-runtime-{case}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn temp_db_path(case: &str) -> PathBuf {
        temp_dir(case).join("data.db")
    }

    fn cleanup(path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    async fn insert_live_runtime_handle(
        sessions: &Mutex<RuntimeSessions>,
        project_id: &str,
        session: &ProjectRuntimeSession,
        log_path: &Path,
    ) -> std::sync::Arc<RuntimeSessionHandle> {
        let log_file = tokio::fs::File::open(log_path)
            .await
            .expect("open scenario runtime log");
        let handle = std::sync::Arc::new(RuntimeSessionHandle {
            session: AsyncMutex::new(session.clone()),
            child: AsyncMutex::new(None),
            log_path: log_path.to_path_buf(),
            log_file: AsyncMutex::new(log_file),
            recent_logs: AsyncMutex::new(session.recent_logs.clone().into_iter().collect()),
        });

        sessions
            .lock()
            .expect("lock runtime sessions")
            .insert(project_id.to_string(), handle.clone());

        handle
    }

    fn create_project_settings(working_dir: &Path, runtime_spec: ProjectRuntimeSpec) -> ProjectSettings {
        ProjectSettings {
            llm_configs: vec![],
            default_token_budget: 100_000,
            autonomy_mode: AutonomyMode::Autopilot,
            phase_control: PhaseControlPolicy::Manual,
            conflict_resolution: ConflictResolutionPolicy::AiAssisted,
            working_directory: Some(working_dir.display().to_string()),
            default_execution_engine: None,
            post_run_validation_command: None,
            runtime_spec: Some(runtime_spec),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn configure_runtime_persists_runtime_spec() {
        let db_path = temp_db_path("configure");
        let db = Database::new_at_path(&db_path).expect("open db");
        let state_db = Mutex::new(db);
        let sessions = Mutex::new(HashMap::new());

        let project = {
            let db = state_db.lock().expect("lock db");
            db.create_project_with_settings(
                "Runtime project",
                "Configure runtime spec",
                ProjectSettings::default(),
            )
            .expect("create project")
        };

        let spec = ProjectRuntimeSpec {
            install_command: Some("npm install".to_string()),
            run_command: "npm run dev".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: Some("npm test".to_string()),
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:3000".to_string()),
            port_hint: Some(3000),
            acceptance_suite: None,
        };

        let updated = configure_runtime_impl(&state_db, &sessions, project.id.clone(), spec.clone())
            .await
            .expect("configure runtime");
        assert_eq!(
            updated.settings.runtime_spec.as_ref().map(|runtime| runtime.run_command.clone()),
            Some(spec.run_command.clone())
        );

        let stored = {
            let db = state_db.lock().expect("lock db");
            db.get_project(&project.id).expect("reload project")
        };
        assert_eq!(
            stored.settings.runtime_spec.as_ref().map(|runtime| runtime.port_hint),
            Some(Some(3000))
        );

        cleanup(&db_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_status_stop_and_tail_runtime_logs() {
        let dir = temp_dir("start-stop");
        let db_path = dir.join("data.db");
        let db = Database::new_at_path(&db_path).expect("open db");
        let state_db = Mutex::new(db);
        let sessions = Mutex::new(HashMap::new());

        let spec = ProjectRuntimeSpec {
            install_command: Some("printf 'install\\n'".to_string()),
            run_command: "printf 'booted\\n'; while :; do sleep 1; done".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:4810".to_string()),
            port_hint: Some(4810),
            acceptance_suite: None,
        };

        let project = {
            let db = state_db.lock().expect("lock db");
            db.create_project_with_settings(
                "Runtime project",
                "Start and stop runtime",
                create_project_settings(&dir, spec.clone()),
            )
            .expect("create project")
        };

        let status = start_runtime_impl(&state_db, &sessions, project.id.clone())
            .await
            .expect("start runtime");
        let session = status.session.expect("session");
        assert_eq!(session.status, RuntimeSessionStatus::Running);
        assert_eq!(session.url.as_deref(), Some("http://127.0.0.1:4810"));
        assert!(session
            .recent_logs
            .iter()
            .any(|line| line.contains("install")));

        sleep(Duration::from_millis(150)).await;

        let tail = tail_runtime_logs_impl(&state_db, &sessions, project.id.clone(), Some(20))
            .await
            .expect("tail logs");
        assert!(tail
            .lines
            .iter()
            .any(|line| line.contains("booted")));

        let stopped = stop_runtime_impl(&state_db, &sessions, project.id.clone())
            .await
            .expect("stop runtime");
        let stopped_session = stopped.session.expect("stopped session");
        assert_eq!(stopped_session.status, RuntimeSessionStatus::Stopped);

        let refreshed = get_runtime_status_impl(&state_db, &sessions, project.id.clone())
            .await
            .expect("status");
        let refreshed_session = refreshed.session.expect("refreshed session");
        assert_eq!(refreshed_session.status, RuntimeSessionStatus::Stopped);
        assert!(refreshed_session
            .recent_logs
            .iter()
            .any(|line| line.contains("booted")));

        cleanup(&db_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn log_scan_fails_on_forced_fatal_with_excerpt() {
        let sessions = Mutex::new(HashMap::new());
        let project_id = format!("forced-fatal-{}", uuid::Uuid::new_v4());
        let spec = ProjectRuntimeSpec {
            install_command: None,
            run_command: "node server.js".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:3030".to_string()),
            port_hint: Some(3030),
            acceptance_suite: None,
        };
        let handle = create_runtime_handle(&project_id, &spec)
            .await
            .expect("create runtime handle");
        append_runtime_log(&handle, "runtime", "spawning run command")
            .await
            .expect("append setup line");
        append_runtime_log(&handle, "stdout", "FATAL: forced for test")
            .await
            .expect("append fatal line");
        {
            let mut session = handle.session.lock().await;
            session.status = RuntimeSessionStatus::Running;
        }
        {
            let mut guard = sessions.lock().expect("lock runtime sessions");
            guard.insert(project_id.clone(), handle);
        }

        let check = run_log_scan_check(
            "log scan -- fatal patterns",
            &[r"(?i)FATAL".to_string()],
            &LogScanMode::MustNotMatch,
            200,
            &project_id,
            &sessions,
        )
        .await;

        assert!(!check.passed);
        assert_eq!(check.kind, CheckKind::LogScan);
        assert!(check.detail.contains("matched forbidden /(?i)FATAL/"));
        assert!(check
            .actual
            .as_deref()
            .unwrap_or("")
            .contains("FATAL: forced for test"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runtime_status_falls_back_to_persisted_session_without_live_process() {
        let dir = temp_dir("persisted-status");
        let db_path = dir.join("data.db");
        let db = Database::new_at_path(&db_path).expect("open db");
        let state_db = Mutex::new(db);
        let sessions = Mutex::new(HashMap::new());

        let spec = ProjectRuntimeSpec {
            install_command: None,
            run_command: "printf 'booted\\n'; while :; do sleep 1; done".to_string(),
            readiness_check: RuntimeReadinessCheck::None,
            verify_command: None,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url: Some("http://127.0.0.1:4820".to_string()),
            port_hint: Some(4820),
            acceptance_suite: None,
        };

        let project = {
            let db = state_db.lock().expect("lock db");
            db.create_project_with_settings(
                "Runtime project",
                "Persist runtime session history",
                create_project_settings(&dir, spec),
            )
            .expect("create project")
        };

        let started = start_runtime_impl(&state_db, &sessions, project.id.clone())
            .await
            .expect("start runtime");
        let started_session = started.session.expect("started session");
        let session_id = started_session.session_id.clone();
        assert_eq!(started_session.status, RuntimeSessionStatus::Running);

        let persisted_before_stop = {
            let db = state_db.lock().expect("lock db");
            db.latest_runtime_session(&project.id)
                .expect("load persisted session")
                .expect("persisted runtime session")
        };
        assert_eq!(persisted_before_stop.session.session_id, session_id);

        let detached_handle = {
            let mut live_sessions = sessions.lock().expect("lock runtime sessions");
            live_sessions
                .remove(&project.id)
                .expect("remove live runtime session")
        };

        let status = get_runtime_status_impl(&state_db, &sessions, project.id.clone())
            .await
            .expect("load runtime status");
        let status_session = status.session.expect("runtime status session");
        assert_eq!(status_session.session_id, session_id);
        assert_eq!(status_session.status, RuntimeSessionStatus::Orphaned);
        assert!(status_session
            .last_error
            .as_deref()
            .unwrap_or("")
            .contains("orphaned"));

        let persisted_after = {
            let db = state_db.lock().expect("lock db");
            db.latest_runtime_session(&project.id)
                .expect("load persisted session")
                .expect("persisted runtime session")
        };
        assert_eq!(persisted_after.session.status, RuntimeSessionStatus::Orphaned);

        if let Some(mut child) = detached_handle.child.lock().await.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }

        cleanup(&db_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn forced_fatal_retry_exhausted_fixture_reports_log_scan_failure_and_snapshot_contract() {
        let fixture = GoalRunScenarioFixture::forced_fatal_retry_exhausted();
        let live_handle = insert_live_runtime_handle(
            &fixture.runtime_sessions,
            &fixture.project.id,
            &fixture.runtime_session,
            &fixture.runtime_log_path,
        )
        .await;

        let verification = verify_runtime_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            fixture.project.id.clone(),
            CancellationToken::new(),
        )
        .await
        .expect("verify runtime");

        assert!(!verification.passed);
        assert_eq!(verification.checks.len(), 1);
        assert_eq!(verification.checks[0].kind, CheckKind::LogScan);
        assert!(verification.message.contains("log scan — fatal patterns"));
        assert!(verification.checks[0].detail.contains("line 16"));

        let tail = tail_runtime_logs_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            fixture.project.id.clone(),
            Some(20),
        )
        .await
        .expect("tail runtime logs");
        assert!(tail
            .lines
            .last()
            .expect("tail line")
            .contains("FATAL: forced for test"));

        let snapshot = build_goal_run_delivery_snapshot_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            &fixture.goal_run.id,
        )
        .await
        .expect("build delivery snapshot");
        assert_eq!(snapshot.retry_state.retry_count, 3);
        assert_eq!(
            snapshot
                .verification_result
                .as_ref()
                .expect("verification result")
                .passed,
            false
        );
        assert_eq!(
            snapshot.recent_events.last().map(|event| event.kind.clone()),
            Some(crate::models::GoalRunEventKind::Blocked)
        );

        drop(live_handle);
        fixture.cleanup();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stale_logs_vs_live_logs_fixture_prefers_live_snapshot_but_persisted_tail_keeps_stale_warning() {
        let fixture = GoalRunScenarioFixture::stale_logs_vs_live_logs();
        let live_handle = insert_live_runtime_handle(
            &fixture.runtime_sessions,
            &fixture.project.id,
            &fixture.runtime_session,
            &fixture.runtime_log_path,
        )
        .await;

        let snapshot = build_goal_run_delivery_snapshot_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            &fixture.goal_run.id,
        )
        .await
        .expect("build delivery snapshot");
        let live_logs = &snapshot
            .runtime_status
            .as_ref()
            .expect("runtime status")
            .session
            .as_ref()
            .expect("runtime session")
            .recent_logs;
        assert!(live_logs.iter().any(|line| line.contains("fresh status ok")));
        assert!(
            !live_logs
                .iter()
                .any(|line| line.contains("stale blocker from a prior run"))
        );

        {
            let mut sessions = fixture.runtime_sessions.lock().expect("lock runtime sessions");
            sessions.remove(&fixture.project.id);
        }
        drop(live_handle);

        let tail = tail_runtime_logs_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            fixture.project.id.clone(),
            Some(20),
        )
        .await
        .expect("tail runtime logs");
        assert!(tail
            .lines
            .iter()
            .any(|line| line.contains("WARNING: stale blocker from a prior run")));

        fixture.cleanup();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn clean_runtime_pass_fixture_verifies_without_external_server() {
        let fixture = GoalRunScenarioFixture::clean_runtime_pass();
        let live_handle = insert_live_runtime_handle(
            &fixture.runtime_sessions,
            &fixture.project.id,
            &fixture.runtime_session,
            &fixture.runtime_log_path,
        )
        .await;

        let verification = verify_runtime_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            fixture.project.id.clone(),
            CancellationToken::new(),
        )
        .await
        .expect("verify runtime");

        assert!(verification.passed);
        assert_eq!(verification.checks.len(), 1);
        assert_eq!(verification.checks[0].kind, CheckKind::LogScan);
        assert!(verification.message.contains("1/1 checks passed"));

        let snapshot = build_goal_run_delivery_snapshot_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            &fixture.goal_run.id,
        )
        .await
        .expect("build delivery snapshot");
        assert!(
            snapshot
                .verification_result
                .as_ref()
                .expect("verification result")
                .passed
        );
        assert_eq!(snapshot.goal_run.status, GoalRunStatus::Completed);
        assert!(snapshot
            .verification_result
            .as_ref()
            .expect("verification result")
            .checks
            .iter()
            .any(|check| check.passed));

        let tail = tail_runtime_logs_impl(
            &fixture.db,
            &fixture.runtime_sessions,
            fixture.project.id.clone(),
            Some(20),
        )
        .await
        .expect("tail runtime logs");
        assert!(tail
            .lines
            .iter()
            .any(|line| line.contains("runtime: server ready")));

        drop(live_handle);
        fixture.cleanup();
    }

    #[test]
    fn verification_blocker_messages_distinguish_runtime_states() {
        assert_eq!(
            verification_blocker_message(None),
            "No runtime session exists for this project"
        );

        let stopped = ProjectRuntimeSession {
            session_id: "stopped".to_string(),
            status: RuntimeSessionStatus::Stopped,
            started_at: None,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: Some("2024-01-01T00:01:00Z".to_string()),
            url: None,
            port_hint: None,
            log_path: None,
            recent_logs: vec![],
            last_error: None,
            exit_code: None,
            pid: None,
        };
        let failed = ProjectRuntimeSession { last_error: Some("boom".to_string()), status: RuntimeSessionStatus::Failed, ..stopped.clone() };
        let orphaned = ProjectRuntimeSession { last_error: Some("lost handle".to_string()), status: RuntimeSessionStatus::Orphaned, ..stopped };

        assert!(verification_blocker_message(Some(&failed)).contains("failed before verification"));
        assert!(verification_blocker_message(Some(&orphaned)).contains("orphaned and unmanaged"));
        assert!(verification_blocker_message(Some(&orphaned)).contains("lost handle"));
    }

    #[test]
    fn port_conflict_message_distinguishes_orphaned_runtime_sessions() {
        let session = ProjectRuntimeSession {
            session_id: "sess-1".to_string(),
            status: RuntimeSessionStatus::Orphaned,
            started_at: None,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: None,
            url: Some("http://127.0.0.1:3000".to_string()),
            port_hint: Some(3000),
            log_path: None,
            recent_logs: vec![],
            last_error: Some("stale process".to_string()),
            exit_code: None,
            pid: None,
        };

        let message = port_conflict_message(3000, Some(&session));
        assert!(message.contains("orphaned runtime session"));
        assert!(message.contains("sess-1"));
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).expect("write test file");
    }

    #[test]
    fn detect_node_uses_pnpm_when_pnpm_lockfile_present() {
        let dir = temp_dir("node-pnpm");
        write_file(&dir, "package.json", r#"{"scripts":{"dev":"vite"}}"#);
        write_file(&dir, "pnpm-lock.yaml", "");
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert!(spec.run_command.starts_with("pnpm"), "run_command should start with pnpm, got: {}", spec.run_command);
        assert_eq!(spec.install_command, Some("pnpm install".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_node_recognises_nextjs_and_uses_port_3000() {
        let dir = temp_dir("node-next");
        write_file(
            &dir,
            "package.json",
            r#"{"scripts":{"dev":"next dev"},"dependencies":{"next":"14.0.0"}}"#,
        );
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.port_hint, Some(3000));
        assert!(spec.run_command.contains("dev"), "run_command should contain 'dev', got: {}", spec.run_command);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_java_spring_boot_via_pom_xml() {
        let dir = temp_dir("java-spring-pom");
        write_file(
            &dir,
            "pom.xml",
            "<project>\n  <dependencies>\n    <dependency>\n      <artifactId>spring-boot-starter-web</artifactId>\n    </dependency>\n  </dependencies>\n</project>",
        );
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "mvn spring-boot:run");
        assert_eq!(spec.port_hint, Some(8080));
        match &spec.readiness_check {
            RuntimeReadinessCheck::Http { timeout_seconds, .. } => {
                assert!(*timeout_seconds >= 90, "readiness timeout should be >= 90s, got {}", timeout_seconds);
            }
            other => panic!("expected Http readiness check, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_java_spring_boot_via_build_gradle() {
        let dir = temp_dir("java-spring-gradle");
        write_file(
            &dir,
            "build.gradle",
            "plugins {\n    id 'org.springframework.boot' version '3.0.0'\n}\n",
        );
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "./gradlew bootRun");
        assert_eq!(spec.port_hint, Some(8080));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_docker_compose_produces_valid_spec() {
        let dir = temp_dir("docker-compose");
        write_file(&dir, "docker-compose.yml", "version: '3'\n");
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "docker compose up");
        match &spec.stop_behavior {
            RuntimeStopBehavior::Graceful { .. } => {}
            other => panic!("expected Graceful stop behavior, got {:?}", other),
        }
        assert!(
            matches!(spec.readiness_check, RuntimeReadinessCheck::None),
            "expected None readiness check"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_ruby_rails_from_gemfile() {
        let dir = temp_dir("ruby-rails");
        write_file(&dir, "Gemfile", "source 'https://rubygems.org'\ngem 'rails', '~> 7.0'\n");
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "bundle exec rails server");
        assert_eq!(spec.port_hint, Some(3000));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_ruby_sinatra_from_gemfile() {
        let dir = temp_dir("ruby-sinatra");
        write_file(&dir, "Gemfile", "source 'https://rubygems.org'\ngem 'sinatra'\n");
        write_file(&dir, "app.rb", "require 'sinatra'\nget '/' do\n  'Hello'\nend\n");
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert!(spec.run_command.contains("bundle exec ruby"), "run_command should contain 'bundle exec ruby', got: {}", spec.run_command);
        assert_eq!(spec.port_hint, Some(4567));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_python_django_from_manage_py() {
        let dir = temp_dir("python-django");
        write_file(&dir, "manage.py", "#!/usr/bin/env python\n# Django manage.py\n");
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "python3 manage.py runserver 0.0.0.0:8000");
        assert_eq!(spec.port_hint, Some(8000));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_makefile_fallback_uses_dev_target() {
        let dir = temp_dir("makefile-fallback");
        write_file(&dir, "Makefile", "dev:\n\techo hello\n");
        // NOTE: no other indicator files present — only Makefile
        let spec = detect_runtime_spec_from_working_dir(&dir)
            .expect("no error")
            .expect("some spec");
        assert_eq!(spec.run_command, "make dev");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
