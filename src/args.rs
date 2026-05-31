use clap::Parser;
use clap::builder::styling::{AnsiColor, Effects, Styles};

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Parser, Debug)]
#[command(
    version = crate::props::VERSION,
    styles = styles(),
    about = "Interactive TUI pipeline editor built for rapid iteration",
    long_about = "Rura transforms the tedious \"edit, up-arrow, rerun\" shell cycle into a fluid, \
    interactive TUI scratchpad. It features live previews, syntax highlighting, and partial \
    execution, allowing you to debug and iterate on commands in real time.",
    after_help = "\x1b[1mUsage Examples:\x1b[0m\n  \
      # Open a file\n  \
      rura --file data.json\n\n  \
      # Pipe data into rura\n  \
      cat logs.txt | rura\n\n  \
      # Start with an initial command\n  \
      rura --command \"grep error | sort\"\n\n  \
      # Print the last executed command from history without opening the UI\n  \
      rura --last\n\n\
    \x1b[1mKey Bindings:\x1b[0m\n  \
      Enter           Execute the full command pipeline.\n  \
      Alt + \\         Execute the pipeline up to the current subcommand (where your cursor is).\n  \
      Alt + |         Execute the pipeline up to the previous subcommand.\n  \
      F11             Toggle \"Live Until Cursor\" mode.\n  \
      F12             Toggle \"Live Full\" mode.\n  \
      Tab             Trigger forward command or file completion.\n  \
      Ctrl + p / n    Previous/Next command in history.\n  \
      Ctrl + s        Save the current output to a file.\n  \
      Ctrl + Alt + s  Save the current command to a file.\n  \
      Ctrl + c        Exit Rura. The last executed command is printed to your terminal.\n\n\
    \x1b[1mConfiguration:\x1b[0m\n  \
      Rura can be configured via a TOML file. The shell used is determined by (in order):\n  \
      1. The --shell CLI argument\n  \
      2. The shell property in the configuration file\n  \
      3. The SHELL environment variable\n  \
      4. Default to sh\n\n\
    \x1b[1mStorage & Logs:\x1b[0m\n  \
      History: ~/.local/share/rura/history.txt (Linux), ~/Library/Application Support/rura/history.txt (macOS)\n  \
      Logs:    ~/.cache/rura/logs.txt (Linux), ~/Library/Caches/rura/logs.txt (macOS)\n\n\
    Full documentation available at: https://github.com/tlipinski/rura"
)]
pub struct Args {
    #[arg(short, long, help = "Path to the input file")]
    pub file: Option<String>,
    #[arg(short, long, help = "Initial command to populate the input field")]
    pub command: Option<String>,
    #[arg(short = 'C', long, help = "Path to a custom TOML configuration file")]
    pub config: Option<String>,
    #[arg(
        short,
        long,
        help = "Specify the shell to use for execution and completions"
    )]
    pub shell: Option<String>,
    #[arg(short, long, help = "Print the last command from history and exit")]
    pub last: bool,
    #[arg(long = "cache", hide = true)]
    pub use_cache: bool,
}
