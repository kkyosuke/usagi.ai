use crate::domain::project::{ProjectState, Worktree};
use crate::infrastructure::{git, global_registry};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Initialises a project at the requested path without changing the process directory.
pub fn run(repository_url: &str, directory: Option<PathBuf>, branch: Option<String>) -> Result<()> {
    let project_path = directory.unwrap_or(std::env::current_dir()?);
    fs::create_dir_all(&project_path).context("Failed to create target directory")?;
    let project_path = project_path
        .canonicalize()
        .context("Failed to resolve target directory")?;
    initialize_project(repository_url, &project_path, branch.as_deref())?;
    global_registry::register_project_at(&project_path)?;
    println!("Project initialized successfully.");
    Ok(())
}

fn initialize_project(
    repository_url: &str,
    project_path: &Path,
    branch: Option<&str>,
) -> Result<()> {
    println!("Initializing repository: {}", repository_url);

    let usagi_dir = project_path.join(".usagi");
    if usagi_dir.join("state.json").exists() {
        bail!("This project is already initialized.");
    }

    let main_dir = project_path.join("main");
    if !main_dir.exists() {
        println!("Cloning repository into main/...");
        git::clone(repository_url, &main_dir, branch)?;
    } else {
        println!("Warning: main/ directory already exists. Skipping clone.");
    }

    let current_branch = git::get_current_branch(&main_dir)?;

    let mut state = ProjectState {
        version: 1,
        initialized: true,
        worktrees: vec![Worktree {
            branch: current_branch.clone(),
            directory: "main".to_string(),
            default: true,
            modified_at: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
            status: crate::domain::project::SessionStatus::Todo,
            has_upstream: git::has_upstream(&main_dir).unwrap_or(false),
        }],
        current_worktree: Some(current_branch),
        last_updated: None,
        ai_model: None,
    };
    state.update_last_updated();
    let config_path = project_path.join("usagi.config");
    if !config_path.exists() {
        let config_content = format!(
            "# usagi project configuration\nrepository_url = \"{}\"\n",
            repository_url
        );
        fs::write(config_path, config_content).context("Failed to write usagi.config")?;
    }

    let gitignore_path = project_path.join(".gitignore");
    let mut gitignore_content = String::new();
    if gitignore_path.exists() {
        gitignore_content =
            fs::read_to_string(&gitignore_path).context("Failed to read existing .gitignore")?;
    }
    if !gitignore_content
        .lines()
        .any(|line| line.trim() == ".usagi/")
    {
        if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
            gitignore_content.push('\n');
        }
        gitignore_content.push_str(".usagi/\n");
        fs::write(gitignore_path, gitignore_content).context("Failed to write .gitignore")?;
    }

    // Write the initialization marker only after the clone and local files succeed.
    fs::create_dir_all(&usagi_dir).context("Failed to create .usagi directory")?;
    crate::infrastructure::project_state::save_project_state(project_path, &state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::project_state::get_project_state;
    use crate::test_support::{init_repo, TestDir};

    #[test]
    fn failed_clone_can_be_retried_without_changing_current_directory() -> Result<()> {
        let dir = TestDir::new();
        let project = dir.path().join("project");
        let remote = dir.path().join("remote");
        let original_dir = std::env::current_dir()?;
        assert!(run(remote.to_str().unwrap(), Some(project.clone()), None).is_err());
        assert_eq!(std::env::current_dir()?, original_dir);
        assert!(!project.join(".usagi/state.json").exists());

        init_repo(&remote, "develop");
        // Also recover from the incomplete marker left by older versions.
        fs::create_dir_all(project.join(".usagi"))?;
        fs::write(project.join(".gitignore"), "# exclude .usagi/ later")?;
        initialize_project(remote.to_str().unwrap(), &project, None)?;
        let state = get_project_state(&project)?;
        assert_eq!(state.current_worktree.as_deref(), Some("develop"));
        assert_eq!(state.worktrees[0].directory, "main");
        assert!(fs::read_to_string(project.join(".gitignore"))?
            .lines()
            .any(|line| line == ".usagi/"));
        assert_eq!(std::env::current_dir()?, original_dir);
        let before = fs::read(project.join(".usagi/state.json"))?;
        assert!(initialize_project(remote.to_str().unwrap(), &project, None).is_err());
        assert_eq!(fs::read(project.join(".usagi/state.json"))?, before);
        Ok(())
    }
}
