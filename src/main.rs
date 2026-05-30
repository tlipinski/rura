mod app;
mod cmd_runner;
mod completable_input;
mod completion;
mod config;
mod debouncer;
mod help_widget;
mod history;
mod output_widget;
mod props;
mod rura;
mod rura_widget;
mod save_to_file_widget;
mod search_widget;
mod theme;
mod uicmd;

use crate::app::App;
use crate::config::load_config;
use crate::history::History;
use anyhow::Result;
use clap::Parser;
use env_logger::{Builder, Target};
use log::{LevelFilter, debug, error, info};
use props::APP_NAME;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::exit;
use std::{env, fs};

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
        println!("{}", History::using_file().previous(""));
        exit(0)
    }

    let config = load_config(args.config.as_deref());

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
    shell: Option<String>,
    #[arg(short, long)]
    last: bool,
    #[arg(long = "ff-split", hide = true)]
    split_commands: bool,
}

fn run(args: Args, config: config::Config) -> Result<()> {
    let shell = args
        .shell
        .clone()
        .or(config.shell)
        .or({
            if let Ok(shell_var) = env::var("SHELL") {
                let shell_path = PathBuf::from(shell_var);
                shell_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(String::from)
            } else {
                None
            }
        })
        .unwrap_or("sh".into());

    debug!("Shell: {:?}", shell);

    info!("Starting TUI");
    let mut terminal = ratatui::init();

    let split_commands = args.split_commands;

    let app = App::new(
        args,
        &config.theme,
        config.keybindings,
        config.command_line_placement,
        config.error_display_mode,
        config.highlight_duration_ms,
        config.debounce_duration_ms,
        shell,
        split_commands,
    );
    let last_command = app.run(&mut terminal)?;

    info!("Restoring terminal");
    ratatui::restore();

    println!("{}", last_command);

    Ok(())
}
