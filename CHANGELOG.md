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