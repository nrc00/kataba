//! `kataba-term`: a ratatui terminal frontend for the Kataba editor stack.
//!
//! Entry point. Sets up crossterm raw mode + alternate screen, restores the
//! terminal on exit (including on panic), and runs the `App` event/render loop.
//!
//! Zero `unsafe`; every failure path is handled rather than panicked.

use std::io::{self, stdout, Write};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use kataba_term::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Restore the terminal even when a panic escapes the run loop.
    let _panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        _panic_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut app = App::new()?;
    app.run(&mut terminal)?;

    // Always restore the terminal before reporting the result.
    let _ = restore_terminal();

    Ok(())
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    io::stdout().flush()?;
    Ok(())
}
