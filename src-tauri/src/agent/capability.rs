/// Capability snapshot types and rendering for the CTO prompt.

#[derive(Debug, Clone)]
pub struct WorkingDirectoryState {
    pub configured: bool,
    pub path: Option<String>,
    pub exists: bool,
    pub is_git_repo: bool,
    pub existing_source_files: Vec<String>, // relative paths, capped at 50
}

#[derive(Debug, Clone)]
pub struct CapabilitySnapshot {
    pub working_directory: WorkingDirectoryState,
}

/// Collect source files under `root`, skipping common non-source dirs, capped at 50.
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

/// Build a `CapabilitySnapshot` from an optional working directory path.
pub fn build_capability_snapshot(working_directory_path: Option<&str>) -> CapabilitySnapshot {
    let wd_configured = working_directory_path
        .map(|p| !p.trim().is_empty())
        .unwrap_or(false);

    let wd_path = if wd_configured {
        working_directory_path.map(|s| s.to_string())
    } else {
        None
    };

    let wd_exists = wd_path
        .as_deref()
        .map(|p| std::path::Path::new(p).exists())
        .unwrap_or(false);

    let wd_is_git = if wd_exists {
        wd_path
            .as_deref()
            .map(|p| std::path::Path::new(p).join(".git").exists())
            .unwrap_or(false)
    } else {
        false
    };

    let source_files = if wd_exists {
        wd_path
            .as_deref()
            .map(collect_source_files)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    CapabilitySnapshot {
        working_directory: WorkingDirectoryState {
            configured: wd_configured,
            path: wd_path,
            exists: wd_exists,
            is_git_repo: wd_is_git,
            existing_source_files: source_files,
        },
    }
}

/// Render the capability snapshot into a human-readable section for injection into prompts.
pub fn render_capability_section(snapshot: &CapabilitySnapshot) -> String {
    let mut out = String::from("Working directory capabilities:\n");

    let wd_display = snapshot
        .working_directory
        .path
        .as_deref()
        .unwrap_or("(none)");

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

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_capability_section_includes_all_subsections() {
        let snapshot = CapabilitySnapshot {
            working_directory: WorkingDirectoryState {
                configured: true,
                path: Some("/tmp/test-project".to_string()),
                exists: true,
                is_git_repo: false,
                existing_source_files: vec!["src/main.rs".to_string()],
            },
        };

        let rendered = render_capability_section(&snapshot);
        assert!(rendered.contains("Working directory:"), "missing working directory");
        assert!(rendered.contains("Source files:"), "missing source files");
        assert!(rendered.contains("src/main.rs"), "missing source file entry");
    }

    #[test]
    fn render_capability_section_not_configured() {
        let snapshot = CapabilitySnapshot {
            working_directory: WorkingDirectoryState {
                configured: false,
                path: None,
                exists: false,
                is_git_repo: false,
                existing_source_files: vec![],
            },
        };

        let rendered = render_capability_section(&snapshot);
        assert!(rendered.contains("not configured"), "should say not configured");
    }

    #[test]
    fn render_capability_section_empty_repo() {
        let snapshot = CapabilitySnapshot {
            working_directory: WorkingDirectoryState {
                configured: true,
                path: Some("/tmp/empty-project".to_string()),
                exists: true,
                is_git_repo: false,
                existing_source_files: vec![],
            },
        };

        let rendered = render_capability_section(&snapshot);
        assert!(
            rendered.contains("empty repo"),
            "should indicate empty repo"
        );
    }
}
