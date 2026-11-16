//! Headless Agentry server for development and testing: runs the same server
//! as the app, without a window, and logs every hook event to stderr.
//!
//!     cargo run -p agentry-server --bin agentry-devserver
//!     RUST_LOG=debug …   # also log each payload's field names

use agentry_server::{init_logging, Server, ServerOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let server = Server::start(ServerOptions::default()).await?;
    eprintln!(
        "agentry-devserver listening on 127.0.0.1:{} — Ctrl-C to stop",
        server.port
    );
    wait_for_stop_signal().await?;
    server.shutdown().await;
    Ok(())
}

/// Ctrl-C, or SIGTERM (e.g. `kill`), so the port file is always cleaned up.
async fn wait_for_stop_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            r = tokio::signal::ctrl_c() => r?,
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
