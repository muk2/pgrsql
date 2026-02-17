mod db;
mod editor;
mod ui;

use crate::db::ConnectionManager;
use crate::ui::App;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

/// A beautiful TUI SQL editor for PostgreSQL
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Auto-connect to a saved connection by name
    #[arg(long = "connect")]
    connect: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    tracing_subscriber::fmt::init();

    // Parse CLI args (before entering raw mode so --help / errors print normally)
    let cli = Cli::parse();

    // Resolve auto-connect config if requested
    let auto_connect_config = if let Some(ref name) = cli.connect {
        let saved = ConnectionManager::load_saved_connections().unwrap_or_default();
        let mut config = match saved
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            Some(c) => c,
            None => {
                eprintln!("Error: no saved connection named {:?}", name);
                eprintln!("Saved connections:");
                for c in ConnectionManager::load_saved_connections().unwrap_or_default() {
                    eprintln!("  - {}", c.name);
                }
                std::process::exit(1);
            }
        };

        // Resolve password: PGPASSWORD env var, then interactive prompt
        if config.password.is_empty() {
            if let Ok(pw) = std::env::var("PGPASSWORD") {
                config.password = pw;
            } else {
                let prompt = format!("Password for {}: ", config.display_string());
                config.password = rpassword::read_password_from_tty(Some(&prompt))?;
            }
        }

        Some(config)
    } else {
        None
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();

    // Auto-connect if requested
    if let Some(config) = auto_connect_config {
        app.try_auto_connect(config).await;
    }

    // Run the app
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events (ignore release/repeat)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Global quit: Ctrl+Q or Ctrl+D
                if (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('d'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return Ok(());
                }

                // Handle input based on current focus
                app.handle_input(key).await?;

                if app.should_quit {
                    return Ok(());
                }
            }
        }

        // Process any pending async operations
        app.tick().await?;
    }
}
