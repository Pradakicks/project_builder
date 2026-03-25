//! Git CLI wrappers for branch/commit lifecycle around external tool runs.
//! All operations use `tokio::process::Command` to call the `git` binary,
//! matching the pattern used in `external.rs` for spawning CLI tools.

use tokio::process::Command;

/// Run a git command in the given working directory. Returns stdout on success.
async fn git(working_dir: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(working_dir)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("git {} failed: {stderr}", args.join(" ")))
    }
}

/// Slugify a piece name into a valid git branch name.
/// "Auth Service (v2)" → "piece/auth-service-v2"
pub fn slugify_branch_name(piece_name: &str) -> String {
    let slug: String = piece_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse multiple hyphens and trim
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    let trimmed = result.trim_end_matches('-');
    format!("piece/{trimmed}")
}

/// Get the current branch name.
pub async fn current_branch(working_dir: &str) -> Result<String, String> {
    git(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

/// Check if a branch exists locally.
pub async fn branch_exists(working_dir: &str, branch: &str) -> Result<bool, String> {
    let result = git(
        working_dir,
        &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")],
    )
    .await;
    match result {
        Ok(_) => Ok(true),
        Err(e) if e.contains("failed") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Check if the working tree has uncommitted changes.
pub async fn has_uncommitted_changes(working_dir: &str) -> Result<bool, String> {
    let output = git(working_dir, &["status", "--porcelain"]).await?;
    Ok(!output.is_empty())
}

/// Create and checkout a new branch, or checkout existing one.
pub async fn ensure_branch(working_dir: &str, branch: &str) -> Result<(), String> {
    if branch_exists(working_dir, branch).await? {
        git(working_dir, &["checkout", branch]).await?;
    } else {
        git(working_dir, &["checkout", "-b", branch]).await?;
    }
    Ok(())
}

/// Stage all changes and commit with a message. Returns Some(sha) if committed,
/// None if there was nothing to commit.
pub async fn stage_and_commit(
    working_dir: &str,
    message: &str,
) -> Result<Option<String>, String> {
    // Check if there's anything to commit
    if !has_uncommitted_changes(working_dir).await? {
        return Ok(None);
    }

    git(working_dir, &["add", "-A"]).await?;
    git(working_dir, &["commit", "-m", message]).await?;

    let sha = get_head_sha(working_dir).await?;
    Ok(Some(sha))
}

/// Get the abbreviated HEAD commit SHA.
pub async fn get_head_sha(working_dir: &str) -> Result<String, String> {
    git(working_dir, &["rev-parse", "--short", "HEAD"]).await
}

/// Get a diff stat summary between two refs.
pub async fn diff_stat(working_dir: &str, since_ref: &str) -> Result<String, String> {
    git(working_dir, &["diff", "--stat", &format!("{since_ref}..HEAD")]).await
}
