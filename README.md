# pgrsql

A beautiful, fast TUI SQL editor for PostgreSQL written in Rust.

![pgrsql](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- **Visual Database Browser**: Navigate databases, schemas, and tables in a tree view
- **SQL Syntax Highlighting**: Keywords, strings, numbers, and comments are color-coded
- **Query Results Table**: Scrollable, navigable results with cell selection
- **Query History**: Persistent history with search capability
- **Connection Management**: Save and manage multiple PostgreSQL connections
- **Vim Keybindings**: Optional vim mode with Normal, Insert, Visual, and Visual Line modes
- **Keyboard-First Design**: Efficient navigation without leaving the keyboard
- **Dark Theme**: Easy on the eyes for long coding sessions

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) (1.70 or later)
- PostgreSQL server to connect to

### From Source

```bash
git clone https://github.com/yourusername/pgrsql.git
cd pgrsql
cargo build --release
```

The binary will be at `target/release/pgrsql`.

### With Cargo

```bash
cargo install --path .
```

Or once published:

```bash
cargo install pgrsql
```

## Quick Start

1. **Launch pgrsql**:
   ```bash
   pgrsql
   ```

2. **Connect to your database**:
   - A connection dialog appears on startup
   - Your last-used connection is auto-populated (just enter your password)
   - Enter your PostgreSQL connection details:
     - **Name**: A friendly name for this connection
     - **Host**: Database host (e.g., `localhost`)
     - **Port**: Database port (default: `5432`)
     - **Database**: Database name (e.g., `postgres`)
     - **Username**: Your PostgreSQL username
     - **Password**: Your password (must be entered each session for security)
   - Press `Enter` to connect
   - Use `Up/Down` to select saved connections, `Del` to delete them

3. **Write and execute queries**:
   - Type your SQL in the editor pane
   - Press `F5` or `Ctrl+Enter` to execute
   - Results appear in the bottom pane with row count and execution time

4. **Browse your schema**:
   - Use the sidebar (left pane) to explore
   - Press `1`, `2`, or `3` to switch between Databases, Tables, and History tabs
   - Press `Enter` on a table to insert its name into the editor

## Usage

### Layout

```
┌─────────────────────────────────────────────────────────────┐
│ pgrsql  user@localhost:5432/mydb | mydb | public            │
├──────────────┬──────────────────────────────────────────────┤
│ Databases    │ Query Editor                                 │
│ Tables       │                                              │
│ History      │ SELECT * FROM users                          │
│              │ WHERE active = true;                         │
│ ▼ public     │                                              │
│   ├ users    ├──────────────────────────────────────────────┤
│   ├ orders   │ Results (1/1)                                │
│   └ products │ id │ name    │ email          │ active      │
│              │  1 │ Alice   │ alice@test.com │ true        │
│              │  2 │ Bob     │ bob@test.com   │ true        │
├──────────────┴──────────────────────────────────────────────┤
│ 2 rows returned (12.34ms)                                   │
└─────────────────────────────────────────────────────────────┘
```

### Keyboard Shortcuts

#### Global
| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit pgrsql |
| `Ctrl+C` | Open connection dialog |
| `?` | Toggle help overlay |

#### Navigation
| Key | Action |
|-----|--------|
| `Tab` | Next pane (Sidebar → Editor → Results → Sidebar) |
| `Shift+Tab` | Previous pane |

> **Note:** In the Editor, `Tab` inserts spaces. Use `Shift+Tab` to go back to the Sidebar, or execute a query (`F5`) to move to Results.

#### Editor
| Key | Action |
|-----|--------|
| `F5` or `Ctrl+Enter` | Execute query |
| `Ctrl+L` | Clear editor |
| `Ctrl+Up` | Previous query from history |
| `Ctrl+Down` | Next query from history |
| `Ctrl+C` | Copy selection |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste from clipboard |
| `Ctrl+A` | Select all |
| `Ctrl+Left/Right` | Move by word |
| `Home/End` | Move to line start/end |
| `Ctrl+Home/End` | Move to document start/end |
| `F2` or `Alt+V` | Toggle Vim mode |

### Vim Mode

Press `F2` or `Alt+V` in the editor to toggle Vim keybindings. Your preference is saved across sessions.

The status bar shows the current mode: `-- NORMAL --`, `-- INSERT --`, `-- VISUAL --`, or `-- V-LINE --`.

#### Vim Normal Mode
| Key | Action |
|-----|--------|
| `i` / `a` | Insert before/after cursor |
| `I` / `A` | Insert at line start/end |
| `o` / `O` | Open line below/above |
| `h` `j` `k` `l` | Move left/down/up/right |
| `w` / `b` / `e` | Word forward/back/end |
| `0` / `$` / `^` | Line start/end/first non-blank |
| `gg` / `G` | Go to top/bottom |
| `{` / `}` | Paragraph up/down |
| `x` / `X` | Delete char forward/backward |
| `dd` | Delete line |
| `yy` | Yank (copy) line |
| `d`+motion | Delete with motion |
| `c`+motion | Change with motion |
| `p` / `P` | Paste after/before |
| `v` / `V` | Enter Visual / Visual Line mode |

