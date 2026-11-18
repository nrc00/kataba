//! A buffer: an `EditorView` plus file metadata, syntax state, and lifecycle.

use std::path::{Path, PathBuf};

use crate::editor::EditorView;
use crate::syntax::{detect_language, SyntaxCache};

/// A single editable document buffer.
pub struct Buffer {
    /// The editing state (rope, selection, history, mode, scroll, syntax).
    pub view: EditorView,
    /// Path the buffer was loaded from, if any.
    pub file_path: Option<PathBuf>,
    /// Display name shown in the bufferline.
    pub name: String,
}

impl Buffer {
    /// Create a new empty scratch buffer.
    pub fn scratch(name: impl Into<String>) -> Self {
        let view = EditorView::new("");
        Self {
            view,
            file_path: None,
            name: name.into(),
        }
    }

    /// Load a file from disk into a new buffer.
    pub fn load_file(path: &Path) -> Result<Self, std::io::Error> {
        let text = std::fs::read_to_string(path)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        let mut view = EditorView::new(&text);
        if let Some(lang_id) = detect_language(path) {
            let cache = SyntaxCache::new();
            let doc = view.doc.clone();
            let _ = view.syntax.set_language(lang_id, &cache, &doc);
        }
        Ok(Self {
            view,
            file_path: Some(path.to_path_buf()),
            name,
        })
    }

    /// Save the buffer to its file path, or to `path` if none is set.
    pub fn save(&mut self, path: Option<&Path>) -> Result<(), std::io::Error> {
        let text = self.view.doc.slice_text(0, self.view.doc.len());
        let target = match path {
            Some(p) => {
                self.file_path = Some(p.to_path_buf());
                p
            }
            None => self.file_path.as_deref().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file path")
            })?,
        };
        std::fs::write(target, text)?;
        if let Some(name) = self.file_path.as_ref().and_then(|p| p.file_name().and_then(|n| n.to_str()))
        {
            self.name = name.to_string();
        }
        if let Some(lang_id) = detect_language(target) {
            let cache = SyntaxCache::new();
            let doc = self.view.doc.clone();
            let _ = self.view.syntax.set_language(lang_id, &cache, &doc);
        }
        self.view.dirty = false;
        Ok(())
    }

    /// Mark the buffer as dirty (has unsaved changes).
    pub fn set_dirty(&mut self, dirty: bool) {
        self.view.dirty = dirty;
    }

    /// Whether the buffer is dirty.
    pub fn is_dirty(&self) -> bool {
        self.view.dirty
    }

    /// Primary cursor head as a document char offset.
    pub fn head(&self) -> usize {
        self.view.head()
    }

    /// Number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.view.line_count()
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::scratch("scratch")
    }
}
