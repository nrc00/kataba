//! treesitter-viz: a throwaway read-only TUI file viewer that renders a
//! document with tree-sitter syntax highlighting.
//!
//! Built on `kataba-core` (rope buffer), `kataba-treesitter` (grammar loading
//! and parsing), and `ratatui`. This is demo-only glue next to the real
//! `kataba-term` front-end planned for 0.1; it exists to see the syntax stack
//! working end to end outside of tests.
//!
//! ```text
//! Usage: treesitter-viz [--lang <name>] [--dump] <file>
//! ```
//!
//! Grammars and highlight queries are read from the sibling Helix checkout by
//! default (`<repo>/helix/runtime/{grammars,queries}`), or from
//! `KATABA_TREESITTER_GRAMMAR_DIR` / `KATABA_TREESITTER_QUERY_DIR`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kataba_core::Rope;
use kataba_treesitter::{SyntaxParser, load_language};

mod highlight;
mod viewer;

use highlight::{HighlightSpan, Highlighter};
use viewer::Viewer;

const HELP: &str = "\
treesitter-viz - throwaway read-only tree-sitter highlight viewer

Usage:
    treesitter-viz [--lang <name>] [--dump] <file>

Options:
    --lang <name>   Force a grammar name (e.g. json, rust) instead of
                    detecting it from the file extension.
    --dump          Print the highlight spans instead of opening the TUI.
    -h, --help      Show this help.

Keys:
    q / esc         quit
    j / down, k / up    scroll
    space / pgdn    page down
    pgup            page up
    g / home        jump to top
    G / end         jump to bottom
    h               toggle highlighting

Grammars are loaded from $KATABA_TREESITTER_GRAMMAR_DIR (default:
<repo>/helix/runtime/grammars); highlight queries from
$KATABA_TREESITTER_QUERY_DIR (default: <repo>/helix/runtime/queries).
";

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS, // --help
        Err(message) => {
            eprintln!("treesitter-viz: {message}\n\n{HELP}");
            return ExitCode::FAILURE;
        }
    };
    match execute(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("treesitter-viz: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Command-line arguments.
struct Args {
    path: PathBuf,
    lang: Option<String>,
    dump: bool,
}

/// Returns `Ok(None)` when the help text was printed.
fn parse_args() -> Result<Option<Args>, String> {
    let mut path: Option<PathBuf> = None;
    let mut lang = None;
    let mut dump = false;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(None);
            }
            "--dump" => dump = true,
            "--lang" => {
                lang = Some(args.next().ok_or("`--lang` requires a value")?);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag `{flag}`"));
            }
            file => {
                if path.is_some() {
                    return Err("expected exactly one <file> argument".to_string());
                }
                path = Some(PathBuf::from(file));
            }
        }
    }

    let path = path.ok_or("missing <file> argument")?;
    Ok(Some(Args { path, lang, dump }))
}

fn execute(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = load(&args.path, args.lang.as_deref())?;

    if args.dump {
        dump(&args.path, &loaded);
        return Ok(());
    }

    let mut viewer = Viewer::new(
        args.path.display().to_string(),
        loaded.rope,
        loaded.spans,
        loaded.has_highlighter,
        loaded.lang,
        loaded.abi,
        loaded.note,
    );
    let mut terminal = ratatui::try_init()?;
    let result = viewer.run(&mut terminal);
    ratatui::try_restore()?;
    result?;
    Ok(())
}

/// Everything the viewer or `--dump` needs, already resolved.
struct Loaded {
    text: String,
    rope: Rope,
    lang: Option<String>,
    abi: Option<usize>,
    spans: Vec<HighlightSpan>,
    has_highlighter: bool,
    /// Human-readable explanation when highlighting is partial or missing.
    note: String,
}

