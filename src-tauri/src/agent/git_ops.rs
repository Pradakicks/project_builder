//! Git CLI wrappers for branch/commit lifecycle around external tool runs.
//! All operations use `tokio::process::Command` to call the `git` binary,
//! matching the pattern used in `external.rs` for spawning CLI tools.

use tokio::process::Command;
use tracing::{debug, info, warn};

/// Run a git command in the given working directory. Returns stdout on success.
async fn git(working_dir: &str, args: &[&str]) -> Result<String, String> {
    debug!(working_dir, args = ?args, "Running git command");
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
        warn!(args = ?args, stderr = %stderr, "Git command failed");
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
    info!(working_dir, branch, "Ensuring git branch");
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
    info!(sha = %sha, message, "Committed successfully");
    Ok(Some(sha))
}

/// Get the abbreviated HEAD commit SHA.
pub async fn get_head_sha(working_dir: &str) -> Result<String, String> {
    git(working_dir, &["rev-parse", "--short", "HEAD"]).await
}

/// List files on a branch (relative to repo root).
pub async fn list_branch_files(working_dir: &str, branch: &str) -> Result<String, String> {
    git(working_dir, &["ls-tree", "-r", "--name-only", branch]).await
}

/// Get a diff stat summary between two refs.
pub async fn diff_stat(working_dir: &str, since_ref: &str) -> Result<String, String> {
    git(working_dir, &["diff", "--stat", &format!("{since_ref}..HEAD")]).await
}

/// Checkout a specific branch (must already exist).
pub async fn checkout_branch(working_dir: &str, branch: &str) -> Result<(), String> {
    git(working_dir, &["checkout", branch]).await?;
    Ok(())
}

/// Attempt to merge a branch into the current branch without committing.
/// Returns Ok(true) if the merge is clean, Ok(false) if there are conflicts.
pub async fn try_merge(working_dir: &str, branch: &str) -> Result<bool, String> {
    info!(working_dir, branch, "Attempting merge");
    let output = Command::new("git")
        .current_dir(working_dir)
        .args(["merge", "--no-commit", "--no-ff", branch])
        .output()
        .await
        .map_err(|e| format!("Failed to run git merge: {e}"))?;

    if output.status.success() {
        Ok(true)
    } else {
        // Check if it's a conflict (exit code 1) vs a real error
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("CONFLICT") || stderr.contains("Automatic merge failed") {
            warn!(branch, "Merge conflict detected");
            Ok(false)
        } else if !list_conflict_files(working_dir).await.unwrap_or_default().is_empty() {
            warn!(branch, "Merge conflict detected from unresolved files");
            Ok(false)
        } else {
            Err(format!("git merge failed: {stderr}"))
        }
    }
}

/// List files with merge conflicts (only valid during an active merge conflict).
pub async fn list_conflict_files(working_dir: &str) -> Result<Vec<String>, String> {
    let output = git(working_dir, &["diff", "--name-only", "--diff-filter=U"]).await?;
    Ok(output.lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect())
}

/// Get the full diff including conflict markers (only valid during an active merge conflict).
pub async fn get_conflict_diff(working_dir: &str) -> Result<String, String> {
    git(working_dir, &["diff"]).await
}

/// Abort an in-progress merge, restoring the previous state.
pub async fn abort_merge(working_dir: &str) -> Result<(), String> {
    warn!(working_dir, "Aborting merge");
    git(working_dir, &["merge", "--abort"]).await?;
    Ok(())
}

/// Complete a pending merge by staging all changes and committing.
/// Returns the commit SHA.
pub async fn complete_merge(working_dir: &str, message: &str) -> Result<String, String> {
    info!(working_dir, message, "Completing merge");
    git(working_dir, &["add", "-A"]).await?;
    git(working_dir, &["commit", "-m", message]).await?;
    get_head_sha(working_dir).await
}

/// Get diff stat between two specific refs (not relative to HEAD).
pub async fn diff_stat_between(working_dir: &str, ref_a: &str, ref_b: &str) -> Result<String, String> {
    git(working_dir, &["diff", "--stat", &format!("{ref_a}..{ref_b}")]).await
}
