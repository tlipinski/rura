mod app;
mod args;
mod completable_input;
mod completion;
mod config;
mod content_widget;
mod debouncer;
mod details_widget;
mod file_saver;
mod help_widget;
mod history;
mod output_widget;
mod presets;
mod presets_widget;
mod props;
mod rura;
mod rura_input;
mod rura_widget;
mod save_to_file_widget;
mod search_widget;
mod shell;
mod stdin;
mod theme;
mod uicmd;

use crate::app::App;
use crate::args::Args;
use crate::config::{history_path, load_config};
use crate::history::History;
use anyhow::Result;
use clap::Parser;
use env_logger::{Builder, Target};
use log::{LevelFilter, error, info};
use props::APP_NAME;
use std::fs;
use std::fs::OpenOptions;
use std::process::exit;

fn main() {
    let _: Vec<_> = dirs::cache_dir()
        .map(|d| d.join(APP_NAME).join("logs.txt"))
        .into_iter()
        .flat_map(|path| {
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            OpenOptions::new().create(true).append(true).open(path)
        })
        .map(|file| {
            Builder::new()
                .target(Target::Pipe(Box::new(file)))
                .filter_level(LevelFilter::Debug)
                .init()
        })
        .collect();

    let args = Args::parse();

    if args.last {
        if let Some(p) = history_path() {
            println!("{}", History::using_file(p).previous(""));
            exit(0)
        }
    }

    let config = load_config(args.config.as_deref());

    info!("{args:?}");

    match run_tui(args, config) {
        Ok(()) => {
            info!("Exiting application");
        }
        Err(e) => {
            error!("{e}");
        }
    }
}

fn run_tui(args: Args, config: config::Config) -> Result<()> {
    info!("Starting TUI");

    let mut terminal = ratatui::init();

    let app = App::new(args, config);

    let last_command = app.run(&mut terminal)?;

    info!("Restoring terminal");
    ratatui::restore();

    println!("{}", last_command);

    Ok(())
}
