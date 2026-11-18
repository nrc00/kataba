//! Subprocess transport management for Language Server instances.

use crate::error::{LspError, Result};
use crate::framing::LspCodec;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::codec::FramedRead;
use tracing::warn;

/// Configuration for launching a language server subprocess.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Command executable name or path (e.g., "rust-analyzer", "gopls").
    pub command: String,
    /// Arguments to pass to the server command.
    pub args: Vec<String>,
    /// Environment variables to set for the server process.
    pub env: Vec<(String, String)>,
    /// Optional working directory for the server process.
    pub current_dir: Option<std::path::PathBuf>,
    /// Root markers to detect project root (e.g., ["Cargo.toml", "package.json"]).
    pub root_markers: Vec<String>,
    /// File types this server handles (e.g., ["rust", "toml"]).
    pub filetypes: Vec<String>,
    /// Server-specific initialization options.
    pub init_options: Option<serde_json::Value>,
    /// Server-specific settings (sent via workspace/didChangeConfiguration).
    pub settings: Option<serde_json::Value>,
}

impl ServerConfig {
    /// Creates a new `ServerConfig` with command and empty arguments.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: Vec::new(),
            current_dir: None,
            root_markers: Vec::new(),
            filetypes: Vec::new(),
            init_options: None,
            settings: None,
        }
    }

    /// Adds an argument to the server command.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Sets root markers for project root detection.
    #[must_use]
    pub fn root_markers(mut self, markers: Vec<String>) -> Self {
        self.root_markers = markers;
        self
    }

    /// Sets file types this server handles.
    #[must_use]
    pub fn filetypes(mut self, types: Vec<String>) -> Self {
        self.filetypes = types;
        self
    }

    /// Sets server-specific initialization options.
    #[must_use]
    pub fn init_options(mut self, options: serde_json::Value) -> Self {
        self.init_options = Some(options);
        self
    }

    /// Sets server-specific settings.
    #[must_use]
    pub fn settings(mut self, settings: serde_json::Value) -> Self {
        self.settings = Some(settings);
        self
    }
}

/// Active transport holding the running subprocess and its I/O streams.
pub struct ProcessTransport {
    /// Framed stream for reading incoming messages from server stdout.
    pub reader: FramedRead<ChildStdout, LspCodec>,
    /// Standard input stream for writing messages to the server.
    pub stdin: ChildStdin,
    /// The running child process.
    child: Child,
}

impl ProcessTransport {
    /// Spawns the language server process and connects standard I/O streams.
    pub fn spawn(config: &ServerConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        if let Some(ref dir) = config.current_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(LspError::Io)?;

        let stdout = child.stdout.take().ok_or_else(|| {
            LspError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Failed to capture server stdout",
            ))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            LspError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Failed to capture server stdin",
            ))
        })?;

        // Asynchronously drain stderr into tracing to prevent pipe deadlocks and terminal clutter
        if let Some(stderr) = child.stderr.take() {
            let server_cmd = config.command.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!(target: "lsp::stderr", server = %server_cmd, "{}", line);
                }
            });
        }

        let reader = FramedRead::new(stdout, LspCodec::new());

        Ok(Self {
            reader,
            stdin,
            child,
        })
    }

    /// Returns the process ID of the child language server, if running.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Splits the transport into its read half, write half, and child handle.
    ///
    /// The event loop drives `reader` on its read task and hands `stdin` to a
    /// dedicated writer task. Keeping the two directions on separate tasks is
    /// what prevents a server that stops draining its stdin (common during
    /// indexing) from deadlocking the read loop.
    #[must_use]
    pub fn into_parts(self) -> (FramedRead<ChildStdout, LspCodec>, ChildStdin, Child) {
        (self.reader, self.stdin, self.child)
    }

    /// Waits for the process to exit with a timeout, killing it if unresponsive.
    pub async fn wait_or_kill(mut self, timeout: Duration) -> Result<()> {
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(err)) => Err(LspError::Io(err)),
            Err(_) => {
                // Hard timeout expired: force kill zombie process
                let _ = self.child.kill().await;
                Ok(())
            }
        }
    }
}
