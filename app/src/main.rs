//! Proof of Lobster - Deploy Moltbook agents on Theseus
//!
//!         ██████            ██████
//!         ██▒▒▒▒████████████▒▒▒▒██
//!           ██▒▒▒▒██    ██▒▒▒▒██
//!             ████▒▒▒▒▒▒▒▒████
//!               ██▒▒████▒▒██
//!             ██▒▒██    ██▒▒██
//!            ══ PROOF OF LOBSTER ══

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;

mod agent_assets;
mod app;
mod auth;
mod client;
mod config;
mod extrinsic;
mod moltbook;
mod screens;
mod wallet;

use app::{App, AppMessage};

#[derive(Parser, Debug)]
#[command(name = "lobster")]
#[command(about = "Proof of Lobster - Deploy Moltbook agents on Theseus")]
#[command(version)]
struct Cli {
    /// Server URL (defaults to local development server)
    #[arg(short, long, default_value = "http://localhost:8080")]
    server: String,

    /// Path to agent files directory
    #[arg(short, long, default_value = "agent")]
    agent_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(cli.server, cli.agent_dir).await?;

    // Create message channel for async operations
    let (tx, mut rx) = mpsc::channel::<AppMessage>(32);

    // Initialize session (validates persisted token and fetches balance)
    app.init_session(tx.clone());

    // Run app
    let result = run_app(&mut terminal, &mut app, tx, &mut rx).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tx: mpsc::Sender<AppMessage>,
    rx: &mut mpsc::Receiver<AppMessage>,
) -> Result<()> {
    // Periodic task timers
    let mut last_jwt_check = std::time::Instant::now();
    let mut last_balance_fetch = std::time::Instant::now();
    
    // Check JWT every 30 seconds
    const JWT_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
    // Fetch balance every 12 seconds (~2 blocks)
    const BALANCE_FETCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(12);
    
    loop {
        // Draw UI
        terminal.draw(|f| app.render(f))?;

        // Handle async messages
        while let Ok(msg) = rx.try_recv() {
            app.handle_message(msg, tx.clone()).await?;
        }
        
        // Periodic JWT validation (only if authenticated)
        if app.config.is_authenticated() && last_jwt_check.elapsed() >= JWT_CHECK_INTERVAL {
            last_jwt_check = std::time::Instant::now();
            app.check_session_validity(tx.clone());
        }
        
        // Periodic balance fetch (only if authenticated and has wallet)
        if app.config.is_authenticated() && app.wallet.is_some() && last_balance_fetch.elapsed() >= BALANCE_FETCH_INTERVAL {
            last_balance_fetch = std::time::Instant::now();
            app.refresh_balance(tx.clone());
        }

        // Poll for events with timeout
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Global quit
                    if key.code == KeyCode::Char('q') && app.can_quit() {
                        return Ok(());
                    }

                    // Let app handle key
                    app.handle_key(key.code, tx.clone()).await?;
                }
            }
        }

        // Check if app wants to quit
        if app.should_quit() {
            return Ok(());
        }
    }
}
