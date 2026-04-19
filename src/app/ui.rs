use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;

#[derive(Copy, Clone)]
enum Tier {
    Info,
    Warning,
    Danger,
}

/// Build a styled hint line from a string using `[key]` markup:
/// bracketed tokens become bold colored keys (Enter/Esc use accent color,
/// others use a regular key color); the rest of the text is dim.
fn styled_hint(s: &str) -> Line<'_> {
    let accent_color = Color::Yellow;
    let body_color = Color::DarkGray;
    let mut spans: Vec<Span> = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        if open > 0 {
            spans.push(Span::styled(&rest[..open], Style::default().fg(body_color)));
        }
        if let Some(rel_close) = rest[open..].find(']') {
            let close = open + rel_close;
            let key = &rest[open + 1..close];
            let is_accent = key.eq_ignore_ascii_case("Enter")
                || key.eq_ignore_ascii_case("Esc")
                || key.contains("Enter");
            let style = if is_accent {
                Style::default().fg(accent_color).bold()
            } else {
                Style::default().fg(body_color)
            };
            spans.push(Span::styled(format!("[{}]", key), style));
            rest = &rest[close + 1..];
        } else {
            spans.push(Span::styled(rest, Style::default().fg(body_color)));
            rest = "";
            break;
        }
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest, Style::default().fg(body_color)));
    }
    Line::from(spans)
}

fn overlay_block(title: &str, tier: Tier) -> Block<'_> {
    let color = match tier {
        Tier::Info => Color::Cyan,
        Tier::Warning => Color::Yellow,
        Tier::Danger => Color::Red,
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(color))
        .title(title.to_string())
}

#[inline]
fn next_scroll_top(prev_top: u16, cursor: u16, len: u16) -> u16 {
    if cursor < prev_top {
        cursor
    } else if prev_top.saturating_add(len) <= cursor {
        cursor.saturating_add(1).saturating_sub(len)
    } else {
        prev_top
    }
}

fn sync_textarea_hardware_cursor(
    textarea: &tui_textarea::TextArea<'_>,
    area: Rect,
    prev_top: (u16, u16),
) -> ((u16, u16), (u16, u16)) {
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let inner_w = inner.width.max(1);
    let inner_h = inner.height.max(1);

    let (row, col) = textarea.cursor();
    let row = row as u16;
    let line = textarea
        .lines()
        .get(row as usize)
        .map(String::as_str)
        .unwrap_or("");
    let display_col = line
        .chars()
        .take(col)
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0) as u16)
        .fold(0u16, u16::saturating_add);

    let top_row = next_scroll_top(prev_top.0, row, inner_h);
    let top_col = next_scroll_top(prev_top.1, display_col, inner_w);
    let x = inner
        .x
        .saturating_add(display_col.saturating_sub(top_col))
        .min(inner.x.saturating_add(inner.width.saturating_sub(1)));
    let y = inner
        .y
        .saturating_add(row.saturating_sub(top_row))
        .min(inner.y.saturating_add(inner.height.saturating_sub(1)));
    ((x, y), (top_row, top_col))
}

