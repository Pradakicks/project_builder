use crate::models::{Project, ProjectFile, ProjectSettings};
use crate::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::State;
use tracing::info;

fn slugify_project_name(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn run_git(args: &[&str], cwd: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if args.contains(&"commit")
        && (stderr.contains("Author identity unknown")
            || stderr.contains("unable to auto-detect email address"))
    {
        return Err(
            "Git user identity is not configured. Set git user.name and user.email, then create the project again.".to_string(),
        );
    }

    Err(format!("git {} failed: {stderr}", args.join(" ")))
}

fn bootstrap_repo(parent_directory: &str, project_name: &str) -> Result<PathBuf, String> {
    let parent = PathBuf::from(parent_directory);
    if !parent.exists() || !parent.is_dir() {
        return Err("Parent folder does not exist".to_string());
    }

    let slug = slugify_project_name(project_name);
    if slug.is_empty() {
        return Err("Project name must contain letters or numbers".to_string());
    }

    let repo_path = parent.join(slug);
    if repo_path.exists() {
        return Err(format!(
            "Project folder already exists: {}",
            repo_path.display()
        ));
    }

    std::fs::create_dir_all(&repo_path)
        .map_err(|e| format!("Failed to create project folder: {e}"))?;

    if let Err(error) = (|| -> Result<(), String> {
        run_git(&["init", "-b", "main"], &repo_path)?;
        run_git(&["commit", "--allow-empty", "-m", "Initial commit"], &repo_path)?;
        Ok(())
    })() {
        let _ = std::fs::remove_dir_all(&repo_path);
        return Err(error);
    }

    Ok(repo_path)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn create_project(
    state: State<AppState>,
    name: String,
    description: String,
    parent_directory: Option<String>,
) -> Result<Project, String> {
    let mut settings = ProjectSettings::default();
    let mut created_repo_path: Option<PathBuf> = None;

    if let Some(parent_directory) = parent_directory.as_deref() {
        let repo_path = bootstrap_repo(parent_directory, &name)?;
        settings.working_directory = Some(repo_path.to_string_lossy().to_string());
        created_repo_path = Some(repo_path);
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    match db.create_project_with_settings(&name, &description, settings) {
        Ok(project) => {
            info!(project_id = %project.id, name = %project.name, working_directory = ?project.settings.working_directory, "Created project");
            Ok(project)
        }
        Err(error) => {
            if let Some(path) = created_repo_path {
                let _ = std::fs::remove_dir_all(path);
            }
            Err(error)
        }
    }
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_project(state: State<AppState>, id: String) -> Result<Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_project(&id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_project(
    state: State<AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_project(&id, name.as_deref(), description.as_deref(), None, None)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_projects(state: State<AppState>) -> Result<Vec<Project>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_projects()
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_project(&id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn save_project_to_file(state: State<AppState>, id: String, path: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let project_file = db.export_project(&id)?;
    let json = serde_json::to_string_pretty(&project_file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn load_project_from_file(state: State<AppState>, path: String) -> Result<Project, String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let project_file: ProjectFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.import_project(&project_file)
}
