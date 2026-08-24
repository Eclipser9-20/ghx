use crate::commands::git as gitcmd;
use crate::git;
use crate::palette::{Paint, COMMENT};
use anyhow::Result;

#[derive(clap::Subcommand)]
pub enum AiCommand {
    /// Commit staged changes with a generated message
    Commit,
    /// Summarize a diff in plain English (working tree by default)
    Review {
        /// Review the staged diff instead of the working tree
        #[arg(long)]
        staged: bool,
    },
}

pub fn run(cmd: AiCommand) -> Result<()> {
    match cmd {
        AiCommand::Commit => gitcmd::commit(None, true),
        AiCommand::Review { staged } => {
            let diff = git::diff(staged)?;
            println!("{}", crate::ai::review_diff(&diff)?);
            println!();
            println!(
                "{}",
                "Generated from the diff — read the code before trusting it.".tc(COMMENT)
            );
            Ok(())
        }
    }
}
