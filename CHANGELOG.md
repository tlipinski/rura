## v1.0.1 - 2026-05-14

### 🐛 Bug Fixes

- Fixed `stdin` handling: addressed an edge case where a missing trailing newline could affect sensitive commands (e.g., `jq 'input_line_number'`)

### 💼 Other

- Ensured only successfully completed commands are saved to history in Normal and Live modes

### 📚 Documentation

- Updated the README to use a Git URL for Cargo installation
- Added installation instructions for Arch Linux via the AUR
- Clarified error display options in the README
- Documented storage and logging locations in the README

## v1.0.0 - 2026-05-11

First release