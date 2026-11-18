use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::StatusLine;
use crate::git::{ChangeKind, Section};
use crate::input::Input;

const SECTION_COLORS: [(Section, Color); 3] = [
    (Section::Staged, Color::Green),
    (Section::Unstaged, Color::Yellow),
    (Section::Untracked, Color::Magenta),
];

pub fn draw_status(
    f: &mut Frame,
    area: Rect,
    head: &crate::git::HeadInfo,
    lines: &[StatusLine],
    state: &mut ListState,
    open_files: &std::collections::HashSet<std::path::PathBuf>,
    section_folded: &[bool],
    msg: Option<&str>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint
            Constraint::Length(1), // HEAD
            Constraint::Length(1), // blank
            Constraint::Min(0),    // sections + files
            Constraint::Length(1), // footer
        ])
        .split(area);

    // ---- hint bar ----
    let hint = Line::from(vec![
        Span::styled("rustgit ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" navigate · "),
        Span::styled("space", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" toggle diff/fold · "),
        Span::styled("s/u", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" stage/unstage · "),
        Span::styled("S/U", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" stage/unstage all · "),
        Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" stage all · "),
        Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" discard · "),
        Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" commit · "),
        Span::styled("d/D", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" diff · "),
        Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" log · "),
        Span::styled("b", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" branch · "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" refresh · "),
        Span::styled("h/?", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" help · "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]);
    let hint = Paragraph::new(hint).block(Block::default().borders(Borders::NONE));
    f.render_widget(hint, chunks[0]);

    // ---- HEAD section ----
    let head_line = Line::from(vec![
        Span::styled("HEAD ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
        Span::raw(": "),
        Span::styled(
            if head.detached { "(detached) ".to_string() } else { format!("{} ", head.branch) },
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(head.abbrev.as_str(), Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::styled(&head.commit_message, Style::default().fg(Color::White)),
    ]);
    let head_widget = Paragraph::new(head_line).block(
        Block::default().borders(Borders::NONE).padding(ratatui::widgets::Padding::left(1)),
    );
    f.render_widget(head_widget, chunks[1]);

    // ---- body: sections + files ----
    let items: Vec<ListItem<'_>> = lines
        .iter()
        .map(|line| match line {
            StatusLine::SectionHeader { section, title, count } => {
                let color = SECTION_COLORS.iter().find(|(s, _)| *s == *section).map(|(_, c)| *c).unwrap_or(Color::White);
                let folded = section_folded[(*section).idx()];
                let arrow = if folded { "▸" } else { "▾" };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(arrow, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(format!("{title} ({count})"), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                ]))
                .style(Style::default().bg(Color::Black))
            }
            StatusLine::Blank => ListItem::new(Line::from(Span::raw(""))),
            StatusLine::File { path, mode, section: _ } => {
                let color = match mode {
                    ChangeKind::Added => Color::Green,
                    ChangeKind::Modified => Color::Yellow,
                    ChangeKind::Deleted => Color::Red,
                };
                let is_open = open_files.contains(path);
                let arrow = if is_open { "▾" } else { "▸" };
                ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(arrow, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(mode.letter().to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(path.to_string_lossy(), Style::default().fg(Color::White)),
                ]))
            }
            StatusLine::Diff(text) => {
                let style = match text.as_bytes().first() {
                    Some(b'+') => Style::default().fg(Color::Green),
                    Some(b'-') => Style::default().fg(Color::Red),
                    Some(b' ') => Style::default().fg(Color::Gray),
                    _ => Style::default().fg(Color::DarkGray),
                };
                ListItem::new(Line::from(Span::styled(text.as_str(), style)))
            }
        })
        .collect();

    let mut list = List::new(items).block(
        Block::default().borders(Borders::ALL).title(" status ").title_alignment(Alignment::Center),
    );
    if state.selected().is_some() {
        list = list.highlight_symbol("▶ ");
    }
    f.render_stateful_widget(list, chunks[3], state);

    // ---- footer ----
    let footer = msg
        .map(|m| Span::styled(m, Style::default().fg(Color::Red)))
        .unwrap_or_else(|| Span::raw("j/k ↑/↓  g/G top/bottom  space open diff  s/u stage/unstage  S/U all  a stage-all  c commit  q quit"));
    let footer = Paragraph::new(footer).block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, chunks[4]);
}

