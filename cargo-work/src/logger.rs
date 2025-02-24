use color_eyre::Result;
use colored::Colorize;
use tracing::{
    field::{self, Visit},
    Event, Level, Subscriber,
};
use tracing_subscriber::{filter::EnvFilter, layer::Context, prelude::*, Layer};

pub fn init() -> Result<()> {
    let mut filter = EnvFilter::from_default_env();
    if std::env::var("RUST_LOG").is_err() {
        filter = filter.add_directive("cargo_work=info".parse()?);
    }

    tracing_subscriber::registry()
        .with(filter)
        .with(WorkLayer)
        .init();

    Ok(())
}

pub struct WorkLayer;
static PFX: &str = "  WORK";

impl<S> Layer<S> for WorkLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();

        let mut fields = match *meta.level() {
            Level::ERROR => {
                print!("[{}] ", PFX.bold().red());
                WorkField {}
            }
            Level::WARN => {
                print!("[{}] ", PFX.bold().yellow());
                WorkField {}
            }
            Level::INFO => {
                print!("[{}] ", PFX.bold().green());
                WorkField {}
            }
            Level::DEBUG => {
                print!(
                    "[{}] {}{}: ",
                    PFX.bold().blue(),
                    meta.target().blue(),
                    meta.line().map(|l| format!("#{}", l)).unwrap_or_default()
                );
                WorkField {}
            }
            Level::TRACE => {
                print!(
                    "[{}] {}{}: ",
                    PFX.bold().blue(),
                    meta.target().blue(),
                    meta.line().map(|l| format!("#{}", l)).unwrap_or_default()
                );
                WorkField {}
            }
        };

        event.record(&mut fields);

        println!();
    }
}

struct WorkField {}

impl Visit for WorkField {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            print!("{:?}", value);
        } else {
            print!(" {}={:?}", field.name().italic().bold(), value)
        }
    }
}
