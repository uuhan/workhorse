use paste::paste;

macro_rules! cargo_command {
    ($command: ident) => {
        paste! {
            pub mod [<$command:lower>] {
                use std::ops::{Deref, DerefMut};
                use std::path::PathBuf;
                use std::process::{self, Command};

                use anyhow::{Context, Result};
                use clap::Parser;

                use crate::options::HorseOptions;

                #[derive(Clone, Debug, Default, Parser)]
                #[command(
                    display_order = 1,
                    about = "Run cargo " $command:lower " command",
                    after_help = "Run `cargo help " $command:lower "` for more detailed information."
                )]
                pub struct $command {
                    #[command(flatten)]
                    pub cargo: cargo_options::$command,

                    #[command(flatten)]
                    pub horse: HorseOptions,
                }

                impl $command {
                    /// Execute cargo command
                    pub fn execute(&self) -> Result<()> {
                        let current_command = stringify!([<$command:lower>]);
                        let mut build = self.build_command()?;
                        let mut child = build.spawn().with_context(|| format!("Failed to run cargo {current_command}"))?;
                        let status = child.wait().expect(&format!("Failed to wait on cargo {current_command} process"));
                        if !status.success() {
                            process::exit(status.code().unwrap_or(1));
                        }
                        Ok(())
                    }

                    /// Generate cargo subcommand
                    pub fn build_command(&self) -> Result<Command> {
                        let mut build = self.cargo.command();
                        Ok(build)
                    }
                }

                impl Deref for $command {
                    type Target = cargo_options::$command;

                    fn deref(&self) -> &Self::Target {
                        &self.cargo
                    }
                }

                impl DerefMut for $command {
                    fn deref_mut(&mut self) -> &mut Self::Target {
                        &mut self.cargo
                    }
                }

                impl From<cargo_options::$command> for $command {
                    fn from(cargo: cargo_options::$command) -> Self {
                        Self {
                            cargo,
                            ..Default::default()
                        }
                    }
                }

            }
        }
    };
}

cargo_command!(Build);
cargo_command!(Test);
cargo_command!(Install);
cargo_command!(Run);
cargo_command!(Check);
