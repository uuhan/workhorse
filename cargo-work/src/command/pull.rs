use std::env;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::process::{self, Command};

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Clone, Debug, Default, Parser)]
#[command(
    display_order = 1,
    after_help = "Pull your build artifacts from the horsed server."
)]
pub struct Pull {}
