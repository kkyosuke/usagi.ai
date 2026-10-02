use crate::presentation::tui::action::{Action, ActionResult};
use anyhow::Result;

pub struct NewAction;

#[async_trait::async_trait]
impl Action for NewAction {
    async fn execute(&self) -> Result<ActionResult> {
        let term = console::Term::stdout();
        term.clear_screen()?;

        let git_url = match inquire::Text::new("git url:").prompt() {
            Ok(url) if url.is_empty() => {
                term.write_line("Git URL is required.")?;
                term.write_line("Press any key to continue...")?;
                term.read_key()?;
                return Ok(ActionResult::Continue);
            }
            Ok(url) => url,
            Err(_) => return Ok(ActionResult::Continue),
        };

        let repo_name = git_url
            .split('/')
            .last()
            .unwrap_or("repository")
            .trim_end_matches(".git");
        let default_dir = format!("./{}", repo_name);

        let directory_input = match inquire::Text::new("directory:")
            .with_default(&default_dir)
            .prompt()
        {
            Ok(dir) => dir,
            Err(_) => return Ok(ActionResult::Continue),
        };

        let branch_input = match inquire::Text::new("branch:")
            .with_default("default")
            .prompt()
        {
            Ok(branch) => branch,
            Err(_) => return Ok(ActionResult::Continue),
        };

        let directory = if directory_input.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(directory_input))
        };
        let branch = if branch_input == "default" || branch_input.is_empty() {
            None
        } else {
            Some(branch_input)
        };

        term.clear_screen()?;
        match crate::presentation::cli::init::run(&git_url, directory, branch) {
            Ok(_) => {}
            Err(e) => {
                term.write_line(&format!("Error: {}", e))?;
            }
        }

        term.write_line("Press any key to continue...")?;
        term.read_key()?;

        Ok(ActionResult::Continue)
    }
}