pub fn draw_diff(f: &mut Frame, area: Rect, title: &str, content: &str, scroll: u16) {
    let lines: Vec<&str> = content.lines().collect();
    let visible: Vec<&str> = lines.iter().skip(scroll as usize).cloned().collect();
    let text: Vec<Line> = visible
        .iter()
        .map(|line| {
            let style = match line.as_bytes().first() {
                Some(b'+') => Style::default().fg(Color::Green),
                Some(b'-') => Style::default().fg(Color::Red),
                Some(b' ') => Style::default().fg(Color::White),
                Some(b'\\') => Style::default().fg(Color::DarkGray),
                _ => Style::default(),
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_alignment(Alignment::Center),
        )
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

pub fn draw_commit(f: &mut Frame, area: Rect, input: &Input, msg: Option<&str>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled("commit ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("write message, "),
        Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" submit · "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" cancel"),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // render the multi-line buffer with scrolling
    let scroll = input.scroll();
    let visible_height = chunks[1].height.saturating_sub(0) as usize;
    let total = input.num_lines();
    let start = scroll.min(total);
    let end = (start + visible_height).min(total);
    let mut text: Vec<Line> = Vec::new();
    for i in start..end {
        let line = input.line(i);
        let is_cursor_line = i == input.cursor_pos().0;
        let style = if is_cursor_line {
            Style::default().bg(Color::Blue).fg(Color::White)
        } else {
            Style::default()
        };
        text.push(Line::from(Span::styled(line, style)));
    }
    let editor = Paragraph::new(text).block(
        Block::default().borders(Borders::ALL).title("message").title_alignment(Alignment::Left),
    );
    f.render_widget(editor, chunks[1]);

    // place the terminal cursor at the cursor position within the visible area
    let (cl, cc) = input.cursor_pos();
    if cl >= start && cl < end {
        let x = chunks[1].left() + cc as u16 + 1;
        let y = chunks[1].top() + (cl - start) as u16 + 1;
        f.set_cursor(
            x.min(chunks[1].right().saturating_sub(1)),
            y.min(chunks[1].bottom().saturating_sub(1)),
        );
    }

    let footer = msg
        .map(|m| Span::styled(m, Style::default().fg(Color::Red)))
        .unwrap_or_else(|| Span::raw("Enter = newline  Ctrl-k cut to end  Ctrl-u clear line  c submit · Esc cancel"));
    let footer = Paragraph::new(footer).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

pub fn draw_list_view(f: &mut Frame, area: Rect, title: &str, items: &[String], state: &mut ListState, help: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|s| ListItem::new(Span::raw(s.as_str())))
        .collect();
    let list = List::new(list_items).block(Block::default().borders(Borders::ALL));
    f.render_stateful_widget(list, chunks[1], state);

    let footer = Paragraph::new(Span::raw(help)).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

pub fn draw_help(f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        "help",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let help_text = [
        "status view:",
        "  j/k          move down/up",
        "  g/G          go to top/bottom",
        "  space        open file diff (inline) / toggle section fold",
        "  s            stage selected file",
        "  u            unstage selected file",
        "  S            stage all unstaged",
        "  U            unstage all staged",
        "  a            stage all changes",
        "  x            discard selected file (worktree checkout / delete untracked)",
        "  c            open commit editor",
        "  d            diff selected file (worktree vs index)",
        "  D            diff selected staged file (index vs HEAD)",
        "  l            view commit log",
        "  b            view / switch branches",
        "  r            refresh status",
        "  h/?          this help",
        "  q            quit",
        "",
        "commit editor:",
        "  Enter  newline   c  submit commit   Esc  cancel",
        "",
        "diff / log / branches:",
        "  j/k  scroll   Enter/b  back to status   q  quit",
    ];
    let items: Vec<ListItem> = help_text.iter().map(|s| ListItem::new(Span::raw(*s))).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" keys ").title_alignment(Alignment::Center));
    f.render_widget(list, chunks[1]);

    let footer = Paragraph::new(Span::raw("press b or Esc to return")).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}
