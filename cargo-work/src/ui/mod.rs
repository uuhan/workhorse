#![allow(
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate
)]

mod app;
mod colors;
mod destroy;
mod tabs;
mod theme;

use std::io::stdout;

use app::App;
use color_eyre::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{TerminalOptions, Viewport};

pub use self::{
    colors::{color_from_oklab, RgbSwatch},
    theme::THEME,
};

pub fn init() -> Result<()> {
    color_eyre::install()?;
    // this size is to match the size of the terminal when running the demo
    // using vhs in a 1280x640 sized window (github social preview size)
    // let viewport = Viewport::Fixed(Rect::new(0, 0, 81, 18));
    let viewport = Viewport::Fullscreen;
    let terminal = ratatui::init_with_options(TerminalOptions { viewport });
    execute!(stdout(), EnterAlternateScreen).expect("failed to enter alternate screen");
    let app_result = App::default().run(terminal);
    execute!(stdout(), LeaveAlternateScreen).expect("failed to leave alternate screen");
    ratatui::restore();
    app_result
}
