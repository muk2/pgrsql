# pgrsql - Project Guidance

## Overview

pgrsql is a TUI (Terminal User Interface) SQL editor for PostgreSQL written in Rust. It provides a visual, keyboard-driven interface for connecting to PostgreSQL databases, browsing schemas, writing queries, and viewing results.

## Project Structure

```
pgrsql/
├── Cargo.toml          # Dependencies and project metadata
├── src/
│   ├── main.rs         # Entry point, terminal setup, main loop
│   ├── db/             # Database layer
│   │   ├── mod.rs
│   │   ├── connection.rs   # Connection management
│   │   ├── query.rs        # Query execution and result parsing
│   │   └── schema.rs       # Schema introspection
│   ├── editor/         # Text editor
│   │   ├── mod.rs
│   │   ├── buffer.rs       # Text buffer with cursor/selection
│   │   └── history.rs      # Query history
│   └── ui/             # User interface
│       ├── mod.rs
│       ├── app.rs          # Application state and input handling
│       ├── components.rs   # UI rendering
│       └── theme.rs        # Colors and syntax highlighting
├── README.md           # User documentation
└── LICENSE             # MIT license
```

## Development

### Building

```bash
cargo build          # Debug build
cargo build --release  # Release build
```

### Running

```bash
cargo run
```

### Key Dependencies

- **ratatui**: TUI framework (fork of tui-rs)
- **crossterm**: Terminal manipulation
- **tokio-postgres**: Async PostgreSQL driver
- **tokio**: Async runtime

## Coding Standards

- Use `rustfmt` for formatting
- Follow Rust naming conventions (snake_case for functions/variables, PascalCase for types)
- Keep modules focused and single-purpose
- Prefer returning `Result` over panicking
- Use `anyhow` for error handling in application code
- Use `thiserror` for library error types

## Architecture

### State Management

The `App` struct in `src/ui/app.rs` holds all application state:
- Connection state
- Editor buffer
- Query results
- UI focus and selections

### Event Loop

1. Terminal events are polled in `main.rs`
2. Key events are dispatched to `App::handle_input()`
3. Input handlers update state and may trigger async operations
4. The UI is redrawn with `ui::draw()`

### Database Operations

All database operations are async and use `tokio-postgres`. The `ConnectionManager` handles connection lifecycle and schema switching.

## Testing

Run tests with:
```bash
cargo test
```

Integration tests require a running PostgreSQL instance. Set `DATABASE_URL` environment variable or use the default local connection.