fn load(path: &Path, lang_override: Option<&str>) -> Result<Loaded, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let rope = Rope::from_str(&text);
    let mut loaded = Loaded {
        text,
        rope,
        lang: None,
        abi: None,
        spans: Vec::new(),
        has_highlighter: false,
        note: String::new(),
    };

    let lang = match lang_override {
        Some(lang) => Some(lang.to_string()),
        None => detect_language(path).map(str::to_string),
    };
    let Some(lang) = lang else {
        loaded.note = "no grammar mapping for this file type".to_string();
        return Ok(loaded);
    };
    loaded.lang = Some(lang.clone());

    let grammar_path = grammar_dir().join(format!("{lang}{}", std::env::consts::DLL_SUFFIX));
    if !grammar_path.exists() {
        loaded.note = format!("no compiled grammar at {}", grammar_path.display());
        return Ok(loaded);
    }
    let grammar = match load_language(&lang, &grammar_path) {
        Ok(grammar) => grammar,
        Err(error) => {
            loaded.note = format!("grammar: {error}");
            return Ok(loaded);
        }
    };
    loaded.abi = Some(grammar.abi());

    let mut parser = SyntaxParser::new(grammar.language())?;
    let tree = parser.parse(&loaded.rope)?;

    let query_path = query_dir().join(&lang).join("highlights.scm");
    if !query_path.exists() {
        loaded.note = format!("no highlights.scm at {}", query_path.display());
        return Ok(loaded);
    }
    match Highlighter::load(grammar.language(), &query_path) {
        Ok(highlighter) => {
            loaded.spans = highlighter.spans(&tree, &loaded.text);
            loaded.has_highlighter = true;
        }
        Err(error) => {
            loaded.note = format!("query: {error}");
        }
    }
    Ok(loaded)
}

/// Maps a file extension to a Helix-compatible grammar name.
fn detect_language(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let lang = match extension.as_str() {
        "json" => "json",
        "toml" => "toml",
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "md" | "markdown" => "markdown",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" => "bash",
        "css" => "css",
        "html" | "htm" => "html",
        "lua" => "lua",
        "nix" => "nix",
        "zig" => "zig",
        "java" => "java",
        "rb" => "ruby",
        "php" => "php",
        _ => return None,
    };
    Some(lang)
}

/// `<repo>/helix/runtime`.
fn helix_runtime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../helix/runtime")
}

fn grammar_dir() -> PathBuf {
    match std::env::var_os("KATABA_TREESITTER_GRAMMAR_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => helix_runtime_dir().join("grammars"),
    }
}

fn query_dir() -> PathBuf {
    match std::env::var_os("KATABA_TREESITTER_QUERY_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => helix_runtime_dir().join("queries"),
    }
}

/// Prints the resolved highlight spans, for headless verification.
fn dump(path: &Path, loaded: &Loaded) {
    println!("file:  {}", path.display());
    if let Some(lang) = &loaded.lang {
        println!("lang:  {lang}");
    }
    if let Some(abi) = loaded.abi {
        println!("abi:   {abi}");
    }
    if !loaded.note.is_empty() {
        println!("note:  {}", loaded.note);
    }
    println!("spans: {}", loaded.spans.len());
    if loaded.spans.is_empty() {
        return;
    }

    const LIMIT: usize = 80;
    let starts = line_starts(&loaded.text);
    for span in loaded.spans.iter().take(LIMIT) {
        let (line, column) = line_col(&starts, span.start);
        let mut end = (span.start + 40).min(span.end);
        while !loaded.text.is_char_boundary(end) {
            end -= 1;
        }
        let snippet = &loaded.text[span.start..end];
        let escaped = snippet.replace('\n', "\\n").replace('\r', "\\r");
        let ellipsis = if end < span.end { "..." } else { "" };
        println!(
            "{line:>4}:{column:<3} {:?} {escaped}{ellipsis}",
            span.style.fg
        );
    }
    if loaded.spans.len() > LIMIT {
        println!("... {} more", loaded.spans.len() - LIMIT);
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
    );
    starts
}

/// 1-based line and column (in bytes) for a byte offset.
fn line_col(starts: &[usize], byte: usize) -> (usize, usize) {
    let index = starts
        .partition_point(|&start| start <= byte)
        .saturating_sub(1);
    (index + 1, byte - starts[index])
}
