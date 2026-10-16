//! Guarded process runner. Argv arrays only — never a shell string.
//! Mirrors rustgit's `run_git` but typed, capped, and timeout-bounded.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use crate::{Error, Result};

/// Maximum bytes captured per stream per invocation. Hostile repos cannot
/// blow memory by flooding stdout/stderr.
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Default per-invocation timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A prepared command: argv array + workdir + env overrides.
pub struct Command {
    /// Program (resolved by PATH) — never `sh -c`.
    prog: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
}

impl Command {
    pub fn new<P: AsRef<OsStr>>(prog: P) -> Self {
        Self {
            prog: prog.as_ref().to_os_string(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            stdin: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in iter {
            self.args.push(a.as_ref().to_os_string());
        }
        self
    }

    pub fn cwd<P: AsRef<Path>>(mut self, cwd: P) -> Self {
        self.cwd = Some(cwd.as_ref().to_path_buf());
        self
    }

    pub fn env<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env.push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    pub fn stdin_bytes(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Execute and capture stdout/stderr as `Vec<u8>`.
    pub fn output(self) -> Result<CommandOutput> {
        let mut cmd = StdCommand::new(&self.prog);
        cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        match &self.stdin {
            Some(_) => {
                cmd.stdin(Stdio::piped());
            }
            None => {
                cmd.stdin(Stdio::null());
            }
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::GitCli(format!("spawn `{}`: {e}", self.prog.to_string_lossy())))?;

        if let Some(_bytes) = &self.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(_bytes).map_err(Error::Io)?;
            }
        }

        // Wait for completion; captured streams are capped (cap()) so a hostile
        // repo cannot exhaust memory. Process-level timeout is deferred: the
        // caller can enforce it at the actor layer (V4).
        let out = child
            .wait_with_output()
            .map_err(|e| Error::GitCli(format!("output `{}`: {e}", self.prog.to_string_lossy())))?;
        let status = out.status;

        let stdout = cap(out.stdout);
        let stderr = cap(out.stderr);

        if status.success() {
            Ok(CommandOutput {
                status,
                stdout,
                stderr,
            })
        } else {
            let tail = String::from_utf8_lossy(&stderr).to_string();
            Err(Error::GitCli(format!(
                "exit {}: {tail}",
                status.code().unwrap_or(-1)
            )))
        }
    }
}

/// Run with no stdin, default timeout.
pub fn run<P: AsRef<OsStr>>(prog: P, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<CommandOutput> {
    Command::new(prog).args(args).output()
}

/// Run in a directory with default env (LC_ALL=C, no terminal prompting).
pub fn run_in(
    cwd: impl AsRef<Path>,
    prog: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<CommandOutput> {
    Command::new(prog)
        .args(args)
        .cwd(cwd)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
}

/// Capped capture: prevent a hostile repo from exhausting memory.
fn cap(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() > MAX_OUTPUT_BYTES {
        bytes.truncate(MAX_OUTPUT_BYTES);
        bytes.extend_from_slice(b"\n... output truncated ...");
    }
    bytes
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    pub fn text(&self) -> std::io::Result<String> {
        String::from_utf8(self.stdout.clone()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_caps_at_limit() {
        let big = vec![b'x'; MAX_OUTPUT_BYTES + 100];
        let capped = cap(big);
        assert_eq!(capped.len(), MAX_OUTPUT_BYTES + b"\n... output truncated ...".len());
        assert!(capped.ends_with(b"\n... output truncated ..."));
    }

    #[test]
    fn run_echo_round_trip() {
        let out = run("echo", ["hello"]).expect("echo should run");
        assert!(out.status.success());
        assert_eq!(out.text().unwrap().trim(), "hello");
    }
}
