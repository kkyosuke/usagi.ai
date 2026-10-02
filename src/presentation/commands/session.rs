use crate::domain::project::Worktree;
use crate::infrastructure::git;
use crate::infrastructure::project_state::{get_project_state, save_project_state};
use crate::presentation::cli::hop::app::{InputModal, SelectModal};
use crate::presentation::commands::{Command, CommandAction, CommandContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::{CommandFactory, Parser, Subcommand};
use console::style;
use console::Term;
use std::path::Path;

pub struct SessionCommand;

const NAME: &str = "session";
const DESCRIPTION: &str = "Manage sessions";
const HELP: &str = "Manages sessions (new working branches and worktrees).
Usage: session start [branch_name] [--base <base_branch>] [--remote]
       session close [branch_name]
       session update [--all] [--base <base_branch>]
       session status <branch_name> <status>
Closes and removes an existing session (removes worktree and deletes the local branch).
If branch_name is omitted, a list of available sessions will be displayed to choose from.";

#[async_trait]
impl Command for SessionCommand {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn help(&self) -> &str {
        HELP
    }

    fn is_match(&self, parts: &[String]) -> bool {
        let has_remote = parts.iter().any(|p| p == "--remote" || p == "-r");
        let has_base = parts.iter().any(|p| p == "--base" || p == "-b");

        let is_session_start_remote = parts.len() >= 2
            && parts[0] == "session"
            && parts[1] == "start"
            && has_remote
            && !has_base;
        let is_session_start_interactive =
            parts.len() == 2 && parts[0] == "session" && parts[1] == "start" && !has_remote;
        let is_session_close_interactive =
            parts.len() == 2 && parts[0] == "session" && parts[1] == "close";

        is_session_start_remote
            || is_session_start_interactive
            || is_session_close_interactive
            || parts.get(0).map_or(false, |name| name == self.name())
    }

    fn should_close_command_mode(&self, parts: &[String]) -> bool {
        parts.get(1).map_or(false, |p| p == "close")
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandAction> {
        let parts = context.parts;
        let has_remote = parts.iter().any(|p| p == "--remote" || p == "-r");
        let has_base = parts.iter().any(|p| p == "--base" || p == "-b");
        let cmd_to_execute = parts.join(" ");
        let selected_worktree = context.worktrees[context.selected_index].clone();
        let project_path = context.project_path.clone();

        let is_session_start_remote = parts.len() >= 2
            && parts[0] == "session"
            && parts[1] == "start"
            && has_remote
            && !has_base;
        if is_session_start_remote {
            let _ = crate::infrastructure::git::fetch(&project_path, "origin");
            let remote_branches = crate::infrastructure::git::list_remote_branches(&project_path)?;

            if remote_branches.is_empty() {
                return Ok(CommandAction::DisplayMessage(format!(
                    "{}",
                    style("No remote branches found.").red()
                )));
            } else {
                let parts_clone = parts.clone();
                let selected_worktree_clone = selected_worktree.clone();
                return Ok(CommandAction::SetSelectModal(SelectModal {
                    title: "Select base branch from remote".to_string(),
                    items: remote_branches,
                    selected_index: 0,
                    on_select: Box::new(move |app, selected| {
                        Box::pin(async move {
                            let mut new_parts = parts_clone;
                            let has_branch = new_parts.len() >= 3;

                            new_parts.push("--base".to_string());
                            new_parts.push(selected.clone());

                            if has_branch {
                                let new_input = new_parts.join(" ");
                                let (result, is_long_running, command) = app
                                    .run_command_with_parts(new_parts.clone(), &new_input)
                                    .await;
                                app.finalize_command_execution(
                                    result,
                                    &selected_worktree_clone,
                                    &new_input,
                                    &new_input,
                                    0,
                                    is_long_running,
                                    &new_parts,
                                    command,
                                );
                            } else {
                                app.input_modal = Some(InputModal {
                                    title: "Enter session branch name:".to_string(),
                                    value: selected
                                        .split('/')
                                        .last()
                                        .unwrap_or(&selected)
                                        .to_string(),
                                    on_submit: Box::new(move |app, branch_name| {
                                        Box::pin(async move {
                                            new_parts.push(branch_name);
                                            let new_input = new_parts.join(" ");
                                            let (result, is_long_running, command) = app
                                                .run_command_with_parts(
                                                    new_parts.clone(),
                                                    &new_input,
                                                )
                                                .await;
                                            app.finalize_command_execution(
                                                result,
                                                &selected_worktree_clone,
                                                &new_input,
                                                &new_input,
                                                0,
                                                is_long_running,
                                                &new_parts,
                                                command,
                                            );
                                            Ok(())
                                        })
                                    }),
                                });
                            }
                            Ok(())
                        })
                    }),
                }));
            }
        }

        let is_session_start_interactive =
            parts.len() == 2 && parts[0] == "session" && parts[1] == "start" && !has_remote;
        if is_session_start_interactive {
            let selected_worktree_clone = selected_worktree.clone();
            return Ok(CommandAction::SetInputModal(InputModal {
                title: "Enter session branch name:".to_string(),
                value: "".to_string(),
                on_submit: Box::new(move |app, branch_name| {
                    Box::pin(async move {
                        let new_input = format!("session start {}", branch_name);
                        let new_parts =
                            vec!["session".to_string(), "start".to_string(), branch_name];
                        let (result, is_long_running, command) = app
                            .run_command_with_parts(new_parts.clone(), &new_input)
                            .await;
                        app.finalize_command_execution(
                            result,
                            &selected_worktree_clone,
                            &new_input,
                            &new_input,
                            0,
                            is_long_running,
                            &new_parts,
                            command,
                        );
                        Ok(())
                    })
                }),
            }));
        }

        let is_session_close_interactive =
            parts.len() == 2 && parts[0] == "session" && parts[1] == "close";
        if is_session_close_interactive {
            let session_branches: Vec<String> = context
                .state
                .worktrees
                .iter()
                .filter(|w| !w.default)
                .map(|w| w.branch.clone())
                .collect();

            if session_branches.is_empty() {
                return Ok(CommandAction::DisplayMessage(format!(
                    "{}",
                    style("No sessions available to close.").red()
                )));
            } else {
                let selected_worktree_clone = selected_worktree.clone();
                return Ok(CommandAction::SetSelectModal(SelectModal {
                    title: "Select session branch to close".to_string(),
                    items: session_branches,
                    selected_index: 0,
                    on_select: Box::new(move |app, selected| {
                        Box::pin(async move {
                            let new_input = format!("session close {}", selected);
                            let new_parts =
                                vec!["session".to_string(), "close".to_string(), selected];
                            let (result, is_long_running, command) = app
                                .run_command_with_parts(new_parts.clone(), &new_input)
                                .await;
                            app.finalize_command_execution(
                                result,
                                &selected_worktree_clone,
                                &new_input,
                                &new_input,
                                0,
                                is_long_running,
                                &new_parts,
                                command,
                            );
                            Ok(())
                        })
                    }),
                }));
            }
        }

        Ok(CommandAction::RunCommand {
            parts,
            cmd_to_execute,
            close_after: false,
        })
    }

    async fn run(
        &self,
        args: Vec<String>,
        project_path: &Path,
        _current_worktree: &str,
        _term: &Term,
    ) -> Result<String> {
        let cli = match SessionCli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(e) => {
                return Err(anyhow!("{}", e));
            }
        };

        match cli.command {
            Some(SessionCommands::Start {
                branch,
                base,
                remote,
            }) => start_session(branch, base, remote, project_path),
            Some(SessionCommands::Close { branch }) => close_session(branch, project_path),
            Some(SessionCommands::Update { all, base }) => update_sessions(all, base, project_path),
            Some(SessionCommands::Status { branch, status }) => {
                update_session_status(&branch, &status, project_path)
            }
            None => {
                let mut cmd = SessionCli::command();
                let help = cmd.render_help().to_string();
                return Err(anyhow!("Usage:\n{}", help));
            }
        }
    }

    fn subcommands(&self) -> Vec<(String, String)> {
        let cmd = SessionCli::command();
        cmd.get_subcommands()
            .map(|sub| {
                (
                    sub.get_name().to_string(),
                    sub.get_about().map(|a| a.to_string()).unwrap_or_default(),
                )
            })
            .collect()
    }

    fn usage(&self, args: &[&str]) -> Option<String> {
        let cmd = SessionCli::command();
        if args.len() > 1 {
            let sub_name = args[1];
            if let Some(sub) = cmd.get_subcommands().find(|s| s.get_name() == sub_name) {
                let usage = sub.clone().render_usage().to_string();
                let cleaned_usage = usage.replace("session-", "session ");
                return Some(cleaned_usage);
            }
        }
        Some(
            "Usage: session [COMMAND]
Commands:
  start   Start a new session
  close   Close a session
  update  Update session(s)
  status  Update session status
  help    Print this message or the help of the given subcommand(s)"
                .to_string(),
        )
    }
}

#[derive(Parser, Debug)]
#[command(name = "session")]
pub struct SessionCli {
    #[command(subcommand)]
    pub command: Option<SessionCommands>,
}

#[derive(Subcommand, Debug)]
pub enum SessionCommands {
    /// Start a new session. If branch is omitted, you will be prompted to enter one.
    Start {
        /// Branch name (optional)
        branch: Option<String>,
        /// Base branch (optional, default: origin default branch)
        #[arg(short, long)]
        base: Option<String>,
        /// Select base branch from remote branches
        #[arg(short, long)]
        remote: bool,
    },
    /// Close a session. If branch is omitted, a list of sessions will be displayed.
    Close {
        /// Branch name (optional)
        branch: Option<String>,
    },
    /// Update session(s)
    Update {
        /// Update all sessions
        #[arg(short, long)]
        all: bool,
        /// Base branch to update from (optional, default: origin default branch)
        #[arg(short, long)]
        base: Option<String>,
    },
    /// Update session status
    Status {
        /// Branch name
        branch: String,
        /// New status (todo, running, done)
        status: String,
    },
}

fn start_session(
    branch: Option<String>,
    base: Option<String>,
    remote: bool,
    project_path: &Path,
) -> Result<String> {
    // Validate state before creating any branches or worktrees.
    let mut state = get_project_state(project_path)?;
    let project_path = project_path.canonicalize()?;
    let project_path = project_path.as_path();
    let base_branch = if remote {
        // TUI環境下では executor.rs が事前にリモートブランチを選択し --base に渡してくる。
        // CLIから直接叩かれた場合のみ inquiry を使用する。
        if let Some(b) = base {
            b
        } else {
            git::fetch(project_path, "origin")?;
            let remote_branches = git::list_remote_branches(project_path)?;
            if remote_branches.is_empty() {
                return Err(anyhow!("No remote branches found."));
            }
            inquire::Select::new("Select base branch from remote:", remote_branches)
                .prompt()
                .map_err(|e| anyhow!("Failed to select remote branch: {}", e))?
        }
    } else {
        match base {
            Some(b) => b,
            None => git::get_default_branch(project_path)?,
        }
    };

    let branch = match branch {
        Some(b) => b,
        None => {
            let default_name = if remote {
                if let Some(stripped) = base_branch.strip_prefix("origin/") {
                    stripped.to_string()
                } else if let Some(slash_idx) = base_branch.find('/') {
                    base_branch[slash_idx + 1..].to_string()
                } else {
                    base_branch.to_string()
                }
            } else {
                "".to_string()
            };

            // TUI環境下での executor.rs による事前入力を想定
            // executor.rs はブランチ名が指定されていない場合に事前に入力を求める。
            // それでも指定がない場合のみ inquire を使用する。
            inquire::Text::new("Enter session branch name:")
                .with_default(&default_name)
                .prompt()
                .map_err(|e| anyhow!("Failed to get branch name: {}", e))?
        }
    };

    if git::branch_exists(&branch, project_path)? {
        return Err(anyhow!("Branch '{}' already exists.", branch));
    }

    let worktree_path = project_path.join(&branch);
    if worktree_path.exists() {
        return Err(anyhow!(
            "Directory '{}' already exists.",
            worktree_path.display()
        ));
    }

    git::create_worktree(project_path, &branch, &worktree_path, &base_branch, remote)?;

    if !state.worktrees.iter().any(|w| w.branch == branch) {
        let mut worktree = Worktree {
            branch: branch.to_string(),
            directory: branch.to_string(),
            default: false,
            modified_at: "".to_string(),
            status: crate::domain::project::SessionStatus::Todo,
            has_upstream: git::has_upstream(&worktree_path).unwrap_or(false),
        };
        worktree.update_modified_at();
        state.worktrees.push(worktree);
    }
    state.current_worktree = Some(branch.to_string());
    state.update_last_updated();
    save_project_state(project_path, &state)?;

    Ok(format!(
        "Session started: branch '{}' in '{}'",
        branch,
        worktree_path.display()
    ))
}

fn close_session(branch: Option<String>, project_path: &Path) -> Result<String> {
    let mut state = get_project_state(project_path)?;

    let branch = match branch {
        Some(b) => b,
        None => {
            let session_branches: Vec<String> = state
                .worktrees
                .iter()
                .filter(|w| !w.default) // デフォルトブランチ（通常はmain）は削除させない方が安全かもしれない
                .map(|w| w.branch.clone())
                .collect();

            if session_branches.is_empty() {
                return Err(anyhow!("No sessions available to close."));
            }

            inquire::Select::new("Select session branch to close:", session_branches)
                .prompt()
                .map_err(|e| anyhow!("Failed to select branch: {}", e))?
        }
    };

    let worktree = state
        .worktrees
        .iter()
        .find(|w| w.branch == branch)
        .ok_or_else(|| anyhow!("Session '{}' not found in project state.", branch))?;
    if worktree.default {
        return Err(anyhow!("Cannot close the default session '{}'.", branch));
    }

    let worktree_path = project_path.join(&worktree.directory);
    if worktree_path.exists() {
        git::remove_worktree(project_path, &worktree_path)?;
    }

    git::delete_branch(project_path, &branch)?;

    state.worktrees.retain(|w| w.branch != branch);
    if state.current_worktree.as_deref() == Some(&branch) {
        state.current_worktree = state
            .worktrees
            .iter()
            .find(|w| w.default)
            .or_else(|| state.worktrees.first())
            .map(|w| w.branch.clone());
    }
    state.update_last_updated();
    save_project_state(project_path, &state)?;

    Ok(format!(
        "Session closed: branch '{}' removed and deleted",
        branch
    ))
}

fn update_sessions(all: bool, base: Option<String>, project_path: &Path) -> Result<String> {
    let state = get_project_state(project_path)?;

    let base_branch = match base {
        Some(b) => b,
        None => git::get_default_branch(project_path)?,
    };

    if let Some(remote) = git::get_branch_remote(project_path, &base_branch)? {
        println!("Fetching remote '{}'...", remote);
        git::fetch(project_path, &remote)?;
    }

    let sessions_to_update = if all {
        if state.worktrees.is_empty() {
            return Ok("No sessions found to update.".to_string());
        }
        state.worktrees.iter().collect::<Vec<_>>()
    } else {
        let selected = state
            .worktrees
            .iter()
            .find(|w| match &state.current_worktree {
                Some(branch) => w.branch == *branch,
                None => w.default,
            })
            .ok_or_else(|| anyhow!("No current session selected."))?;
        vec![selected]
    };

    let mut results = Vec::new();
    for worktree in sessions_to_update {
        let session = &worktree.branch;
        let worktree_path = project_path.join(&worktree.directory);
        if worktree_path.exists() {
            println!(
                "Updating session '{}' with base '{}'...",
                session, base_branch
            );
            match git::rebase(&worktree_path, &base_branch) {
                Ok(_) => results.push(format!("Session '{}' updated successfully.", session)),
                Err(e) => results.push(format!("Failed to update session '{}': {}", session, e)),
            }
        } else {
            results.push(format!(
                "Worktree for session '{}' does not exist at '{}'.",
                session,
                worktree_path.display()
            ));
        }
    }

    Ok(results.join("\n"))
}

fn update_session_status(branch: &str, status: &str, project_path: &Path) -> Result<String> {
    let mut state = get_project_state(project_path)?;

    if let Some(worktree) = state.worktrees.iter_mut().find(|w| w.branch == branch) {
        if let Some(new_status) = crate::domain::project::SessionStatus::from_str(status) {
            worktree.status = new_status.clone();
            worktree.update_modified_at();
            state.update_last_updated();
            save_project_state(project_path, &state)?;
            let status_icon = match new_status {
                crate::domain::project::SessionStatus::Todo => {
                    console::style(new_status.icon()).dim().to_string()
                }
                crate::domain::project::SessionStatus::Running => {
                    console::style(new_status.icon()).green().bold().to_string()
                }
                crate::domain::project::SessionStatus::Done => {
                    console::style(new_status.icon()).blue().bold().to_string()
                }
            };
            Ok(format!(
                "Session status updated: branch '{}' is now '{}' {}",
                branch,
                new_status.as_str(),
                status_icon
            ))
        } else {
            Err(anyhow!(
                "Invalid status '{}'. Valid options are: todo, running, done",
                status
            ))
        }
    } else {
        Err(anyhow!("Session '{}' not found.", branch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{ProjectState, SessionStatus};
    use crate::test_support::{git as run_git, init_repo, TestDir};

    fn project() -> TestDir {
        let dir = TestDir::new();
        init_repo(&dir.path().join("main"), "develop");
        std::fs::create_dir(dir.path().join(".usagi")).unwrap();
        save_project_state(
            dir.path(),
            &ProjectState {
                version: 1,
                initialized: true,
                worktrees: vec![Worktree {
                    branch: "develop".into(),
                    directory: "main".into(),
                    default: true,
                    modified_at: String::new(),
                    status: SessionStatus::Todo,
                    has_upstream: false,
                }],
                current_worktree: Some("develop".into()),
                last_updated: None,
                ai_model: None,
            },
        )
        .unwrap();
        dir
    }

    #[test]
    fn cannot_close_default_session() {
        let dir = project();
        let error = close_session(Some("develop".into()), dir.path()).unwrap_err();
        assert!(error.to_string().contains("default"));
        assert!(dir.path().join("main/.git").exists());
        assert_eq!(get_project_state(dir.path()).unwrap().worktrees.len(), 1);
    }

    #[test]
    fn close_uses_stored_worktree_directory() -> Result<()> {
        let dir = project();
        let worktree = dir.path().join("sessions/topic");
        git::create_worktree(dir.path(), "topic", &worktree, "develop", false)?;
        let mut state = get_project_state(dir.path())?;
        state.worktrees.push(Worktree {
            branch: "topic".into(),
            directory: "sessions/topic".into(),
            default: false,
            modified_at: String::new(),
            status: SessionStatus::Todo,
            has_upstream: false,
        });
        state.current_worktree = Some("topic".into());
        save_project_state(dir.path(), &state)?;
        close_session(Some("topic".into()), dir.path())?;
        assert!(!worktree.exists());
        assert!(!git::branch_exists("topic", dir.path())?);
        assert_eq!(
            get_project_state(dir.path())?.current_worktree.as_deref(),
            Some("develop")
        );
        Ok(())
    }

    #[test]
    fn update_local_branch_with_slash_uses_stored_directory() -> Result<()> {
        let dir = project();
        run_git(&dir.path().join("main"), &["branch", "feature/base"]);
        let result = update_sessions(false, Some("feature/base".into()), dir.path())?;
        assert!(result.contains("updated successfully"), "{result}");
        Ok(())
    }

    #[test]
    fn update_legacy_default_selection() -> Result<()> {
        let dir = project();
        let mut state = get_project_state(dir.path())?;
        state.current_worktree = None;
        save_project_state(dir.path(), &state)?;
        let result = update_sessions(false, Some("develop".into()), dir.path())?;
        assert!(result.contains("updated successfully"), "{result}");
        Ok(())
    }

    #[test]
    fn invalid_project_state_does_not_create_a_worktree() -> Result<()> {
        let dir = project();
        std::fs::remove_file(dir.path().join(".usagi/state.json"))?;
        assert!(start_session(
            Some("topic".into()),
            Some("develop".into()),
            false,
            dir.path()
        )
        .is_err());
        assert!(!dir.path().join("topic").exists());
        assert!(!git::branch_exists("topic", dir.path())?);
        Ok(())
    }
}
