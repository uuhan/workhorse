use cargo_work::Build;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::thread;
use std::time::Duration;
use std::{cmp::min, fmt::Write};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Opt {
    #[command(name = "build", alias = "b")]
    Build(Build),
    #[command(name = "push", alias = "p", about = "Push to remote repository")]
    Push,
    #[command(name = "pull", alias = "l", about = "Fetch artifacts from horsed")]
    Pull,
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
            Opt::Pull => {
                let mut downloaded = 0;
                let total_size = 23123123;

                let pb = ProgressBar::new(total_size);
                pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
                    .progress_chars("#>-"));

                while downloaded < total_size {
                    let new = min(downloaded + 223211, total_size);
                    downloaded = new;
                    pb.set_position(new);
                    thread::sleep(Duration::from_millis(8));
                }

                pb.finish_with_message("downloaded");
            }
        },
    }

    Ok(())
}
