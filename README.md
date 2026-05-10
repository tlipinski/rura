<img src="rura.svg" height="80">

Rura transforms the tedious "edit, up-arrow, rerun" shell cycle into a fluid, interactive TUI scratchpad. It features live previews, syntax highlighting, and partial execution, allowing you to debug and iterate on commands in real time.

![Made with VHS](https://vhs.charm.sh/vhs-1Fv1rbJ2qMfPYx21lF5tDG.gif)

## Features

- **Partial Pipeline Execution**: Execute only up to the current subcommand to debug complex pipes.
- **Context-aware Completion**: Tab-complete commands and file paths using your system's bash tools.
- **Live Execution Modes**: Real-time feedback as you type, with optional "Live Until Cursor" or "Live Full" modes.
- **Search**: Search and highlight text within the output pane with case-sensitivity control.
- **Syntax Highlighting**: Visual feedback for subcommand boundaries, quotes, and pipes.
- **Persistent History**: Quickly access and reuse previous commands.
- **Line Wrapping**: Toggle whether long output lines wrap to fit the view.
- **Flexible Error Display**: Toggle between inline error messages and a dedicated error pane.
- **Customizable**: Fully configurable key bindings, themes, and UI placement via TOML.

## Installation

Check the [Releases](https://github.com/tlipinski/rura/releases) page for pre-compiled binaries for your platform.

Alternatively, you can install Rura from source using Cargo:

```bash
cargo install --path .
```

## Usage

You can start Rura by passing a file as an argument, piping data into it, or providing an initial command.

```bash
# Open a file
rura --file data.json

# Pipe data into rura
cat logs.txt | rura

# Start with an initial command
rura --command "grep error | sort"

# Print the last executed command from history without opening the UI
rura --last
```

### CLI Arguments

- `-f, --file <FILE>`: Path to the input file.
- `-c, --command <COMMAND>`: Initial command to populate the input field.
- `-C, --config <FILE>`: Path to a custom TOML configuration file.
- `-l, --last`: Print the last command from history and exit.
- `-V, --version`: Print version information.

## Key Bindings

### Command Execution

- **Enter**: Execute the full command pipeline.
- **Alt + \\**: Execute the pipeline up to the current subcommand (where your cursor is).
- **Alt + |**: Execute the pipeline up to the *previous* subcommand.
- **Alt + i**: Reset view to show the original input data.

### Navigation & View

- **Arrows** or **Alt + h/j/k/l**: Scroll the output (Left, Down, Up, Right).
- **PageUp / PageDown** or **Alt + Up / Down**: Scroll the output by page.
- **Ctrl + u / Ctrl + d**: Scroll up or down quickly.
- **F3 / F4**: Search forward or backward in the output.
- **Alt + c**: Toggle case sensitivity for search.
- **Alt + w**: Toggle line wrapping.

### Live Execution Modes

- **F11**: Toggle "Live Until Cursor" mode. Executes the pipeline up to the cursor as you type.
- **F12**: Toggle "Live Full" mode. Executes the entire pipeline as you type.

### Command Input & Subcommands

- **Tab**: Trigger forward command or file completion (requires `bash` with `compgen` available).
- **Shift + Tab**: Trigger backward command or file completion.
- **Alt + Right**: Move cursor to the next subcommand.
- **Alt + Left**: Move cursor to the previous subcommand.
- **Ctrl + p**: Previous command in history.
- **Ctrl + n**: Next command in history.

### General

- **F1**: Toggle help screen.
- **Ctrl + c**: Exit Rura. The last executed command is printed to your terminal.

## Configuration

Rura can be configured via a TOML file. The configuration path is determined as follows:
1. Path specified by the `--config` (or `-C`) CLI argument.
2. Path specified by the `RURA_CONFIG` environment variable.
3. Default path:
    - **Linux**: `~/.config/rura/config.toml`
    - **macOS**: `~/Library/Application Support/rura/config.toml`

### General Options

- `command_line_placement`: Set to `"top"` or `"bottom"` (default) to change where the input field is rendered.
- `error_display_mode`: Set to `"inline"` or `"pane"` to choose how errors are shown.
- `highlight_duration_ms`: Duration in milliseconds for the temporary highlighting when executing commands (default: `250`).
- `debounce_duration_ms`: Duration in milliseconds to wait before executing commands in live mode (default: `500`).

### Customizing Key Bindings

You can override any default key binding in the `[keybindings]` section. Multiple keys can be assigned to the same action.

```toml
[keybindings]
quit = ["ctrl+q", "ctrl+c"]
execute_full = ["enter"]
complete = ["tab"]
complete_prev = ["shift+tab"]
subcommand_next = ["alt+right"]
```

### Customizing Theme

Colors and styles can be adjusted in the `[theme]` section. Supported colors include `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`, `black`, `white`, and hex codes (e.g., `"#ffffff"`).

Available theme keys:
- `cmd_regular`: Default subcommand style.
- `cmd_regular_pipe`: Style for the pipe character in regular mode.
- `cmd_regular_current`: Background style for the currently selected subcommand.
- `cmd_highlight`: Style for the subcommand being executed.
- `cmd_highlight_pipe`: Style for the pipe character during execution.
- `cmd_highlight_current`: Style for the current subcommand during execution.
- `cmd_quoted`: Style for quoted strings.
- `cmd_invalid`: Style for invalid subcommands (if parsing fails).
- `output_highlight`: Style for search results in the output.
- `output_highlight_current`: Style for the currently selected search result.
- `line_nums`: Style for line numbers in the output.

```toml
[theme.cmd_highlight]
fg = "black"
bg = "yellow"
bold = true

[theme.line_nums]
fg = "magenta"
```