use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use clap::{
    builder::{PossibleValuesParser, TypedValueParser as _},
    Parser, ValueEnum,
};

#[derive(Clone, Debug, Parser)]
pub struct WorkOptions {}

impl Default for WorkOptions {
    fn default() -> Self {
        Self {}
    }
}
