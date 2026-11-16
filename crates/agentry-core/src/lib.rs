//! Shared building blocks for Agentry: file locations, the hook <-> app
//! protocol, Claude Code settings merging and a minimal HTTP client.
//!
//! Everything here is dependency-light on purpose: the hook binary links this
//! crate and must start in a few milliseconds.

pub mod client;
pub mod paths;
pub mod protocol;
pub mod settings;

/// Version of the Agentry binaries, reported by the hook and `/health`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// HTTP header carrying the shared secret from `~/.agentry/token`.
pub const TOKEN_HEADER: &str = "X-Agentry-Token";

/// File name of the hook executable. Install and uninstall recognise Agentry's
/// own entries in `settings.json` by this name.
pub const HOOK_BIN_NAME: &str = "agentry-hook";
