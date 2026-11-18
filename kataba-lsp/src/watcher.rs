//! File system watcher for LSP `workspace/didChangeWatchedFiles`.

use lsp_types::{FileChangeType, FileEvent, WatchKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, warn};

/// Configuration for a watched path.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// The path to watch.
    pub path: PathBuf,
    /// Whether to watch recursively.
    pub recursive: bool,
    /// The kinds of changes to watch for.
    pub kind: WatchKind,
}

/// A file system watcher that sends LSP-compatible file events.
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    tx: mpsc::Sender<Vec<FileEvent>>,
    watches: Arc<RwLock<HashMap<PathBuf, WatchConfig>>>,
}

impl FileWatcher {
    /// Creates a new file watcher with the given event sender.
    pub fn new(tx: mpsc::Sender<Vec<FileEvent>>) -> Result<Self, notify::Error> {
        let (watcher_tx, watcher_rx) = std::sync::mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Err(e) = watcher_tx.send(res) {
                    error!("Failed to send watch event: {}", e);
                }
            },
            notify::Config::default(),
        )?;

        // Spawn a task to process events
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            while let Ok(event_result) = watcher_rx.recv() {
                if let Ok(event) = event_result {
                    let file_events = convert_notify_event(event);
                    if !file_events.is_empty() {
                        if let Err(e) = tx_clone.send(file_events).await {
                            error!("Failed to send file events: {}", e);
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            watcher,
            tx,
            watches: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Returns a reference to the event channel sender.
    #[must_use]
    pub fn sender(&self) -> &mpsc::Sender<Vec<FileEvent>> {
        &self.tx
    }

    /// Adds a path to watch.
    pub async fn watch(&mut self, config: WatchConfig) -> Result<(), notify::Error> {
        let path = config.path.clone();
        let mode = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        self.watcher.watch(&path, mode)?;
        self.watches.write().await.insert(path.clone(), config);
        debug!("Watching path: {:?}", path);
        Ok(())
    }

    /// Removes a watched path.
    pub async fn unwatch(&mut self, path: &Path) -> Result<(), notify::Error> {
        self.watcher.unwatch(path)?;
        self.watches.write().await.remove(path);
        debug!("Unwatched path: {:?}", path);
        Ok(())
    }

    /// Returns all currently watched paths.
    pub async fn watched_paths(&self) -> Vec<PathBuf> {
        self.watches.read().await.keys().cloned().collect()
    }
}

/// Converts a notify event to LSP FileEvents.
fn convert_notify_event(event: Event) -> Vec<FileEvent> {
    let mut file_events = Vec::new();

    for path in event.paths {
        let change_type = match event.kind {
            EventKind::Create(_) => FileChangeType::CREATED,
            EventKind::Modify(_) => FileChangeType::CHANGED,
            EventKind::Remove(_) => FileChangeType::DELETED,
            EventKind::Access(_) => continue, // Not an LSP file change type
            _ => continue,
        };

        // Convert path to URI
        if let Ok(uri) = lsp_types::Url::from_file_path(&path) {
            file_events.push(FileEvent {
                uri,
                typ: change_type,
            });
        } else {
            warn!("Failed to convert path to URI: {:?}", path);
        }
    }

    file_events
}

/// Builder for registering file watchers with an LSP server.
pub struct WatchRegistrationBuilder {
    registrations: Vec<lsp_types::DidChangeWatchedFilesRegistrationOptions>,
}

impl WatchRegistrationBuilder {
    /// Creates a new registration builder.
    pub fn new() -> Self {
        Self {
            registrations: Vec::new(),
        }
    }

    /// Adds a path to watch.
    pub fn watch_path(mut self, path: &Path, recursive: bool, kind: WatchKind) -> Self {
        let glob_pattern = if recursive {
            "**/*".to_string()
        } else {
            "*".to_string()
        };

        // Get the directory path for the glob
        let dir_path = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };

        if let Ok(uri) = lsp_types::Url::from_directory_path(&dir_path) {
            let relative_pattern = lsp_types::RelativePattern {
                base_uri: lsp_types::OneOf::Right(uri),
                pattern: glob_pattern,
            };
            self.registrations
                .push(lsp_types::DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![lsp_types::FileSystemWatcher {
                        glob_pattern: lsp_types::GlobPattern::String(
                            relative_pattern.pattern.clone(),
                        ),
                        kind: Some(kind),
                    }],
                });
        }

        self
    }

    /// Builds the registration options for `client/registerCapability`.
    pub fn build(self) -> lsp_types::RegistrationParams {
        lsp_types::RegistrationParams {
            registrations: self
                .registrations
                .into_iter()
                .enumerate()
                .map(|(i, opts)| lsp_types::Registration {
                    id: format!("watch-{}", i),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: serde_json::to_value(opts).ok(),
                })
                .collect(),
        }
    }
}

impl Default for WatchRegistrationBuilder {
    fn default() -> Self {
        Self::new()
    }
}
