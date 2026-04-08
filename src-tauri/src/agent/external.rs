use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error, trace};

/// Result of an external tool run.
pub struct ExternalRunResult {
    pub exit_code: i32,
    pub output: String,
    pub duration_secs: u64,
}

/// Configuration for an external engine run.
pub struct ExternalRunConfig {
    pub system_prompt: String,
    pub user_prompt: String,
    pub working_dir: String,
    pub timeout_secs: u64,
    /// Extra env vars to inject (e.g., OPENAI_API_KEY for Codex).
    pub env_vars: Vec<(String, String)>,
}

/// Build the command args for a given engine.
fn build_command(engine: &str, config: &ExternalRunConfig) -> Result<(String, Vec<String>), String> {
    debug!(engine, working_dir = %config.working_dir, "Building external command");
    match engine {
        "claude-code" => {
            let mut args = vec![
                "-p".to_string(),
                config.user_prompt.clone(),
                "--output-format".to_string(),
                "text".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--no-session-persistence".to_string(),
            ];
            if !config.system_prompt.is_empty() {
                args.push("--append-system-prompt".to_string());
                args.push(config.system_prompt.clone());
            }
            Ok(("claude".to_string(), args))
        }
        "codex" => {
            // Codex has no --system-prompt flag; prepend context to the prompt
            let full_prompt = if config.system_prompt.is_empty() {
                config.user_prompt.clone()
            } else {
                format!("{}\n\n---\n\n{}", config.system_prompt, config.user_prompt)
            };
            let args = vec![
                "exec".to_string(),
                full_prompt,
                "--full-auto".to_string(),
                "--ephemeral".to_string(),
            ];
            Ok(("codex".to_string(), args))
        }
        _ => Err(format!("Unknown execution engine: {engine}")),
    }
}

/// Check that a binary is available on PATH.
async fn check_binary(program: &str) -> Result<(), String> {
    debug!(program, "Checking binary availability");
    match Command::new("which").arg(program).output().await {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(format!(
            "'{program}' not found on PATH. Install it or check your PATH."
        )),
    }
}

/// Run an external tool, streaming stdout line-by-line through the sender.
///
/// Returns when the process exits or the timeout expires.
pub async fn run_external(
    engine: &str,
    config: &ExternalRunConfig,
    sender: mpsc::Sender<String>,
) -> Result<ExternalRunResult, String> {
    let (program, args) = build_command(engine, config)?;

    // Verify binary exists before spawning
    check_binary(&program).await?;

    info!(engine, program = %program, working_dir = %config.working_dir, timeout = config.timeout_secs, "Spawning external tool");
    debug!(arg_count = args.len(), env_vars = config.env_vars.len(), "External tool command details");
    trace!(system_prompt_len = config.system_prompt.len(), user_prompt_len = config.user_prompt.len(), "External tool prompt sizes");

    let start = Instant::now();

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&config.working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    for (key, val) in &config.env_vars {
        cmd.env(key, val);
    }

    let mut child = cmd.spawn().map_err(|e| {
        error!(program = %program, error = %e, "Failed to spawn external tool");
        format!("Failed to spawn {program}: {e}")
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    // Read stdout and stderr concurrently
    let sender_clone = sender.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut output = String::new();
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
            let _ = sender_clone.send(line + "\n").await;
        }
        output
    });

    let stderr_handle = tokio::spawn(async move {
        let mut output = String::new();
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
        }
        output
    });

    // Wait for completion with timeout
    let timeout = Duration::from_secs(config.timeout_secs);
    let result = tokio::time::timeout(timeout, child.wait()).await;

    let duration_secs = start.elapsed().as_secs();

    match result {
        Ok(Ok(status)) => {
            let stdout_output = stdout_handle.await.unwrap_or_default();
            let stderr_output = stderr_handle.await.unwrap_or_default();

            // Combine outputs — stderr appended if non-empty
            let mut full_output = stdout_output;
            if !stderr_output.is_empty() {
                if !full_output.is_empty() {
                    full_output.push('\n');
                }
                full_output.push_str("[stderr]\n");
                full_output.push_str(&stderr_output);
            }

            let exit_code = status.code().unwrap_or(-1);
            if exit_code != 0 {
                warn!(engine, exit_code, duration_secs, "External tool exited with non-zero code");
            } else {
                info!(engine, exit_code, duration_secs, "External tool completed successfully");
            }
            Ok(ExternalRunResult {
                exit_code,
                output: full_output,
                duration_secs,
            })
        }
        Ok(Err(e)) => {
            error!(error = %e, "External tool process error");
            Err(format!("Process error: {e}"))
        }
        Err(_) => {
            // Timeout — kill the child
            if let Err(e) = child.kill().await {
                warn!(error = %e, "Failed to kill timed-out process");
            }
            warn!(engine, timeout_secs = config.timeout_secs, "External tool timed out");
            let stdout_output = stdout_handle.await.unwrap_or_default();

            // Send timeout message through the channel
            let _ = sender
                .send(format!(
                    "\n[Timed out after {}s]\n",
                    config.timeout_secs
                ))
                .await;

            Err(format!(
                "Process timed out after {}s. Partial output:\n{}",
                config.timeout_secs, stdout_output
            ))
        }
    }
}
