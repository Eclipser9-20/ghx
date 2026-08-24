use crate::filter::{self, FilterOptions};
use anyhow::{bail, Result};
use colored::Colorize;

pub fn run(
    path: Option<String>,
    invert_paths: bool,
    replace_text: Vec<String>,
    yes: bool,
) -> Result<()> {
    if path.is_none() && replace_text.is_empty() {
        bail!("nothing to do — pass --path and/or --replace-text");
    }
    if invert_paths && path.is_none() {
        bail!("--invert-paths requires --path");
    }

    let replace_text = replace_text
        .iter()
        .map(|s| filter::parse_replace_text(s))
        .collect::<Result<Vec<_>>>()?;

    let opts = FilterOptions {
        path,
        invert_paths,
        replace_text,
        yes,
    };

    println!(
        "{}",
        "THIS REWRITES HISTORY. Every commit hash after the first affected commit will change."
            .red()
            .bold()
    );
    println!(
        "{}",
        "Anyone else with a clone of this repo will need to re-clone or force-reset — \
         their existing history will no longer match."
            .red()
    );
    println!();

    let plan = filter::plan(&opts)?;
    println!(
        "{} commits total, {} would be rewritten, {} would become empty",
        plan.total_commits,
        plan.affected_commits.to_string().yellow(),
        plan.emptied_commits.to_string().yellow()
    );

    if !opts.yes {
        println!();
        println!(
            "{}",
            "Dry run only — pass --yes to actually rewrite history.".dimmed()
        );
        return Ok(());
    }

    let new_tip = filter::run(&opts)?;
    let short = new_tip.to_string()[..7].to_string();
    println!(
        "{} History rewritten — current branch now at {}",
        "✓".green().bold(),
        short.yellow()
    );
    Ok(())
}
