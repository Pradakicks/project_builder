use crate::db::Database;
use crate::models::{
    Project, ProjectRuntimeSession, ProjectRuntimeSpec, ProjectRuntimeStatus,
    RuntimeLogTail, RuntimeReadinessCheck, RuntimeSessionStatus, RuntimeStopBehavior,
};
use crate::AppState;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, info};

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
    let package_json = working_dir.join("package.json");
    if package_json.exists() {
        let raw = std::fs::read_to_string(&package_json).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let scripts = json
            .get("scripts")
            .and_then(|value| value.as_object())
            .cloned()
            .unwrap_or_default();

        let run_command = if scripts.contains_key("dev") {
            "npm run dev"
        } else if scripts.contains_key("start") {
            "npm run start"
        } else {
            return Ok(None);
        };

        let verify_command = if scripts.contains_key("test") {
            Some("npm test".to_string())
        } else if scripts.contains_key("build") {
            Some("npm run build".to_string())
        } else {
            None
        };

        let (app_url, port_hint, readiness_check) = if scripts
            .get("dev")
            .and_then(|value| value.as_str())
            .map(|script| script.contains("vite"))
            .unwrap_or(false)
        {
            (
                Some("http://127.0.0.1:5173".to_string()),
                Some(5173),
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
                Some(3000),
                RuntimeReadinessCheck::Http {
                    path: "/".to_string(),
                    expected_status: 200,
                    timeout_seconds: 90,
                    poll_interval_ms: 500,
                },
            )
        };

        return Ok(Some(ProjectRuntimeSpec {
            install_command: Some("npm install".to_string()),
            run_command: run_command.to_string(),
            readiness_check,
            verify_command,
            stop_behavior: RuntimeStopBehavior::Kill,
            app_url,
            port_hint,
        }));
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

fn resolve_runtime_url(spec: &ProjectRuntimeSpec) -> Option<String> {
    spec.app_url
        .as_ref()
        .map(|url| url.trim_end_matches('/').to_string())
        .or_else(|| spec.port_hint.map(|port| format!("http://127.0.0.1:{port}")))
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
    cmd
}

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

async fn current_runtime_status(
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
        Some(handle) => Some(refresh_runtime_session(&handle).await?),
        None => None,
    };

    Ok(ProjectRuntimeStatus {
        project_id: project_id.to_string(),
        spec,
        session,
    })
}

async fn start_runtime_session(
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
            let deadline = tokio::time::Instant::now() + Duration::from_secs(*timeout_seconds);
            let interval = Duration::from_millis(*poll_interval_ms);
            let mut last_error = None;

            loop {
                if tokio::time::Instant::now() > deadline {
                    let error = last_error.unwrap_or_else(|| {
                        format!("Timed out waiting for runtime readiness at {target}")
                    });
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
                    Ok(response) => {
                        last_error = Some(format!(
                            "Unexpected readiness status: {}",
                            response.status()
                        ));
                    }
                    Err(error) => {
                        last_error = Some(error.to_string());
                    }
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

async fn stop_runtime_session(
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
                match stop_behavior {
                    RuntimeStopBehavior::Kill => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    RuntimeStopBehavior::Graceful { timeout_seconds } => {
                        if timeout(Duration::from_secs(timeout_seconds), child.wait())
                            .await
                            .is_err()
                        {
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

async fn configure_runtime_impl(
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

async fn get_runtime_status_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
) -> Result<ProjectRuntimeStatus, String> {
    current_runtime_status(state_db, runtime_sessions, &project_id).await
}

async fn detect_runtime_impl(
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

async fn start_runtime_impl(
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
    start_runtime_session(&project, &spec, runtime_sessions).await
}

async fn stop_runtime_impl(
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
    Ok(ProjectRuntimeStatus {
        project_id,
        spec,
        session,
    })
}

async fn tail_runtime_logs_impl(
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

async fn verify_runtime_impl(
    state_db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    project_id: String,
) -> Result<String, String> {
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
    if !matches!(
        status.session.as_ref().map(|session| &session.status),
        Some(RuntimeSessionStatus::Running)
    ) {
        return Err("Runtime must be running before verification".to_string());
    }

    if let Some(verify_command) = spec.verify_command.as_deref() {
        let handle = {
            let sessions = runtime_sessions.lock().map_err(|e| e.to_string())?;
            sessions
                .get(&project_id)
                .cloned()
                .ok_or_else(|| "Runtime session not found".to_string())?
        };
        let exit_code = run_shell_command_to_completion(
            verify_command,
            &working_directory,
            handle,
            "verify",
            300,
        )
        .await?;
        if exit_code != 0 {
            return Err(format!("Verify command exited with code {exit_code}"));
        }
        return Ok(format!("Verification passed via `{verify_command}`"));
    }

    let url = resolve_runtime_url(&spec).ok_or_else(|| {
        "Runtime verification requires verifyCommand or appUrl/portHint".to_string()
    })?;
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Runtime health check failed with status {}", response.status()));
    }
    Ok(format!("Runtime responded successfully at {url}"))
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
) -> Result<String, String> {
    info!(project_id = %project_id, "IPC: verify_runtime");
    verify_runtime_impl(&state.db, &state.runtime_sessions, project_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{
        AutonomyMode, ConflictResolutionPolicy, PhaseControlPolicy, ProjectSettings,
        RuntimeReadinessCheck, RuntimeSessionStatus, RuntimeStopBehavior,
    };
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
}
