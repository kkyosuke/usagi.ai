use crate::infrastructure::project_state::{get_project_state, save_project_state};
use crate::presentation::commands::{Command, CommandAction, CommandContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use console::Term;
use std::path::Path;

pub struct SpaceCommand;

const NAME: &str = "space";
const DESCRIPTION: &str = "Switch workspace";
const HELP: &str = "Switches the workspace.
Usage: space <worktree_name>
Switches the current working directory to the specified worktree.
Specifying 'main' returns to the main worktree.";

#[async_trait]
impl Command for SpaceCommand {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn help(&self) -> &str {
        HELP
    }

    fn usage(&self, _args: &[&str]) -> Option<String> {
        Some("Usage: space <WORKTREE>".to_string())
    }

    fn should_sync_selection(&self, _parts: &[String]) -> bool {
        true
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandAction> {
        let mut parts = context.parts;
        if parts.len() == 1 {
            let selected = context
                .worktrees
                .get(context.selected_index)
                .ok_or_else(|| anyhow!("No workspace selected."))?;
            parts.push(selected.clone());
        }
        let cmd_to_execute = parts.join(" ");

        Ok(CommandAction::RunCommand {
            parts,
            cmd_to_execute,
            close_after: true,
        })
    }

    async fn run(
        &self,
        args: Vec<String>,
        project_path: &Path,
        _current_worktree: &str,
        _term: &Term,
    ) -> Result<String> {
        let cli = match SpaceCli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(e) => {
                return Err(anyhow!("{}", e));
            }
        };

        if let Some(worktree) = cli.worktree {
            let mut state = get_project_state(project_path)?;
            let selected = state
                .worktrees
                .iter_mut()
                .find(|w| {
                    if worktree == "main" {
                        w.default
                    } else {
                        w.branch == worktree
                    }
                })
                .ok_or_else(|| anyhow!("Worktree '{}' does not exist.", worktree))?;
            if !project_path.join(&selected.directory).is_dir() {
                return Err(anyhow!(
                    "Workspace directory '{}' does not exist.",
                    selected.directory
                ));
            }
            selected.update_modified_at();
            state.current_worktree = Some(selected.branch.clone());
            state.update_last_updated();
            save_project_state(project_path, &state)?;
            Ok(format!("Switched to workspace '{}'", worktree))
        } else {
            Ok("".to_string())
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "space")]
pub struct SpaceCli {
    pub worktree: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{ProjectState, SessionStatus, Worktree};
    use crate::test_support::TestDir;

    fn project() -> TestDir {
        let dir = TestDir::new();
        std::fs::create_dir(dir.path().join(".usagi")).unwrap();
        std::fs::create_dir(dir.path().join("main")).unwrap();
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
                current_worktree: None,
                last_updated: None,
                ai_model: None,
            },
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn main_alias_selects_the_actual_default_branch() -> Result<()> {
        let dir = project();
        SpaceCommand
            .run(
                vec!["space".into(), "main".into()],
                dir.path(),
                "",
                &Term::buffered_stdout(),
            )
            .await?;
        let state = get_project_state(dir.path())?;
        assert_eq!(state.current_worktree.as_deref(), Some("develop"));
        assert!(!state.worktrees[0].modified_at.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn missing_directory_does_not_change_selection() -> Result<()> {
        let dir = project();
        std::fs::remove_dir(dir.path().join("main"))?;
        assert!(SpaceCommand
            .run(
                vec!["space".into(), "main".into()],
                dir.path(),
                "",
                &Term::buffered_stdout()
            )
            .await
            .is_err());
        assert!(get_project_state(dir.path())?.current_worktree.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn empty_selection_returns_an_error() -> Result<()> {
        let dir = project();
        let context = CommandContext {
            parts: vec!["space".into()],
            state: get_project_state(dir.path())?,
            worktrees: vec![],
            selected_index: 0,
            input_history: vec![],
            project_path: dir.path().to_path_buf(),
        };
        assert!(SpaceCommand.execute(context).await.is_err());
        Ok(())
    }
}
