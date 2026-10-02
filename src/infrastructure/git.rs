//! Git Infrastructure Module
//!
//! # Architecture Policy
//! This module uses two different approaches for interacting with Git, depending on the operation:
//!
//! 1. **System `git` Command Line (Network & State Mutation)**
//!    - Used for: `clone`, `fetch`, `push`, `worktree` operations, `rebase`, etc.
//!    - Reason: Operations that involve network communication (especially SSH) require complex handling of credentials,
//!      passphrases, `~/.ssh/config`, and `ssh-agent`. Attempting to handle all these edge cases securely and reliably
//!      using `git2` is complicated and prone to failure. Delegating to the native `git` CLI ensures that the user's
//!      existing SSH and Git configurations are respected automatically.
//!
//! 2. **`git2` Library (Local Inspection & Read-only)**
//!    - Used for: `get_current_branch`, `has_upstream`, etc.
//!    - Reason: Reading repository states locally using `git2` provides type safety and better performance compared
//!      to spawning subprocesses and parsing string outputs.
//!

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command as ProcessCommand;

/// Clones a repository into `target`, optionally checking out `branch`.
pub fn clone(url: &str, target: &Path, branch: Option<&str>) -> Result<()> {
    let mut command = ProcessCommand::new("git");
    command.arg("clone");
    if let Some(b) = branch {
        command.arg("-b").arg(b);
    }
    command.arg("--").arg(url).arg(target);

    let status = command.status().context("Failed to execute git clone")?;
    if !status.success() {
        return Err(anyhow!("git clone failed."));
    }
    Ok(())
}

/// Creates a new git worktree at `worktree_path` based on `base_branch`.
pub fn create_worktree(
    project_path: &Path,
    branch: &str,
    worktree_path: &Path,
    base_branch: &str,
    track: bool,
) -> Result<()> {
    let mut command = ProcessCommand::new("git");
    command
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("worktree")
        .arg("add");

    command.arg(if track { "--track" } else { "--no-track" });

    let status = command
        .arg("-b")
        .arg(branch)
        .arg(worktree_path)
        .arg(base_branch)
        .status()
        .context("Failed to execute git worktree add")?;

    if !status.success() {
        return Err(anyhow!("git worktree add failed."));
    }
    Ok(())
}

/// Removes a git worktree at `worktree_path`.
pub fn remove_worktree(project_path: &Path, worktree_path: &Path) -> Result<()> {
    let status = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("worktree")
        .arg("remove")
        .arg(worktree_path)
        .status()
        .context("Failed to execute git worktree remove")?;

    if !status.success() {
        return Err(anyhow!("git worktree remove failed."));
    }
    Ok(())
}

/// Deletes a git branch.
pub fn delete_branch(project_path: &Path, branch: &str) -> Result<()> {
    let status = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("branch")
        .arg("-D")
        .arg(branch)
        .status()
        .context("Failed to execute git branch -D")?;

    if !status.success() {
        return Err(anyhow!("git branch -D failed."));
    }
    Ok(())
}

/// Returns `true` if `branch` already exists in the repository under `project_path/main`.
pub fn branch_exists(branch: &str, project_path: &Path) -> Result<bool> {
    let repo = open_repository(&project_path.join("main"))?;
    let result = match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(_) => Ok(true),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(error) => Err(error).context("Failed to look up branch"),
    };
    result
}

/// Finds the remote of a remote-tracking branch without mistaking local branch
/// names such as `feature/login` for a remote named `feature`.
pub fn get_branch_remote(project_path: &Path, branch: &str) -> Result<Option<String>> {
    let repo = open_repository(&project_path.join("main"))?;
    if repo.find_branch(branch, git2::BranchType::Local).is_ok() {
        return Ok(None);
    }
    let remote_branch = match repo.find_branch(branch, git2::BranchType::Remote) {
        Ok(branch) => branch,
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            // A newly published branch may not have a local tracking ref yet.
            // Only fetch a prefix that is an actual configured remote.
            let remotes = repo.remotes()?;
            return Ok(remotes
                .iter()
                .flatten()
                .filter(|remote| {
                    branch
                        .strip_prefix(*remote)
                        .is_some_and(|suffix| suffix.starts_with('/') && suffix.len() > 1)
                })
                .max_by_key(|remote| remote.len())
                .map(str::to_string));
        }
        Err(error) => return Err(error).context("Failed to look up remote branch"),
    };
    let name = remote_branch
        .get()
        .name()
        .context("Remote branch name is not UTF-8")?;
    let remote = repo.branch_remote_name(name)?;
    Ok(Some(
        remote
            .as_str()
            .context("Remote name is not UTF-8")?
            .to_string(),
    ))
}

/// Returns the default remote branch (e.g. `origin/main`) for the repo at `project_path/main`.
pub fn get_default_branch(project_path: &Path) -> Result<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("symbolic-ref")
        .arg("refs/remotes/origin/HEAD")
        .output()
        .context("Failed to get default branch")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(branch) = stdout.trim().strip_prefix("refs/remotes/") {
            return Ok(branch.to_string());
        }
    }

    Ok("origin/main".to_string())
}

