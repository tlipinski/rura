mod app;
mod config;
mod props;
mod rura;

use crate::app::App;
use crate::config::load_config;
use clap::Parser;
use env_logger::{Builder, Target};
use log::{LevelFilter, error, info};
use std::error::Error;
use std::fs::OpenOptions;
use props::APP_NAME;

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
    file: Option<String>,
}

fn run(args: Args, config: config::Config) -> Result<(), Box<dyn Error>> {
    info!("Starting TUI");
    let mut terminal = ratatui::init();

    let app = App::new(args, &config.theme, &config.keybindings);
    let last_command = app.run(&mut terminal)?;

    info!("Restoring terminal");
    ratatui::restore();
    
    println!("{}", last_command);

    Ok(())
}
