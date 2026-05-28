## v.1.3.0 - 2026-05-28

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
