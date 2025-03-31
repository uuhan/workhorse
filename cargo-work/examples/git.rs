use color_eyre::eyre::Result;
use git2::Repository;

fn main() -> Result<()> {
    color_eyre::install()?;

    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    println!("COMMIT: {}", head.peel_to_commit()?.id().to_string());
    println!("MESSAGE: {}", head.peel_to_commit()?.message().unwrap());
    println!("SUMMARY: {}", head.peel_to_commit()?.summary().unwrap());
    Ok(())
}
