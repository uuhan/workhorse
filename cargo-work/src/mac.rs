use crate::options::HorseOptions;
use cargo_options::{CargoOptions, CargoOptionsExt};
use serde::Serialize;

pub trait CargoKind {
    type Target: Serialize + CargoOptionsExt;
    fn cargo_options(&self) -> &Self::Target;
    fn horse_options(&self) -> &HorseOptions;
    fn options(&self) -> CargoOptions {
        self.cargo_options().options()
    }
    fn use_zigbuild(&self) -> bool;
    fn name(&self) -> &str;
}

macro_rules! cargo_command {
    ($command: ident) => {
        paste::paste! {
            pub mod [<$command:lower>] {
                use $crate::mac::CargoKind;
                use std::ops::{Deref, DerefMut};
                use tokio::process::Command;
                use std::process::ExitStatus;
                use color_eyre::eyre::{Result, WrapErr};
                use clap::Parser;

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
                    pub horse: $crate::options::HorseOptions,

                    #[clap(short, long, help = "使用 zigbuild")]
                    pub zigbuild: bool,
                }

                impl $command {
                    /// Execute cargo command
                    pub async fn execute(&self) -> Result<ExitStatus> {
                        let current_command = stringify!([<$command:lower>]);
                        let mut build = self.build_command()?;
                        let mut child = build.spawn().wrap_err_with(|| format!("Failed to run cargo {current_command}"))?;
                        Ok(child.wait().await?)
                    }

                    /// Generate cargo subcommand
                    pub fn build_command(&self) -> Result<Command> {
                        let build = self.cargo.command();
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

                impl CargoKind for $command {
                    type Target = cargo_options::$command;
                    fn cargo_options(&self) -> &Self::Target {
                        &self.cargo
                    }
                    fn horse_options(&self) -> &$crate::options::HorseOptions {
                        &self.horse
                    }
                    fn use_zigbuild(&self) -> bool {
                        self.zigbuild
                    }
                    fn name(&self) -> &str {
                        stringify!([<$command:lower>])
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
cargo_command!(Rustc);
cargo_command!(Check);
cargo_command!(Clean);
cargo_command!(Clippy);
cargo_command!(Metadata);
cargo_command!(Doc);

pub mod zigbuild {
    use crate::mac::CargoKind;
    use std::ops::{Deref, DerefMut};
    use std::process::ExitStatus;
    use tokio::process::Command;

    use clap::Parser;
    use color_eyre::eyre::{Result, WrapErr};

    #[derive(Clone, Debug, Default, Parser)]
    #[command(
        display_order = 1,
        about = "Run cargo zigbuild command",
        after_help = "Run `cargo zigbuild --help` for more detailed information."
    )]
    pub struct Zigbuild {
        #[command(flatten)]
        pub cargo: cargo_options::Build,

        #[command(flatten)]
        pub horse: crate::options::HorseOptions,

        #[clap(short, long, default_value = "true", help = "使用 zigbuild")]
        pub zigbuild: bool,
    }

    impl Zigbuild {
        /// Execute cargo command
        pub async fn execute(&self) -> Result<ExitStatus> {
            let current_command = "zigbuild";
            let mut build = self.build_command()?;
            let mut child = build
                .spawn()
                .wrap_err_with(|| format!("Failed to run cargo {current_command}"))?;
            Ok(child.wait().await?)
        }

        /// Generate cargo subcommand
        pub fn build_command(&self) -> Result<Command> {
            let build = self.cargo.command();
            Ok(build)
        }
    }

    impl Deref for Zigbuild {
        type Target = cargo_options::Build;

        fn deref(&self) -> &Self::Target {
            &self.cargo
        }
    }

    impl DerefMut for Zigbuild {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.cargo
        }
    }

    impl From<cargo_options::Build> for Zigbuild {
        fn from(cargo: cargo_options::Build) -> Self {
            Self {
                cargo,
                ..Default::default()
            }
        }
    }

    impl CargoKind for Zigbuild {
        type Target = cargo_options::Build;
        fn cargo_options(&self) -> &Self::Target {
            &self.cargo
        }
        fn horse_options(&self) -> &crate::options::HorseOptions {
            &self.horse
        }
        fn use_zigbuild(&self) -> bool {
            self.zigbuild
        }
        fn name(&self) -> &str {
            // cargo zigbuild 接收的参数和 cargo build 是一样的
            // TODO: 未来使用更加定制化的 zig 工具链
            "build"
        }
    }
}