pub fn render(f: &mut Frame, app: &mut App) {
    let agent_rows = app.agents.len().max(1) as u16;
    let agents_height = (agent_rows + 2).min(f.area().height.saturating_sub(5));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),             // header
            Constraint::Length(agents_height), // agents list (fits content)
            Constraint::Min(3),                // input (fills remaining space)
            Constraint::Length(1),             // hint bar
        ])
        .split(f.area());

    // Header.
    let header = Paragraph::new(Line::from(vec![
        Span::styled("qmux", Style::default().fg(Color::Cyan).bold()),
        Span::raw("  window: "),
        Span::styled(&app.window_id, Style::default().fg(Color::Yellow)),
    ]));
    f.render_widget(header, chunks[0]);

    // Main agent list.
    let lines: Vec<Line> = if app.agents.is_empty() {
        vec![Line::from(Span::styled(
            "(no agents — press 's' to spawn)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.agents
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let mark = if a.selected { "[x]" } else { "[ ]" };
                let arrow = if Some(i) == app.cursor { "▶ " } else { "  " };
                Line::from(vec![
                    Span::raw(format!("{}{} ", arrow, mark)),
                    Span::styled(&a.name, Style::default().fg(Color::Green)),
                    Span::raw("  "),
                    Span::styled(&a.preset, Style::default().fg(Color::Blue)),
                    Span::raw("  "),
                    Span::styled(&a.pane_id, Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect()
    };
    let body = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Agents"));
    f.render_widget(body, chunks[1]);

    use crate::app::mode::Mode;

    // Input.
    let in_insert = matches!(app.mode, Mode::Insert);
    let input_title = {
        let mut spans = vec![Span::raw("Prompt ")];
        if in_insert {
            spans.push(Span::styled(
                "[INSERT]",
                Style::default().fg(Color::Green).bold(),
            ));
        } else {
            let dim = Style::default().fg(Color::DarkGray);
            let key_i = Style::default().fg(Color::Yellow).bold();
            let key_f = Style::default().fg(Color::Magenta).bold();
            spans.push(Span::styled("(", dim));
            spans.push(Span::styled("[i]", key_i));
            spans.push(Span::styled(" edit · ", dim));
            spans.push(Span::styled("[f]", key_f));
            spans.push(Span::styled(" forward)", dim));
        }
        Line::from(spans)
    };
    app.input
        .set_block(Block::default().borders(Borders::ALL).title(input_title));
    let input_cursor_style = if in_insert {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    app.input.set_cursor_style(input_cursor_style);
    app.input.set_cursor_line_style(Style::default());
    f.render_widget(&app.input, chunks[2]);
    if in_insert {
        let (cursor, top) =
            sync_textarea_hardware_cursor(&app.input, chunks[2], app.input_viewport_top);
        app.input_viewport_top = top;
        f.set_cursor_position(cursor);
    }

    // Hint bar.
    let default_hint = match &app.mode {
        Mode::Insert => "[Esc] normal · [Enter] send · [Shift+Enter] newline · [?] help",
        _ => "[s] spawn · [a] adopt · [x] kill · [L] layout · [R] refresh · [space] select · [Enter] send · [?] help · [Q] quit",
    };
    let hint_text: Line = if let Some(s) = &app.status {
        if s.contains("error") || s.contains("failed") {
            Line::from(Span::styled(s.clone(), Style::default().fg(Color::Red)))
        } else {
            Line::from(Span::styled(s.clone(), Style::default().fg(Color::Yellow)))
        }
    } else {
        styled_hint(default_hint)
    };
    let hint = Paragraph::new(hint_text);
    f.render_widget(hint, chunks[3]);

    if let Mode::Spawn(st) = &app.mode {
        render_spawn_overlay(f, st);
    }
    if let Mode::Adopt(st) = &app.mode {
        render_adopt_overlay(f, st);
    }
    if let Mode::QuitConfirm { kill_all } = &app.mode {
        if *kill_all {
            render_center_prompt(
                f,
                "[x] Quit + kill all agents",
                Tier::Danger,
                confirm_body("All agents will be killed."),
            );
        } else {
            render_center_prompt(
                f,
                "[!] Quit",
                Tier::Warning,
                confirm_body("Agents remain running."),
            );
        }
    }
    if let Mode::KillConfirm { name, .. } = &app.mode {
        render_center_prompt(
            f,
            &format!("[x] Kill agent '{}'", name),
            Tier::Danger,
            confirm_body(""),
        );
    }
    if matches!(app.mode, Mode::Forward(_)) {
        let agents_snap = app.agents.clone();
        if let Mode::Forward(fw) = &mut app.mode {
            render_forward_overlay(f, &agents_snap, fw);
        }
    }
    if let Mode::Help = &app.mode {
        let area = f.area();
        f.render_widget(ratatui::widgets::Clear, area);
        let content = vec![
            Line::from(Span::styled(
                "qmux — keybindings    (j/k PgUp/PgDn g G to scroll · Esc ? q to close)",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from("Normal mode:"),
            Line::from("  i         enter insert mode (type prompt)"),
            Line::from("  s         spawn agent"),
            Line::from("  a         adopt an existing pane as an agent"),
            Line::from("  space     toggle selection on focused agent"),
            Line::from("  Enter     send current input to selected agents"),
            Line::from("  f         forward flow"),
            Line::from("  x         kill focused agent (confirm)"),
            Line::from("  L         cycle pane layout (main-h-mirrored / main-h / main-v / main-v-mirrored)"),
            Line::from("  l         re-apply current layout (no cycle)"),
            Line::from("  R         refresh agent list"),
            Line::from("  \u{2191}/\u{2193} k/j   move cursor"),
            Line::from("  J / K     move focused agent down / up (swap panes)"),
            Line::from("  ?         toggle this help"),
            Line::from("  q         quit controller (agents keep running, confirm)"),
            Line::from("  Q         quit controller + kill all agents (confirm, destructive)"),
            Line::from(""),
            Line::from("Insert mode (full text-editor bindings via tui-textarea):"),
            Line::from("  Esc                               back to normal mode"),
            Line::from("  Enter                             send prompt and return to normal"),
            Line::from("  Shift+Enter / Alt+Enter           insert newline"),
            Line::from("  Ctrl+U / Ctrl+R                   undo / redo"),
            Line::from("  Ctrl+Y / Ctrl+X / Ctrl+C          paste / cut / copy (textarea yank buffer)"),
            Line::from(""),
            Line::from("Forward edit preview (full text-editor bindings):"),
            Line::from("  Enter                             continue to target picker"),
            Line::from("  Shift+Enter / Alt+Enter           insert newline"),
            Line::from("  Ctrl+G                            delegate to copy-mode in source pane"),
            Line::from("  Ctrl+P                            pull latest tmux buffer into preview (overrides cursor-up)"),
            Line::from("  Esc                               cancel forward"),
            Line::from("  (other keys)                      see Insert mode — same editing bindings"),
        ];
        // Clamp scroll so we can't scroll past the content end.
        let content_len = content.len() as u16;
        let visible_rows = area.height.saturating_sub(2); // minus block borders
        let max_scroll = content_len.saturating_sub(visible_rows);
        if app.help_scroll > max_scroll {
            app.help_scroll = max_scroll;
        }
        f.render_widget(
            Paragraph::new(content)
                .block(overlay_block("Help", Tier::Info))
                .scroll((app.help_scroll, 0)),
            area,
        );
    }
}

fn render_spawn_overlay(f: &mut Frame, st: &crate::app::mode::SpawnState) {
    let picker_title = if st.selected.is_empty() {
        "Spawn — preset (Enter to name, space to multi-select)"
    } else {
        "Spawn — preset (Enter to spawn all selected)"
    };
    let title = if st.naming {
        "Spawn \u{2014} name"
    } else {
        picker_title
    };

    let lines: Vec<Line> = if !st.naming {
        st.presets
            .iter()
            .enumerate()
            .map(|(i, (id, p))| {
                let arrow = if i == st.cursor { "\u{25B6} " } else { "  " };
                let mark = if st.selected.contains(&i) {
                    "[x]"
                } else {
                    "[ ]"
                };
                Line::from(format!("{}{} {}  ({})", arrow, mark, p.display_name, id))
            })
            .collect()
    } else {
        vec![
            Line::from(format!("Preset: {}", st.presets[st.cursor].0)),
            Line::from(""),
            Line::from(format!("Name: {}_", st.name_input)),
            Line::from(""),
            Line::from(Span::styled(
                "Enter: confirm  Esc: back",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    };

    let height = (lines.len() as u16).saturating_add(2);
    let area = centered_rect(70, height, f.area());
    let block = overlay_block(title, Tier::Info);
    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_adopt_overlay(f: &mut Frame, st: &crate::app::mode::AdoptState) {
    use crate::app::mode::AdoptPhase;

    let (title, lines): (&str, Vec<Line>) = match &st.phase {
        AdoptPhase::PickPane { cursor } => {
            let home = std::env::var("HOME").unwrap_or_default();
            let rows: Vec<Line> = st
                .panes
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let arrow = if i == *cursor { "\u{25B6} " } else { "  " };
                    let path = if !home.is_empty() && p.path.starts_with(&home) {
                        format!("~{}", &p.path[home.len()..])
                    } else {
                        p.path.clone()
                    };
                    Line::from(format!(
                        "{}[{}] {}  {}",
                        arrow, p.pane_index, p.command, path
                    ))
                })
                .collect();
            ("Adopt — pick a pane", rows)
        }
        AdoptPhase::PickPreset { cursor, .. } => {
            let rows: Vec<Line> = st
                .presets
                .iter()
                .enumerate()
                .map(|(i, (id, p))| {
                    let arrow = if i == *cursor { "\u{25B6} " } else { "  " };
                    Line::from(format!("{}{}  ({})", arrow, p.display_name, id))
                })
                .collect();
            ("Adopt — pick a preset", rows)
        }
        AdoptPhase::Naming { preset_id, .. } => (
            "Adopt — name",
            vec![
                Line::from(format!("Preset: {}", preset_id)),
                Line::from(""),
                Line::from(format!("Name: {}_", st.name_input)),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter: confirm  Esc: back",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        ),
    };

    let height = (lines.len() as u16).saturating_add(2);
    let area = centered_rect(70, height, f.area());
    let block = overlay_block(title, Tier::Info);
    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_center_prompt(f: &mut Frame, title: &str, tier: Tier, body: Vec<Line<'_>>) {
    let height = (body.len() as u16).saturating_add(2); // body + top/bottom border
    let area = centered_rect(60, height, f.area());
    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(Paragraph::new(body).block(overlay_block(title, tier)), area);
}

fn confirm_body(lead: &str) -> Vec<Line<'static>> {
    let yes = Style::default().fg(Color::Green).bold();
    let no = Style::default().fg(Color::Red).bold();
    let dim = Style::default().fg(Color::DarkGray);
    let prompt = Line::from(vec![
        Span::styled("y", yes),
        Span::styled("/", dim),
        Span::styled("Enter", yes),
        Span::raw("  "),
        Span::styled("n", no),
        Span::styled("/", dim),
        Span::styled("Esc", no),
    ]);
    if lead.is_empty() {
        vec![prompt]
    } else {
        vec![Line::from(Span::raw(lead.to_string())), prompt]
    }
}

fn render_forward_overlay(
    f: &mut Frame,
    agents: &[crate::app::AgentRow],
    fw: &mut Box<crate::app::mode::ForwardState>,
) {
    use crate::app::mode::ForwardPhase;

    let full = f.area();
    match &fw.phase {
        ForwardPhase::PickSource { cursor } => {
            let title = "Forward — pick source (Enter)";
            let lines: Vec<Line> = agents
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let arrow = if i == *cursor { "▶ " } else { "  " };
                    Line::from(format!("{}{}  ({})", arrow, a.name, a.preset))
                })
                .collect();
            let height = (lines.len() as u16).saturating_add(2);
            let area = centered_rect(80, height, full);
            f.render_widget(ratatui::widgets::Clear, area);
            f.render_widget(
                Paragraph::new(lines).block(overlay_block(title, Tier::Info)),
                area,
            );
        }
        ForwardPhase::EditPreview => {
            let title = "Forward — edit preview  (Enter continue · Ctrl+G copy-mode · Ctrl+P pull · Shift+Enter newline · Esc cancel)";
            let area = full;
            fw.preview.set_block(overlay_block(title, Tier::Info));
            fw.preview
                .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
            fw.preview.set_cursor_line_style(Style::default());
            f.render_widget(ratatui::widgets::Clear, area);
            f.render_widget(&fw.preview, area);
            let (cursor, top) =
                sync_textarea_hardware_cursor(&fw.preview, area, fw.preview_viewport_top);
            fw.preview_viewport_top = top;
            f.set_cursor_position(cursor);
        }
        ForwardPhase::PickTargets { cursor, selected } => {
            let title = "Forward — pick targets (space to toggle, Enter to send — no selection sends to highlighted)";
            let lines: Vec<Line> = agents
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let arrow = if i == *cursor { "▶ " } else { "  " };
                    let mark = if selected.contains(&a.pane_id) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    Line::from(format!("{}{} {}  ({})", arrow, mark, a.name, a.preset))
                })
                .collect();
            let height = (lines.len() as u16).saturating_add(2);
            let area = centered_rect(80, height, full);
            f.render_widget(ratatui::widgets::Clear, area);
            f.render_widget(
                Paragraph::new(lines).block(overlay_block(title, Tier::Info)),
                area,
            );
        }
    }
}

/// Rect centered with percentage width and absolute-row height.
/// Height is clamped to the available area. Using absolute rows avoids the
/// "popup collapses to its borders" problem that percentages cause when the
/// controller pane is small (e.g. 12 rows after our main-pane-height shrink).
fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let height = height.min(r.height);
    let top_pad = r.height.saturating_sub(height) / 2;
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(r);
    let x_pad = 100u16.saturating_sub(percent_x) / 2;
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(x_pad),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(x_pad),
        ])
        .split(v[1])[1]
}
