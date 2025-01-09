use cargo_work::Build;
use clap::{Parser, Subcommand};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Opt {
    #[command(name = "build", alias = "b")]
    Build(Build),
    #[command(name = "push", alias = "p")]
    Push,
}

#[derive(Debug, Parser)]
#[command(
    version,
    name = "cargo-work",
    styles = cargo_options::styles(),
)]
pub enum Cli {
    #[command(subcommand, name = "work")]
    Opt(Opt),
    #[command(flatten)]
    Cargo(Opt),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli {
        Cli::Opt(opt) | Cli::Cargo(opt) => match opt {
            Opt::Build(build) => println!("{:?}", build),
            Opt::Push => println!("git push"),
        },
    }

    Ok(())
}
