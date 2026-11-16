//! Where Agentry and Claude Code keep their files.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Agentry's own directory: `$AGENTRY_HOME`, or `~/.agentry`.
pub fn agentry_home() -> PathBuf {
    match std::env::var_os("AGENTRY_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home_dir().join(".agentry"),
    }
}

/// Claude Code's user config directory: `$CLAUDE_CONFIG_DIR`, or `~/.claude`.
pub fn claude_config_dir() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home_dir().join(".claude"),
    }
}

/// `~/.claude/settings.json` (user scope).
pub fn user_settings_file() -> PathBuf {
    claude_config_dir().join("settings.json")
}

/// `<project>/.claude/settings.local.json` (project scope, not committed).
pub fn project_settings_file(project: &Path) -> PathBuf {
    project.join(".claude").join("settings.local.json")
}

pub fn token_file(home: &Path) -> PathBuf {
    home.join("token")
}

pub fn port_file(home: &Path) -> PathBuf {
    home.join("port")
}

/// Read the port and token the running app published. `None` if either is
/// missing or malformed, which callers treat as "app not running".
pub fn read_connection(home: &Path) -> Option<(u16, String)> {
    let port = fs::read_to_string(port_file(home))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let token = fs::read_to_string(token_file(home))
        .ok()?
        .trim()
        .to_string();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// Return the existing token, or create a new random one (file mode 0600).
pub fn ensure_token(home: &Path) -> io::Result<String> {
    let path = token_file(home);
    if let Ok(existing) = fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() >= 32 {
            return Ok(existing);
        }
    }
    create_private_dir(home)?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    write_private(&path, token.as_bytes())?;
    Ok(token)
}

/// Publish the port the app is listening on.
pub fn write_port(home: &Path, port: u16) -> io::Result<()> {
    create_private_dir(home)?;
    write_private(&port_file(home), port.to_string().as_bytes())
}

/// Remove the port file, only if it still names `port` (another instance may
/// have replaced it).
pub fn remove_port(home: &Path, port: u16) {
    let path = port_file(home);
    if let Ok(current) = fs::read_to_string(&path) {
        if current.trim() == port.to_string() {
            let _ = fs::remove_file(path);
        }
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write a file readable only by the owner, atomically (temp file + rename).
pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_created_once_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("agentry");
        let a = ensure_token(&home).unwrap();
        let b = ensure_token(&home).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(token_file(&home))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn connection_requires_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert!(read_connection(home).is_none());
        ensure_token(home).unwrap();
        assert!(read_connection(home).is_none());
        write_port(home, 4321).unwrap();
        let (port, token) = read_connection(home).unwrap();
        assert_eq!(port, 4321);
        assert_eq!(token.len(), 64);
        remove_port(home, 9999);
        assert!(read_connection(home).is_some());
        remove_port(home, 4321);
        assert!(read_connection(home).is_none());
    }
}
