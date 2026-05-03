mod app;
mod config;
mod history;
mod props;
mod rura;
mod rura_widget;
mod uicmd;
mod theme;

use crate::app::App;
use crate::config::load_config;
use clap::Parser;
use env_logger::{Builder, Target};
use log::{LevelFilter, error, info};
use props::APP_NAME;
use std::error::Error;
use std::fs::OpenOptions;
use std::process::exit;
use crate::history::History;

fn main() {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("/tmp/{APP_NAME}.log"))
        .expect("Failed to open log file");

    Builder::new()
        .target(Target::Pipe(Box::new(file)))
        .filter_level(LevelFilter::Debug)
        .init();

    let args = Args::parse();

    if args.last {
        println!("{}", History::load().previous(""));
        exit(0)
    }

    let config = load_config();

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
    #[arg(short, long)]
    last: bool
}

fn run(args: Args, config: config::Config) -> Result<(), Box<dyn Error>> {
    info!("Starting TUI");
    let mut terminal = ratatui::init();

    let app = App::new(args, &config.theme, config.keybindings, config.command_line_placement);
    let last_command = app.run(&mut terminal)?;

    info!("Restoring terminal");
    ratatui::restore();

    println!("{}", last_command);

    Ok(())
}
