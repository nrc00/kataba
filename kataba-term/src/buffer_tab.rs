//! Buffer tab management: multiple buffers with a tab bar for switching.
//!
//! A `BufferTab` owns a list of `Buffer`s and tracks which one is active.
//! Buffers can be opened, closed, saved, and switched between. The tab bar
//! renders at the top of the editor area.

use std::path::Path;

use crate::buffer::Buffer;

/// A collection of open buffers with an active index.
pub struct BufferTab {
    buffers: Vec<Buffer>,
    active: usize,
}

impl BufferTab {
    /// Create a new tab set with a single scratch buffer.
    pub fn new() -> Self {
        Self {
            buffers: vec![Buffer::scratch("scratch")],
            active: 0,
        }
    }

    /// Number of open buffers.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Whether there are any buffers (always at least one).
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Active buffer index.
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Active buffer, immutable.
    pub fn active_buffer(&self) -> Option<&Buffer> {
        self.buffers.get(self.active)
    }

    /// Active buffer, mutable.
    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.buffers.get_mut(self.active)
    }

    /// Get a buffer by index.
    pub fn buffer(&self, index: usize) -> Option<&Buffer> {
        self.buffers.get(index)
    }

    /// Switch to the buffer at `index`. Returns false if index is out of range.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.buffers.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// Switch to the next buffer (wraps around).
    pub fn next(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        self.active = (self.active + 1) % self.buffers.len();
    }

    /// Switch to the previous buffer (wraps around).
    pub fn prev(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        self.active = self.active.saturating_sub(1);
        if self.active >= self.buffers.len() {
            self.active = self.buffers.len() - 1;
        }
    }

    /// Open a file in a new buffer. If the file is already open, switch to it.
    pub fn open_file(&mut self, path: &Path) -> Result<usize, std::io::Error> {
        // If already open, switch to it.
        if let Some(idx) = self
            .buffers
            .iter()
            .position(|b| b.file_path.as_ref() == Some(&path.to_path_buf()))
        {
            self.active = idx;
            return Ok(idx);
        }
        let buffer = Buffer::load_file(path)?;
        let idx = self.buffers.len();
        self.buffers.push(buffer);
        self.active = idx;
        Ok(idx)
    }

    /// Open a new scratch buffer.
    pub fn open_scratch(&mut self, name: impl Into<String>) -> usize {
        let buffer = Buffer::scratch(name);
        let idx = self.buffers.len();
        self.buffers.push(buffer);
        self.active = idx;
        idx
    }

    /// Close the buffer at `index`. If it was active, switch to another buffer.
    pub fn close(&mut self, index: usize) -> Option<Buffer> {
        if index >= self.buffers.len() {
            return None;
        }
        let buffer = self.buffers.remove(index);
        if self.buffers.is_empty() {
            // Always keep at least one buffer.
            self.buffers.push(Buffer::scratch("scratch"));
            self.active = 0;
        } else if self.active >= self.buffers.len() {
            self.active = self.buffers.len() - 1;
        }
        Some(buffer)
    }

    /// Close the active buffer.
    pub fn close_active(&mut self) -> Option<Buffer> {
        self.close(self.active)
    }

    /// Save the active buffer.
    pub fn save_active(&mut self) -> Result<(), std::io::Error> {
        if let Some(buf) = self.active_buffer_mut() {
            buf.save(None)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no active buffer",
            ))
        }
    }

    /// Tab bar labels: name with dirty indicator.
    pub fn tab_labels(&self) -> Vec<String> {
        self.buffers
            .iter()
            .map(|b| {
                let marker = if b.view.dirty { "●" } else { " " };
                format!("{marker} {}", b.name)
            })
            .collect()
    }
}

impl Default for BufferTab {
    fn default() -> Self {
        Self::new()
    }
}
