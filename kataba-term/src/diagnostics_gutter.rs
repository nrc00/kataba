//! Diagnostics gutter rendering component for Kataba.
//!
//! Provides file-line mapped diagnostic indicators (Error, Warning, Info, Hint)
//! for rendering in the editor gutter margin alongside line numbers.

use ratatui::style::{Color, Style};

/// Severity level of an in-buffer diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl DiagnosticSeverity {
    /// Return the single-char sign icon for gutter display.
    #[must_use]
    pub const fn sign(self) -> &'static str {
        match self {
            Self::Error => "E",
            Self::Warning => "W",
            Self::Info => "I",
            Self::Hint => "H",
        }
    }

    /// Return styling for gutter display.
    #[must_use]
    pub const fn style(self) -> Style {
        match self {
            Self::Error => Style::new().fg(Color::Red),
            Self::Warning => Style::new().fg(Color::Yellow),
            Self::Info => Style::new().fg(Color::Blue),
            Self::Hint => Style::new().fg(Color::DarkGray),
        }
    }
}

/// Gutter entry tracking diagnostic state on a given zero-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDiagnostic {
    pub line: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// Gutter query and cache helper.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticsGutter {
    diagnostics: Vec<LineDiagnostic>,
}

impl DiagnosticsGutter {
    /// Construct empty gutter cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Clear all stored diagnostics.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    /// Add a diagnostic line entry.
    pub fn add(&mut self, line: usize, severity: DiagnosticSeverity, message: String) {
        self.diagnostics.push(LineDiagnostic {
            line,
            severity,
            message,
        });
    }

    /// Query the most severe diagnostic for a zero-based line number.
    #[must_use]
    pub fn get_highest_severity(&self, line: usize) -> Option<&LineDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.line == line)
            .min_by_key(|d| d.severity)
    }

    /// Number of diagnostics currently cached.
    #[must_use]
    pub fn count(&self) -> usize {
        self.diagnostics.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_highest_severity_selected() {
        let mut gutter = DiagnosticsGutter::new();
        gutter.add(10, DiagnosticSeverity::Warning, "unused var".into());
        gutter.add(10, DiagnosticSeverity::Error, "type mismatch".into());
        gutter.add(10, DiagnosticSeverity::Hint, "can be const".into());

        let top = gutter.get_highest_severity(10).expect("should find diag");
        assert_eq!(top.severity, DiagnosticSeverity::Error);
        assert_eq!(top.severity.sign(), "E");
    }

    #[test]
    fn gutter_clear() {
        let mut gutter = DiagnosticsGutter::new();
        gutter.add(1, DiagnosticSeverity::Info, "info".into());
        assert_eq!(gutter.count(), 1);
        gutter.clear();
        assert_eq!(gutter.count(), 0);
    }
}
