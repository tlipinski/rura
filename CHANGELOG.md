## v1.8.0 - 2026-07-07

### Features

- Build DEB and RPM packages

### Bug Fixes

- Added missing "Show presets" shortcut to help widget

## v1.7.0 - 2026-06-25

### Features

- Added presets functionality
- Added history support for search input
- Added line numbers toggle functionality

## v1.6.0 - 2026-06-21

### Features

- Added diffing mode for comparing command outputs
- *(ui)* Added `toggle_live` and `toggle_live_until_cursor` keybindings

## v1.5.0 - 2026-06-05

### Features

- Added Windows support for running commands
- Added subcommand editing capabilities (copy, cut, paste)
- *(ui)* Added theming support for popup widgets
- *(ui)* Added command formatting
- *(ui)* Added scroll support to help popup

## v1.4.0 - 2026-06-02

### Features

- Introduced cached command runner
- Added `no_cache` option to configuration
- Highlight failed subcommand in input field
- Add progress indicator for long-running commands
- Add support for horizontal page-wise scrolling

### Bug Fixes

- Handle tilde-prefixed paths correctly in zsh
- Handle errors in tasks reading `stdin`

## v1.3.0 - 2026-05-28

### Features

- Added shell customization with CLI arg, config and SHELL environment variable
- Added Zsh shell completion support
- Added Fish shell completion support
- Added support for x86_64-unknown-linux-musl target platform

### Bug Fixes

- Append newline to saved content

## v1.2.0 - 2026-05-26

### Features

- Added support for saving commands and output to files

### Bug Fixes

- Handle non utf-8 input in stdin and file reading tasks

## v1.1.1 - 2026-05-19

### Bug Fixes

- Reset both vertical and horizontal offsets when the output length changes

## v1.1.0 - 2026-05-17

### Features

- Add regex support to the search
- Add horizontal scrollbar support to OutputWidget

### Bug Fixes

- Scroll horizontally when navigating through highlights

## v1.0.1 - 2026-05-14

### Bug Fixes

- Fixed `stdin` handling: addressed an edge case where a missing trailing newline could affect sensitive commands (e.g., `jq 'input_line_number'`)

### Other

- Ensured only successfully completed commands are saved to history in Normal and Live modes

## v1.0.0 - 2026-05-11

First release