#### Vim Visual Mode
| Key | Action |
|-----|--------|
| Motion keys | Extend selection |
| `d` / `x` | Delete selection |
| `y` | Yank selection |
| `c` | Change selection |
| `Esc` | Back to Normal mode |

#### Sidebar
| Key | Action |
|-----|--------|
| `1` / `2` / `3` | Switch sidebar tab (Databases / Tables / History) |
| `Up/Down` | Navigate items |
| `Enter` | Select/expand item |

#### Results
| Key | Action |
|-----|--------|
| `Arrow keys` | Navigate cells |
| `Ctrl+C` | Copy selected cell value |
| `Ctrl+[` / `Ctrl+]` | Previous / Next result set |
| `PageUp/PageDown` | Scroll results |
| `Home/End` | Jump to first/last column |

### Working with Multiple Databases

1. Press `1` to switch to the Databases tab in the sidebar
2. Use arrow keys to select a database
3. Press `Enter` to switch to that database
4. The schema browser will refresh with the new database's contents

### Working with Schemas

1. Press `2` to switch to the Tables tab
2. Schemas are shown with `▶` (collapsed) or `▼` (expanded)
3. Press `Enter` on a schema to expand/collapse it
4. Press `Enter` on a table to insert `schema.table` into the editor

### Query History

1. Press `3` to switch to the History tab
2. Browse previous queries (most recent at top)
3. Press `Enter` to load a query into the editor
4. Use `Ctrl+Up/Down` in the editor to quickly cycle through history

## Configuration

### Connection Management

- **Last-used connection**: Automatically pre-populated on startup with cursor on the password field
- **Saved connections**: Browse with `Up/Down`, load with `Enter`, delete with `Del`
- **Password security**: Passwords are never saved to disk; you must enter your password each session

Saved connections are stored in:
- **Linux/macOS**: `~/.config/pgrsql/connections.toml`
- **Windows**: `%APPDATA%\pgrsql\connections.toml`

Format:
```toml
[[connections]]
name = "Production"
host = "prod-db.example.com"
port = 5432
database = "myapp"
username = "readonly"
ssl_mode = "Require"

[[connections]]
name = "Local Dev"
host = "localhost"
port = 5432
database = "myapp_dev"
username = "postgres"
ssl_mode = "Disable"
```

### Query History

Query history is stored in:
- **Linux/macOS**: `~/.local/share/pgrsql/history.json`
- **Windows**: `%LOCALAPPDATA%\pgrsql\history.json`

## Examples

### Basic SELECT
```sql
SELECT id, name, email
FROM users
WHERE created_at > '2024-01-01'
ORDER BY name
LIMIT 100;
```

### JOIN Query
```sql
SELECT
    o.id,
    u.name as customer,
    o.total,
    o.created_at
FROM orders o
JOIN users u ON u.id = o.user_id
WHERE o.status = 'completed'
ORDER BY o.created_at DESC;
```

### Aggregate Query
```sql
SELECT
    DATE_TRUNC('month', created_at) as month,
    COUNT(*) as order_count,
    SUM(total) as revenue
FROM orders
GROUP BY 1
ORDER BY 1 DESC;
```

### Schema Inspection
```sql
-- List all tables
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'public';

-- Describe a table
SELECT column_name, data_type, is_nullable
FROM information_schema.columns
WHERE table_name = 'users';
```

## Troubleshooting

### Connection Issues

**"Connection refused"**
- Verify PostgreSQL is running: `pg_isready -h localhost -p 5432`
- Check if the host/port are correct
- Ensure PostgreSQL is accepting connections in `pg_hba.conf`

**"Password authentication failed"**
- Double-check your username and password
- Verify the user exists: `SELECT usename FROM pg_user;`

**"Database does not exist"**
- List available databases: `psql -l`
- Create the database if needed: `createdb mydb`

### Display Issues

**"Garbled characters"**
- Ensure your terminal supports UTF-8
- Try a different terminal emulator (iTerm2, Alacritty, Windows Terminal)

**"Colors look wrong"**
- Set `TERM=xterm-256color` in your shell
- Ensure your terminal supports 256 colors

## Building from Source

### Development Build
```bash
cargo build
./target/debug/pgrsql
```

### Release Build
```bash
cargo build --release
./target/release/pgrsql
```

### Running Tests
```bash
cargo test
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [ratatui](https://github.com/ratatui-org/ratatui) - TUI framework
- [tokio-postgres](https://github.com/sfackler/rust-postgres) - PostgreSQL driver
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
