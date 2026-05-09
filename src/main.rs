mod app;
mod completion;
mod config;
mod debouncer;
mod history;
mod output_widget;
mod props;
mod rura;
mod rura_widget;
mod theme;
mod uicmd;

use crate::app::App;
use crate::config::load_config;
use crate::history::History;
use clap::Parser;
use env_logger::{Builder, Target};
use log::{LevelFilter, error, info};
use props::APP_NAME;
use std::error::Error;
use std::fs::OpenOptions;
use std::process::exit;
use std::str::FromStr;

fn main() {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("/tmp/{APP_NAME}.log"))
        .expect("Failed to open log file");

    let args = Args::parse();

    let config = load_config(args.config.as_deref());

    if args.last {
        println!("{}", History::load().previous(""));
        exit(0)
    }

    let level_filter = match config.log_level {
        Some(ref level) => {
            LevelFilter::from_str(&level).expect("Invalid log level specified in config")
        }
        None => LevelFilter::Info,
    };

    Builder::new()
        .target(Target::Pipe(Box::new(file)))
        .filter_level(level_filter)
        .init();

    info!("{args:?}");

    match run(args, config) {
        Ok(()) => {
            info!("Exiting application");
        }
        Err(e) => {
            error!("{e}");
        }
    }
}

#[derive(Parser, Debug)]
#[command(version = crate::props::VERSION, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    file: Option<String>,
    #[arg(short, long)]
    command: Option<String>,
    #[arg(short = 'C', long)]
    config: Option<String>,
    #[arg(short, long)]
    last: bool,
}

fn run(args: Args, config: config::Config) -> Result<(), Box<dyn Error>> {
    info!("Starting TUI");
    let mut terminal = ratatui::init();

    let app = App::new(
        args,
        &config.theme,
        config.keybindings,
        config.command_line_placement,
        config.error_display_mode,
        config.highlight_duration_ms,
        config.debounce_duration_ms,
    );
    let last_command = app.run(&mut terminal)?;

    info!("Restoring terminal");
    ratatui::restore();

    println!("{}", last_command);

    Ok(())
}
