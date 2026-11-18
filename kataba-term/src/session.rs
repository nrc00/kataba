//! Session persistence for Kataba.
//!
//! Saves and restores open files, cursor positions, and modes cleanly to/from disk.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Persisted record of a single open buffer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BufferSessionRecord {
    pub path: Option<PathBuf>,
    pub name: String,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

/// Serialized session file payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Session {
    pub active_index: usize,
    pub buffers: Vec<BufferSessionRecord>,
}

impl Session {
    /// Save session to given path.
    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(self)
            .map_err(|e| io::Error::other(e.to_string()))?;
        fs::write(path, data)
    }

    /// Load session from path.
    pub fn load_from_file(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let session: Self = toml::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trip() {
        let session = Session {
            active_index: 1,
            buffers: vec![
                BufferSessionRecord {
                    path: Some(PathBuf::from("src/main.rs")),
                    name: "main.rs".into(),
                    cursor_row: 10,
                    cursor_col: 5,
                },
                BufferSessionRecord {
                    path: None,
                    name: "[scratch]".into(),
                    cursor_row: 0,
                    cursor_col: 0,
                },
            ],
        };

        let temp_dir = std::env::temp_dir();
        let session_file = temp_dir.join("kataba_test_session.toml");
        session.save_to_file(&session_file).expect("save should work");

        let loaded = Session::load_from_file(&session_file).expect("load should work");
        assert_eq!(session, loaded);

        let _ = fs::remove_file(session_file);
    }
}