/// Fetches the latest changes from the specified remote.
pub fn fetch(project_path: &Path, remote: &str) -> Result<()> {
    let status = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("fetch")
        .arg(remote)
        .status()
        .context(format!("Failed to execute git fetch {}", remote))?;

    if !status.success() {
        return Err(anyhow!("git fetch {} failed.", remote));
    }
    Ok(())
}

/// Rebases the current branch of the worktree at `worktree_path` onto `base_branch`.
pub fn rebase(worktree_path: &Path, base_branch: &str) -> Result<()> {
    let status = ProcessCommand::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("rebase")
        .arg(base_branch)
        .status()
        .context(format!("Failed to execute git rebase {}", base_branch))?;

    if !status.success() {
        return Err(anyhow!(
            "git rebase {} failed in {}.",
            base_branch,
            worktree_path.display()
        ));
    }
    Ok(())
}

/// Returns the current branch name of the repository at `repo_path`.
pub fn get_current_branch(repo_path: &Path) -> Result<String> {
    let repo = open_repository(repo_path)?;
    let head = repo.head().context("Failed to get HEAD")?;
    if !head.is_branch() {
        return Err(anyhow!("HEAD is detached; check out a branch first"));
    }
    let branch = head
        .shorthand()
        .ok_or_else(|| anyhow!("HEAD is not a branch"))?;
    Ok(branch.to_string())
}

fn open_repository(repo_path: &Path) -> Result<git2::Repository> {
    let repo = git2::Repository::open_ext(
        repo_path,
        git2::RepositoryOpenFlags::NO_SEARCH,
        &[] as &[&std::ffi::OsStr],
    )
    .context(format!(
        "Failed to open repository at {}",
        repo_path.display()
    ))?;
    Ok(repo)
}

/// Returns `true` if the branch at `repo_path` has an upstream branch.
pub fn has_upstream(repo_path: &Path) -> Result<bool> {
    let repo = match git2::Repository::open_ext(
        repo_path,
        git2::RepositoryOpenFlags::NO_SEARCH,
        &[] as &[&std::ffi::OsStr],
    ) {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };

    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(false),
    };

    if !head.is_branch() {
        return Ok(false);
    }

    let branch = git2::Branch::wrap(head);
    let has_upstream = branch.upstream().is_ok();
    Ok(has_upstream)
}

/// Returns a list of remote branches.
pub fn list_remote_branches(project_path: &Path) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(project_path.join("main"))
        .arg("branch")
        .arg("-r")
        .output()
        .context("Failed to execute git branch -r")?;

    if !output.status.success() {
        return Err(anyhow!("git branch -r failed."));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches = stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.contains("->"))
        .map(|line| line.to_string())
        .collect();

    Ok(branches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git as run_git, init_repo, TestDir};

    #[test]
    fn test_has_upstream_no_search_upwards() -> Result<()> {
        let dir = TestDir::new();
        let parent = dir.path().join("parent");
        init_repo(&parent, "main");
        run_git(
            &parent,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/repo.git",
            ],
        );
        run_git(&parent, &["config", "branch.main.remote", "origin"]);
        run_git(&parent, &["config", "branch.main.merge", "refs/heads/main"]);
        run_git(&parent, &["update-ref", "refs/remotes/origin/main", "main"]);
        assert!(has_upstream(&parent)?);
        let child = parent.join("child");
        std::fs::create_dir(&child)?;
        assert!(!has_upstream(&child)?);
        Ok(())
    }

    #[test]
    fn test_create_worktree_tracking() -> Result<()> {
        let dir = TestDir::new();
        let remote = dir.path().join("remote");
        init_repo(&remote, "main");
        let project = dir.path().join("project");
        std::fs::create_dir(&project)?;
        let main = project.join("main");
        clone(remote.to_str().unwrap(), &main, None)?;
        assert_eq!(
            get_branch_remote(&project, "origin/main")?.as_deref(),
            Some("origin")
        );
        assert_eq!(
            get_branch_remote(&project, "origin/new")?.as_deref(),
            Some("origin")
        );
        assert_eq!(get_branch_remote(&project, "feature/local")?, None);
        // An explicit tracking request must override the user's Git preference.
        run_git(&main, &["config", "branch.autoSetupMerge", "false"]);
        let tracked = project.join("tracked");
        create_worktree(&project, "tracked", &tracked, "origin/main", true)?;
        assert!(has_upstream(&tracked)?);
        let untracked = project.join("untracked");
        create_worktree(&project, "untracked", &untracked, "origin/main", false)?;
        assert!(!has_upstream(&untracked)?);
        Ok(())
    }

    #[test]
    fn branch_lookup_is_exact_and_reports_missing_repository() -> Result<()> {
        let dir = TestDir::new();
        assert!(branch_exists("main", dir.path()).is_err());
        init_repo(&dir.path().join("main"), "main");
        assert!(branch_exists("main", dir.path())?);
        assert!(!branch_exists("missing", dir.path())?);
        assert!(branch_exists("m*", dir.path()).is_err());
        Ok(())
    }

    #[test]
    fn detached_head_is_not_a_branch() {
        let dir = TestDir::new();
        init_repo(dir.path(), "main");
        run_git(dir.path(), &["checkout", "--detach"]);
        assert!(get_current_branch(dir.path()).is_err());
    }
}
