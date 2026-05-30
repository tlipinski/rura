use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    version = crate::props::VERSION,
    about = "Interactive TUI pipeline editor built for rapid iteration",
    long_about = None
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
    #[arg(long = "ff-split", hide = true)]
    pub split_commands: bool,
}
