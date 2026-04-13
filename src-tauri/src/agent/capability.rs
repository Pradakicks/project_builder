use crate::agent::runner::resolve_api_key;
use crate::db::Database;
use crate::models::GoalRunStatus;

// ---------------------------------------------------------------------------
// Public structs
// ---------------------------------------------------------------------------

pub struct EngineCapability {
    pub name: String,
    pub available: bool,
    pub reason: Option<String>,
}

pub struct WorkingDirectoryState {
    pub configured: bool,
    pub path: Option<String>,
    pub exists: bool,
    pub is_git_repo: bool,
    /// Relative paths of source files in the working directory, capped at 50.
    pub existing_source_files: Vec<String>,
}

pub struct RuntimeCapability {
    pub configured: bool,
    pub run_command: Option<String>,
    pub app_url: Option<String>,
    pub verify_command: Option<String>,
}

pub struct VerificationCapability {
    pub supported: bool,
    pub verify_command: Option<String>,
    pub app_url: Option<String>,
}

pub struct LatestFailureSummary {
    pub phase: String,
    pub status: String,
    pub retry_count: i64,
    pub fingerprint: Option<String>,
    pub summary: Option<String>,
    pub blocker_reason: Option<String>,
    pub attention_required: bool,
}

pub struct CapabilitySnapshot {
    pub execution_engines: Vec<EngineCapability>,
    pub working_directory: WorkingDirectoryState,
    pub runtime: RuntimeCapability,
    pub verification: VerificationCapability,
    pub latest_failure: Option<LatestFailureSummary>,
    pub configured_providers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn binary_on_path(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Walk `root` recursively and collect relative source file paths.
/// Caps at 50 entries. Skips common non-source directories.
fn collect_source_files(root: &str) -> Vec<String> {
    const MAX_FILES: usize = 50;
    const SKIP_DIRS: &[&str] = &[
        "node_modules",
        ".git",
        "target",
        "dist",
        "build",
        ".next",
        ".venv",
        "__pycache__",
        ".claude",
    ];
    const SOURCE_EXTS: &[&str] = &[
        "ts", "tsx", "js", "jsx", "rs", "py", "go", "svelte", "vue", "html", "css",
    ];

    let mut files: Vec<String> = Vec::new();
    let root_path = std::path::Path::new(root);

    fn walk(
        dir: &std::path::Path,
        root: &std::path::Path,
        skip: &[&str],
        exts: &[&str],
        out: &mut Vec<String>,
        limit: usize,
    ) {
        if out.len() >= limit {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if out.len() >= limit {
                break;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                if !skip.iter().any(|s| name_str == *s) {
                    walk(&path, root, skip, exts, out, limit);
                }
            } else if let Some(ext) = path.extension() {
                if exts.iter().any(|e| ext == std::ffi::OsStr::new(e)) {
                    if let Ok(rel) = path.strip_prefix(root) {
                        out.push(rel.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    walk(root_path, root_path, SKIP_DIRS, SOURCE_EXTS, &mut files, MAX_FILES);
    files
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub fn build_capability_snapshot(db: &Database, project_id: &str) -> CapabilitySnapshot {
    let project = db.get_project(project_id).ok();
    let settings = project.as_ref().map(|p| &p.settings);

    // --- Execution engines ---
    let builtin_available = !resolve_api_key("claude").is_empty()
        || !resolve_api_key("openai").is_empty();
    let builtin = EngineCapability {
        name: "built-in".to_string(),
        available: builtin_available,
        reason: if builtin_available {
            None
        } else {
            Some("No API key configured for claude or openai".to_string())
        },
    };

    let claude_bin = binary_on_path("claude");
    let claude_code = EngineCapability {
        name: "claude-code".to_string(),
        available: claude_bin,
        reason: if claude_bin {
            None
        } else {
            Some("'claude' binary not found on PATH".to_string())
        },
    };

    let codex_bin = binary_on_path("codex");
    let codex_key = !resolve_api_key("openai").is_empty();
    let codex_available = codex_bin && codex_key;
    let codex_reason = if codex_available {
        None
    } else if !codex_bin {
        Some("'codex' binary not found on PATH".to_string())
    } else {
        Some("No API key configured for openai".to_string())
    };
    let codex = EngineCapability {
        name: "codex".to_string(),
        available: codex_available,
        reason: codex_reason,
    };

    // --- Working directory ---
    let wd = settings.and_then(|s| s.working_directory.clone());
    let wd_configured = wd.is_some();
    let wd_exists = wd
        .as_ref()
        .map(|p| std::path::Path::new(p).exists())
        .unwrap_or(false);
    let wd_is_git = wd
        .as_ref()
        .map(|p| std::path::Path::new(p).join(".git").exists())
        .unwrap_or(false);
    let source_files = if wd_exists {
        wd.as_deref().map(collect_source_files).unwrap_or_default()
    } else {
        Vec::new()
    };

    let working_directory = WorkingDirectoryState {
        configured: wd_configured,
        path: wd,
        exists: wd_exists,
        is_git_repo: wd_is_git,
        existing_source_files: source_files,
    };

    // --- Runtime ---
    let spec = settings.and_then(|s| s.runtime_spec.as_ref());
    let runtime = RuntimeCapability {
        configured: spec.is_some(),
        run_command: spec.map(|s| s.run_command.clone()),
        app_url: spec.and_then(|s| s.app_url.clone()),
        verify_command: spec.and_then(|s| s.verify_command.clone()),
    };

    // --- Verification ---
    let verification = VerificationCapability {
        supported: spec
            .map(|s| s.verify_command.is_some() || s.app_url.is_some() || s.port_hint.is_some())
            .unwrap_or(false),
        verify_command: spec.and_then(|s| s.verify_command.clone()),
        app_url: spec.and_then(|s| s.app_url.clone()),
    };

    // --- Latest failure ---
    let latest_failure = db
        .list_goal_runs(project_id)
        .ok()
        .and_then(|runs| runs.into_iter().next())
        .and_then(|goal_run| {
            let is_failure = matches!(
                goal_run.status,
                GoalRunStatus::Failed | GoalRunStatus::Blocked | GoalRunStatus::Interrupted
            );
            if !is_failure {
                return None;
            }
            let phase = serde_json::to_string(&goal_run.phase)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let status = serde_json::to_string(&goal_run.status)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            Some(LatestFailureSummary {
                phase,
                status,
                retry_count: goal_run.retry_count,
                fingerprint: goal_run.last_failure_fingerprint,
                summary: goal_run.last_failure_summary,
                blocker_reason: goal_run.blocker_reason,
                attention_required: goal_run.attention_required,
            })
        });

    // --- Configured providers ---
    let mut providers: Vec<String> = Vec::new();
    if let Some(s) = settings {
        for cfg in &s.llm_configs {
            let key = resolve_api_key(&cfg.provider);
            if !key.is_empty() && !providers.contains(&cfg.provider) {
                providers.push(cfg.provider.clone());
            }
        }
    }
    // Fallback probe
    if providers.is_empty() {
        for probe in &["claude", "openai"] {
            if !resolve_api_key(probe).is_empty() {
                let name = probe.to_string();
                if !providers.contains(&name) {
                    providers.push(name);
                }
            }
        }
    }

    CapabilitySnapshot {
        execution_engines: vec![builtin, claude_code, codex],
        working_directory,
        runtime,
        verification,
        latest_failure,
        configured_providers: providers,
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

pub fn render_capability_section(snapshot: &CapabilitySnapshot) -> String {
    let mut out = String::from("Capabilities:\n");

    // Execution engines
    out.push_str("  Execution engines:\n");
    for engine in &snapshot.execution_engines {
        if engine.available {
            if engine.name == "built-in" && !snapshot.configured_providers.is_empty() {
                out.push_str(&format!(
                    "    - {}: available (providers: {})\n",
                    engine.name,
                    snapshot.configured_providers.join(", ")
                ));
            } else {
                out.push_str(&format!("    - {}: available\n", engine.name));
            }
        } else {
            let reason = engine.reason.as_deref().unwrap_or("not available");
            out.push_str(&format!(
                "    - {}: NOT available — {}\n",
                engine.name, reason
            ));
        }
    }

    // Working directory
    let wd_display = snapshot
        .working_directory
        .path
        .as_deref()
        .unwrap_or("not configured");
    if snapshot.working_directory.configured {
        out.push_str(&format!(
            "  Working directory: {} (exists: {}, git repo: {})\n",
            wd_display,
            snapshot.working_directory.exists,
            snapshot.working_directory.is_git_repo
        ));
        if snapshot.working_directory.exists {
            if snapshot.working_directory.existing_source_files.is_empty() {
                out.push_str("  Source files: (empty repo — no source files found)\n");
            } else {
                let files = snapshot.working_directory.existing_source_files.join(", ");
                out.push_str(&format!("  Source files: {}\n", files));
            }
        }
    } else {
        out.push_str("  Working directory: not configured\n");
    }

    // Runtime
    if snapshot.runtime.configured {
        let mut parts = Vec::new();
        if let Some(rc) = &snapshot.runtime.run_command {
            parts.push(format!("run_command: {}", rc));
        }
        if let Some(vc) = &snapshot.runtime.verify_command {
            parts.push(format!("verify_command: {}", vc));
        }
        if let Some(url) = &snapshot.runtime.app_url {
            parts.push(format!("app_url: {}", url));
        }
        out.push_str(&format!(
            "  Runtime: configured — {}\n",
            if parts.is_empty() {
                "none".to_string()
            } else {
                parts.join(", ")
            }
        ));
    } else {
        out.push_str("  Runtime: not configured\n");
    }

    // Verification
    if snapshot.verification.supported {
        let mut parts = Vec::new();
        if let Some(vc) = &snapshot.verification.verify_command {
            parts.push(format!("verify_command: {}", vc));
        }
        if let Some(url) = &snapshot.verification.app_url {
            parts.push(format!("app_url: {}", url));
        }
        out.push_str(&format!(
            "  Verification: supported ({})\n",
            if parts.is_empty() {
                "none".to_string()
            } else {
                parts.join(", ")
            }
        ));
    } else {
        out.push_str("  Verification: not supported\n");
    }

    // Latest failure
    if let Some(f) = &snapshot.latest_failure {
        out.push_str(&format!(
            "  Goal-run failure: phase={}, status={}, retry_count: {}, fingerprint={}, attention_required={}\n",
            f.phase,
            f.status,
            f.retry_count,
            f.fingerprint.as_deref().unwrap_or("none"),
            f.attention_required
        ));
        if let Some(summary) = &f.summary {
            out.push_str(&format!("    Summary: {}\n", summary));
        }
        if let Some(blocker) = &f.blocker_reason {
            out.push_str(&format!("    Blocker: {}\n", blocker));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_capability_section_includes_all_subsections() {
        let snapshot = CapabilitySnapshot {
            execution_engines: vec![
                EngineCapability {
                    name: "built-in".to_string(),
                    available: true,
                    reason: None,
                },
                EngineCapability {
                    name: "claude-code".to_string(),
                    available: false,
                    reason: Some("'claude' binary not found on PATH".to_string()),
                },
                EngineCapability {
                    name: "codex".to_string(),
                    available: true,
                    reason: None,
                },
            ],
            working_directory: WorkingDirectoryState {
                configured: true,
                path: Some("/tmp/test-repo".to_string()),
                exists: true,
                is_git_repo: true,
                existing_source_files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            },
            runtime: RuntimeCapability {
                configured: true,
                run_command: Some("npm run dev".to_string()),
                app_url: Some("http://127.0.0.1:5173".to_string()),
                verify_command: Some("npm test".to_string()),
            },
            verification: VerificationCapability {
                supported: true,
                verify_command: Some("npm test".to_string()),
                app_url: Some("http://127.0.0.1:5173".to_string()),
            },
            latest_failure: Some(LatestFailureSummary {
                phase: "implementation".to_string(),
                status: "failed".to_string(),
                retry_count: 2,
                fingerprint: Some("implementation:npm-exit-1".to_string()),
                summary: Some("npm run dev exited with code 1".to_string()),
                blocker_reason: None,
                attention_required: true,
            }),
            configured_providers: vec!["claude".to_string()],
        };

        let rendered = render_capability_section(&snapshot);
        assert!(rendered.contains("Capabilities:"), "missing Capabilities header");
        assert!(rendered.contains("Execution engines:"), "missing engines subsection");
        assert!(rendered.contains("built-in"), "missing built-in engine");
        assert!(rendered.contains("claude-code"), "missing claude-code engine");
        assert!(rendered.contains("Working directory:"), "missing wd subsection");
        assert!(rendered.contains("exists: true"), "missing exists");
        assert!(rendered.contains("git repo: true"), "missing git repo");
        assert!(rendered.contains("Source files:"), "missing source files");
        assert!(rendered.contains("src/main.rs"), "missing source file entry");
        assert!(rendered.contains("Runtime:"), "missing runtime");
        assert!(rendered.contains("npm run dev"), "missing run command");
        assert!(rendered.contains("Verification:"), "missing verification");
        assert!(rendered.contains("Goal-run failure:"), "missing failure section");
        assert!(rendered.contains("retry_count: 2"), "missing retry count");
        assert!(rendered.contains("implementation:npm-exit-1"), "missing fingerprint");
    }
}
