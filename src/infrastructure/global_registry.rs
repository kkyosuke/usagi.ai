use crate::domain::usagi::Repositories;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the usagi application data directory.
fn data_dir() -> Result<PathBuf> {
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?;
    Ok(base_dirs.home_dir().join(".usagi"))
}

/// Reads the list of registered repositories from the global usagi data directory.
pub fn get_repositories() -> Result<Vec<PathBuf>> {
    let repo_json_path = data_dir()?.join("repositories.json");

    if repo_json_path.exists() {
        let content =
            fs::read_to_string(&repo_json_path).context("Failed to read repositories.json")?;
        let repos: Repositories =
            serde_json::from_str(&content).context("Failed to parse repositories.json")?;
        Ok(repos.repositories)
    } else {
        Ok(vec![])
    }
}

/// Persists the list of registered repositories to the global usagi data directory.
#[allow(dead_code)]
pub fn save_repositories(repos: &[PathBuf]) -> Result<()> {
    let dir = data_dir()?;
    fs::create_dir_all(&dir).context("Failed to create data directory")?;

    let repo_json_path = dir.join("repositories.json");
    let repos_struct = Repositories {
        repositories: repos.to_vec(),
    };
    let content =
        serde_json::to_string_pretty(&repos_struct).context("Failed to serialize repositories")?;
    fs::write(repo_json_path, content).context("Failed to write repositories.json")?;

    Ok(())
}

/// Adds the current directory to the global repository registry if not already present.
pub fn register_project() -> Result<()> {
    register_project_at(&std::env::current_dir().context("Failed to get current directory")?)
}

/// Adds an explicit project path without changing the process working directory.
pub fn register_project_at(project_path: &Path) -> Result<()> {
    let current_dir = project_path
        .canonicalize()
        .context("Failed to resolve project directory")?;
    let dir = data_dir()?;
    fs::create_dir_all(&dir).context("Failed to create data directory")?;

    let repo_json_path = dir.join("repositories.json");
    let mut repos = if repo_json_path.exists() {
        let content =
            fs::read_to_string(&repo_json_path).context("Failed to read repositories.json")?;
        serde_json::from_str::<Repositories>(&content)
            .context("Failed to parse repositories.json")?
    } else {
        Repositories::default()
    };

    if !repos.repositories.contains(&current_dir) {
        repos.repositories.push(current_dir);
        let content =
            serde_json::to_string_pretty(&repos).context("Failed to serialize repositories")?;
        fs::write(repo_json_path, content).context("Failed to write repositories.json")?;
    }

    Ok(())
}
