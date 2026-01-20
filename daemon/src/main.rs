mod config;
mod connection;
mod git;
mod handlers;
mod opencode;
mod protocol;
mod state;
mod terminal;

use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::{Args, SessionsConfig};
use state::DaemonState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse CLI arguments
    let args = Args::parse();

    // Determine token
    let token = if args.require_auth() {
        match &args.token {
            Some(t) => Some(t.clone()),
            None => {
                error!("Token required. Use --token or set MAESTRO_DAEMON_TOKEN");
                std::process::exit(1);
            }
        }
    } else {
        warn!("Auth disabled (--insecure-no-auth). Do not use in production!");
        None
    };

    // Load sessions config
    let data_dir = args.data_dir();
    info!("Data directory: {}", data_dir.display());

    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
        info!("Created data directory: {}", data_dir.display());
    }

    let sessions_config = SessionsConfig::load(&data_dir)?;
    let sessions = sessions_config.to_session_infos();
    info!("Loaded {} session(s)", sessions.len());

    // Validate session paths exist
    for session in &sessions {
        let path = std::path::Path::new(&session.path);
        if !path.exists() {
            warn!("Session path does not exist: {}", session.path);
        }
    }

    // Create shared state
    let state = Arc::new(DaemonState::new(token, sessions));

    // Bind TCP listener
    let listener = TcpListener::bind(&args.listen).await?;
    info!("Listening on {}", args.listen);

    // Accept loop
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    connection::handle_client(stream, state).await;
                });
            }
            Err(e) => {
                error!("Accept error: {e}");
            }
        }
    }
}
