//! Per-language keymap loader and overlay support for Kataba.
//!
//! Loads language-specific keybinding overrides from `keymaps/<lang>.toml`
//! and merges them over default or user global bindings.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::Deserialize;

use crate::keybind::KeyBind;

/// Per-language keymap file structure.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LanguageKeymapFile {
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub bindings: Vec<KeyBind>,
}

/// Registry storing language-specific binding overlays.
#[derive(Debug, Default, Clone)]
pub struct LanguageKeymapRegistry {
    /// Maps language name to list of keybindings.
    pub overlays: HashMap<String, Vec<KeyBind>>,
}

impl LanguageKeymapRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            overlays: HashMap::new(),
        }
    }

    /// Register bindings for a specific language.
    pub fn register(&mut self, language: &str, bindings: Vec<KeyBind>) {
        self.overlays.insert(language.to_lowercase(), bindings);
    }

    /// Load language keymaps from a directory containing `<lang>.toml` files.
    pub fn load_directory(&mut self, dir: &Path) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(parsed) = toml::from_str::<LanguageKeymapFile>(&content) {
                            let lang = if parsed.language.is_empty() {
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                parsed.language
                            };
                            if !lang.is_empty() {
                                self.register(&lang, parsed.bindings);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Lookup bindings for a language.
    #[must_use]
    pub fn get(&self, language: &str) -> Option<&[KeyBind]> {
        self.overlays.get(&language.to_lowercase()).map(|v| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_keymap_registration() {
        let mut reg = LanguageKeymapRegistry::new();
        reg.register(
            "rust",
            vec![KeyBind {
                key: "c".into(),
                modifiers: "ctrl".into(),
                description: "rust run".into(),
            }],
        );

        let rust_binds = reg.get("Rust").expect("case-insensitive lookup");
        assert_eq!(rust_binds.len(), 1);
        assert_eq!(rust_binds[0].description, "rust run");
        assert!(reg.get("python").is_none());
    }
}
