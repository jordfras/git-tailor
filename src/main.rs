// TUI application entry point

use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        anyhow::bail!("Usage: git-scissors <commit-ish>");
    }

    let commit_ish = &args[1];

    let reference_oid = git_scissors::find_reference_point(commit_ish)?;
    println!("{}", reference_oid);

    Ok(())
}
