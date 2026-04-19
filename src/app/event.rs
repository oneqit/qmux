use crate::app::mode::Mode;
use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum EventOutcome {
    Continue,
    Quit,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> EventOutcome {
    match &app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Insert => handle_insert(app, key),
        Mode::QuitConfirm { .. } => handle_quit_confirm(app, key),
        Mode::Spawn(_) => handle_spawn(app, key),
        Mode::KillConfirm { .. } => handle_kill_confirm(app, key),
        Mode::Forward(_) => handle_forward(app, key),
        Mode::Help => handle_help(app, key),
        Mode::Adopt(_) => handle_adopt(app, key),
    }
}

fn handle_adopt(app: &mut App, key: KeyEvent) -> EventOutcome {
    use crate::app::mode::AdoptPhase;

    let phase = match &app.mode {
        Mode::Adopt(st) => st.phase.clone(),
        _ => return EventOutcome::Continue,
    };

    match phase {
        AdoptPhase::PickPane { mut cursor } => match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Mode::Adopt(st) = &mut app.mode {
                    if cursor + 1 < st.panes.len() {
                        cursor += 1;
                    }
                    st.phase = AdoptPhase::PickPane { cursor };
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor.saturating_sub(1);
                if let Mode::Adopt(st) = &mut app.mode {
                    st.phase = AdoptPhase::PickPane { cursor };
                }
            }
            KeyCode::Enter => {
                if let Mode::Adopt(st) = &mut app.mode {
                    if let Some(p) = st.panes.get(cursor) {
                        let target_pane_id = p.pane_id.clone();
                        st.phase = AdoptPhase::PickPreset {
                            cursor: 0,
                            target_pane_id,
                        };
                    }
                }
            }
            _ => {}
        },
        AdoptPhase::PickPreset {
            mut cursor,
            target_pane_id,
        } => match key.code {
            KeyCode::Esc => {
                if let Mode::Adopt(st) = &mut app.mode {
                    st.phase = AdoptPhase::PickPane { cursor: 0 };
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Mode::Adopt(st) = &mut app.mode {
                    if cursor + 1 < st.presets.len() {
                        cursor += 1;
                    }
                    st.phase = AdoptPhase::PickPreset {
                        cursor,
                        target_pane_id,
                    };
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor.saturating_sub(1);
                if let Mode::Adopt(st) = &mut app.mode {
                    st.phase = AdoptPhase::PickPreset {
                        cursor,
                        target_pane_id,
                    };
                }
            }
            KeyCode::Enter => {
                use crate::app::actions::suggest_name;
                let existing: Vec<String> = app.agents.iter().map(|a| a.name.clone()).collect();
                if let Mode::Adopt(st) = &mut app.mode {
                    let Some((preset_id, _)) = st.presets.get(cursor).cloned() else {
                        return EventOutcome::Continue;
                    };
                    st.name_input = suggest_name(&existing, &preset_id);
                    st.phase = AdoptPhase::Naming {
                        target_pane_id,
                        preset_id,
                    };
                }
            }
            _ => {}
        },
        AdoptPhase::Naming {
            target_pane_id,
            preset_id,
        } => match key.code {
            KeyCode::Esc => {
                if let Mode::Adopt(st) = &mut app.mode {
                    st.phase = AdoptPhase::PickPreset {
                        cursor: 0,
                        target_pane_id,
                    };
                }
            }
            KeyCode::Char(c) => {
                if let Mode::Adopt(st) = &mut app.mode {
                    st.name_input.push(c);
                }
            }
            KeyCode::Backspace => {
                if let Mode::Adopt(st) = &mut app.mode {
                    st.name_input.pop();
                }
            }
            KeyCode::Enter => {
                use crate::app::actions::adopt_pane;
                let name = match &app.mode {
                    Mode::Adopt(st) => st.name_input.trim().to_string(),
                    _ => return EventOutcome::Continue,
                };
                if name.is_empty() {
                    app.status = Some("name cannot be empty".into());
                    return EventOutcome::Continue;
                }
                match adopt_pane(app, &target_pane_id, &preset_id, &name) {
                    Ok(()) => {
                        app.status = Some(format!("registered {}", name));
                    }
                    Err(e) => {
                        app.status = Some(format!("adopt failed: {}", e));
                    }
                }
                app.mode = Mode::Normal;
            }
            _ => {}
        },
    }
    EventOutcome::Continue
}

fn handle_normal(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Char('i') => {
            app.mode = Mode::Insert;
            EventOutcome::Continue
        }
        KeyCode::Char('a') => {
            enter_adopt_mode(app);
            EventOutcome::Continue
        }
        KeyCode::Char('q') => {
            app.mode = Mode::QuitConfirm { kill_all: false };
            EventOutcome::Continue
        }
        KeyCode::Char('Q') => {
            app.mode = Mode::QuitConfirm { kill_all: true };
            EventOutcome::Continue
        }
        KeyCode::Char('R') => {
            app.refresh();
            EventOutcome::Continue
        }
        KeyCode::Char('L') => {
            app.cycle_layout();
            EventOutcome::Continue
        }
        KeyCode::Char('l') => {
            app.apply_current_layout();
            EventOutcome::Continue
        }
        KeyCode::Char('s') => {
            enter_spawn_mode(app);
            EventOutcome::Continue
        }
        KeyCode::Char('x') => {
            if let Some(idx) = app.cursor {
                if let Some(a) = app.agents.get(idx) {
                    app.mode = Mode::KillConfirm {
                        pane_id: a.pane_id.clone(),
                        name: a.name.clone(),
                    };
                }
            }
            EventOutcome::Continue
        }
        KeyCode::Char('f') => {
            if app.agents.is_empty() {
                app.status = Some("no agents to forward from".into());
            } else {
                let start = app
                    .cursor
                    .unwrap_or(0)
                    .min(app.agents.len().saturating_sub(1));
                app.mode = Mode::Forward(Box::new(crate::app::mode::ForwardState {
                    phase: crate::app::mode::ForwardPhase::PickSource { cursor: start },
                    source_pane_id: None,
                    source_name: None,
                    preview: tui_textarea::TextArea::default(),
                    preview_viewport_top: (0, 0),
                }));
            }
            EventOutcome::Continue
        }
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            EventOutcome::Continue
        }
        KeyCode::Char(' ') => {
            toggle_agent_selection(app);
            EventOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
            app.cursor_down();
            EventOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
            app.cursor_up();
            EventOutcome::Continue
        }
        KeyCode::Char('J') => {
            if let Err(e) = crate::app::actions::swap_agent(app, crate::app::actions::SwapDir::Down)
            {
                app.status = Some(format!("swap error: {}", e));
            }
            EventOutcome::Continue
        }
        KeyCode::Char('K') => {
            if let Err(e) = crate::app::actions::swap_agent(app, crate::app::actions::SwapDir::Up) {
                app.status = Some(format!("swap error: {}", e));
            }
            EventOutcome::Continue
        }
        KeyCode::Enter => {
            send_prompt(app);
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

/// Shared routing for text editing keys inside a `tui_textarea::TextArea`.
/// Normalizes newline input so Shift+Enter works across terminals:
/// - Shift/Alt+Enter → newline
/// - Ctrl+J (how some terminals transmit Shift+Enter) → newline, overriding
///   tui-textarea's default delete-to-line-head
/// - everything else → forwarded to tui-textarea's own bindings
fn route_textarea_edit(ta: &mut tui_textarea::TextArea<'static>, key: KeyEvent) {
    match key.code {
        KeyCode::Enter
            if key
                .modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) =>
        {
            ta.insert_newline();
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ta.insert_newline();
        }
        _ => {
            ta.input(key);
        }
    }
}

fn handle_insert(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            EventOutcome::Continue
        }
        KeyCode::Enter if key.modifiers.is_empty() => {
            send_prompt(app);
            app.mode = Mode::Normal;
            EventOutcome::Continue
        }
        _ => {
            route_textarea_edit(&mut app.input, key);
            EventOutcome::Continue
        }
    }
}

fn send_prompt(app: &mut App) {
    use crate::app::actions::broadcast_send;
    let targets: Vec<String> = app
        .agents
        .iter()
        .filter(|a| a.selected)
        .map(|a| a.pane_id.clone())
        .collect();
    let text = app.input.lines().join("\n");
    if targets.is_empty() {
        app.status = Some("select agents with space first".into());
        return;
    }
    if text.trim().is_empty() {
        app.status = Some("type a prompt first".into());
        return;
    }
    match broadcast_send(app, &text, &targets) {
        Ok(()) => {
            app.input = tui_textarea::TextArea::default();
            app.input_viewport_top = (0, 0);
            app.status = Some(format!("sent to {}", targets.len()));
        }
        Err(e) => {
            app.status = Some(format!("send error: {}", e));
        }
    }
}

fn handle_help(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
            app.mode = Mode::Normal;
            app.help_scroll = 0;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.help_scroll = app.help_scroll.saturating_add(1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            app.help_scroll = app.help_scroll.saturating_add(10);
        }
        KeyCode::PageUp => {
            app.help_scroll = app.help_scroll.saturating_sub(10);
        }
        KeyCode::Char('g') => {
            app.help_scroll = 0;
        }
        KeyCode::Char('G') => {
            app.help_scroll = u16::MAX;
        }
        _ => {}
    }
    EventOutcome::Continue
}

fn toggle_agent_selection(app: &mut App) {
    if let Some(idx) = app.cursor {
        if let Some(a) = app.agents.get_mut(idx) {
            a.selected = !a.selected;
        }
    }
}

fn enter_adopt_mode(app: &mut App) {
    use crate::agent::WhichFinder;
    enter_adopt_mode_with(app, &WhichFinder);
}

fn enter_adopt_mode_with(app: &mut App, finder: &dyn crate::agent::BinaryFinder) {
    use crate::agent::available_presets;
    use crate::app::mode::{AdoptPhase, AdoptState};
    use crate::config::{default_config_path, load_user_config, merged_presets};
    use crate::tmux::list_adoptable_panes;

    let panes = match list_adoptable_panes(app.tmux.as_ref(), &app.window_id) {
        Ok(p) => p,
        Err(e) => {
            app.status = Some(format!("list-panes error: {}", e));
            return;
        }
    };
    if panes.is_empty() {
        app.status = Some("no panes to register".into());
        return;
    }
    let user_cfg = default_config_path()
        .and_then(|p| load_user_config(&p).ok())
        .unwrap_or_default();
    let all = merged_presets(&user_cfg);
    let presets = available_presets(finder, &all);
    if presets.is_empty() {
        app.status = Some("no installed agents found on PATH".into());
        return;
    }
    app.mode = Mode::Adopt(AdoptState {
        panes,
        presets,
        phase: AdoptPhase::PickPane { cursor: 0 },
        name_input: String::new(),
    });
}

fn enter_spawn_mode(app: &mut App) {
    use crate::agent::{available_presets, WhichFinder};
    use crate::app::mode::SpawnState;
    use crate::config::{default_config_path, load_user_config, merged_presets};

    let user_cfg = default_config_path()
        .and_then(|p| load_user_config(&p).ok())
        .unwrap_or_default();
    let all = merged_presets(&user_cfg);
    let avail = available_presets(&WhichFinder, &all);
    if avail.is_empty() {
        app.status = Some("no installed agents found on PATH".into());
        return;
    }
    app.mode = Mode::Spawn(SpawnState {
        presets: avail,
        cursor: 0,
        selected: Vec::new(),
        name_input: String::new(),
        naming: false,
    });
}

fn handle_quit_confirm(app: &mut App, key: KeyEvent) -> EventOutcome {
    let kill_all = match &app.mode {
        Mode::QuitConfirm { kill_all } => *kill_all,
        _ => return EventOutcome::Continue,
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            if kill_all {
                let pane_ids: Vec<String> = app.agents.iter().map(|a| a.pane_id.clone()).collect();
                for pid in pane_ids {
                    let _ = crate::tmux::kill_pane(app.tmux.as_ref(), &pid);
                }
            }
            EventOutcome::Quit
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.mode = Mode::Normal;
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

fn handle_kill_confirm(app: &mut App, key: KeyEvent) -> EventOutcome {
    use crate::app::actions::kill_agent;
    // Extract fields by cloning before we mutably borrow `app`.
    let (pid, nm) = match &app.mode {
        Mode::KillConfirm { pane_id, name } => (pane_id.clone(), name.clone()),
        _ => return EventOutcome::Continue,
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            match kill_agent(app, &pid) {
                Ok(()) => app.status = Some(format!("killed {}", nm)),
                Err(e) => app.status = Some(format!("kill error: {}", e)),
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Esc => app.mode = Mode::Normal,
        _ => {}
    }
    EventOutcome::Continue
}

fn handle_spawn(app: &mut App, key: KeyEvent) -> EventOutcome {
    use crate::app::actions::{reorder_to_end, spawn_agent, suggest_name};

    let Mode::Spawn(st) = &mut app.mode else {
        return EventOutcome::Continue;
    };

    if !st.naming {
        match key.code {
            KeyCode::Esc => app.mode = Mode::Normal,
            KeyCode::Down | KeyCode::Char('j') => {
                if st.cursor + 1 < st.presets.len() {
                    st.cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                st.cursor = st.cursor.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                if let Some(pos) = st.selected.iter().position(|i| *i == st.cursor) {
                    st.selected.remove(pos);
                } else {
                    st.selected.push(st.cursor);
                }
            }
            KeyCode::Enter => {
                if st.selected.is_empty() {
                    let existing: Vec<String> = app.agents.iter().map(|a| a.name.clone()).collect();
                    let (preset_id, _) = &st.presets[st.cursor];
                    st.name_input = suggest_name(&existing, preset_id);
                    st.naming = true;
                } else {
                    let to_spawn: Vec<(String, crate::agent::Preset)> =
                        st.selected.iter().map(|&i| st.presets[i].clone()).collect();
                    let mut spawned = 0usize;
                    let mut first_err: Option<String> = None;
                    let mut new_ids: Vec<String> = Vec::new();
                    for (preset_id, preset) in to_spawn {
                        let existing: Vec<String> =
                            app.agents.iter().map(|a| a.name.clone()).collect();
                        let name = suggest_name(&existing, &preset_id);
                        match spawn_agent(app, &preset_id, &preset, &name) {
                            Ok(pane_id) => {
                                new_ids.push(pane_id);
                                spawned += 1;
                            }
                            Err(e) => {
                                first_err = Some(format!("{}: {}", preset_id, e));
                                break;
                            }
                        }
                    }
                    if !new_ids.is_empty() {
                        if let Err(e) = reorder_to_end(app, &new_ids) {
                            first_err = first_err.or_else(|| Some(format!("reorder: {}", e)));
                        }
                    }
                    app.mode = Mode::Normal;
                    app.status = match first_err {
                        Some(e) => Some(format!("spawn error ({} ok): {}", spawned, e)),
                        None => Some(format!("spawned {}", spawned)),
                    };
                }
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                st.naming = false;
            }
            KeyCode::Char(c) => {
                st.name_input.push(c);
            }
            KeyCode::Backspace => {
                st.name_input.pop();
            }
            KeyCode::Enter => {
                let name = st.name_input.trim().to_string();
                if name.is_empty() {
                    app.status = Some("name cannot be empty".into());
                    return EventOutcome::Continue;
                }
                if app.agents.iter().any(|a| a.name == name) {
                    app.status = Some(format!("name '{}' already used", name));
                    return EventOutcome::Continue;
                }
                let (preset_id, preset) = st.presets[st.cursor].clone();
                match spawn_agent(app, &preset_id, &preset, &name) {
                    Ok(pane_id) => {
                        if let Err(e) = reorder_to_end(app, std::slice::from_ref(&pane_id)) {
                            app.status = Some(format!("reorder: {}", e));
                        }
                        app.mode = Mode::Normal;
                    }
                    Err(e) => {
                        app.status = Some(format!("spawn failed: {}", e));
                        app.mode = Mode::Normal;
                    }
                }
            }
            _ => {}
        }
    }
    EventOutcome::Continue
}

fn handle_forward(app: &mut App, key: KeyEvent) -> EventOutcome {
    use crate::app::actions::{broadcast_send, capture_for_forward};
    use crate::app::mode::ForwardPhase;

    if key.code == KeyCode::Esc {
        app.mode = Mode::Normal;
        return EventOutcome::Continue;
    }

    // Snapshot the current phase (clone is cheap — ForwardPhase is small).
    let phase = match &app.mode {
        Mode::Forward(fw) => fw.phase.clone(),
        _ => return EventOutcome::Continue,
    };

    match phase {
        ForwardPhase::PickSource { mut cursor } => match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if cursor + 1 < app.agents.len() {
                    cursor += 1;
                }
                set_forward_phase(app, ForwardPhase::PickSource { cursor });
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor.saturating_sub(1);
                set_forward_phase(app, ForwardPhase::PickSource { cursor });
            }
            KeyCode::Enter => {
                if cursor >= app.agents.len() {
                    return EventOutcome::Continue;
                }
                let src = app.agents[cursor].clone();
                let markers = crate::app::actions::resolve_markers(&src.preset);
                let markers_ref = markers
                    .as_ref()
                    .map(|(s, e, b)| (s.as_str(), e.as_str(), b.as_slice()));
                match capture_for_forward(app, &src.pane_id, markers_ref) {
                    Ok(text) => {
                        if let Mode::Forward(fw) = &mut app.mode {
                            fw.source_pane_id = Some(src.pane_id);
                            fw.source_name = Some(src.name);
                            fw.preview = text_to_textarea(&text);
                            fw.preview_viewport_top = (0, 0);
                            fw.phase = ForwardPhase::EditPreview;
                        }
                    }
                    Err(e) => {
                        app.status = Some(e.to_string());
                        app.mode = Mode::Normal;
                    }
                }
            }
            _ => {}
        },
        ForwardPhase::EditPreview => match key.code {
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let src = match &app.mode {
                    Mode::Forward(fw) => fw.source_pane_id.clone(),
                    _ => None,
                };
                if let Some(src) = src {
                    if let Err(e) = crate::app::actions::delegate_to_copy_mode(app, &src) {
                        app.status = Some(format!("copy-mode error: {}", e));
                    } else {
                        app.status = Some(
                            "copy-mode: select with v/y, return to controller and press Ctrl+P"
                                .into(),
                        );
                    }
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match crate::app::actions::pull_latest_buffer(app) {
                    Ok(s) if !s.is_empty() => {
                        if let Mode::Forward(fw) = &mut app.mode {
                            fw.preview = text_to_textarea(&s);
                            fw.preview_viewport_top = (0, 0);
                        }
                    }
                    Ok(_) => app.status = Some("buffer empty".into()),
                    Err(e) => app.status = Some(format!("pull error: {}", e)),
                }
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                if let Mode::Forward(fw) = &mut app.mode {
                    fw.phase = ForwardPhase::PickTargets {
                        cursor: 0,
                        selected: Vec::new(),
                    };
                }
            }
            _ => {
                if let Mode::Forward(fw) = &mut app.mode {
                    route_textarea_edit(&mut fw.preview, key);
                }
            }
        },
        ForwardPhase::PickTargets {
            mut cursor,
            mut selected,
        } => match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if cursor + 1 < app.agents.len() {
                    cursor += 1;
                }
                set_forward_phase(app, ForwardPhase::PickTargets { cursor, selected });
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor.saturating_sub(1);
                set_forward_phase(app, ForwardPhase::PickTargets { cursor, selected });
            }
            KeyCode::Char(' ') => {
                if cursor >= app.agents.len() {
                    return EventOutcome::Continue;
                }
                let pid = app.agents[cursor].pane_id.clone();
                if let Some(pos) = selected.iter().position(|x| x == &pid) {
                    selected.remove(pos);
                } else {
                    selected.push(pid);
                }
                set_forward_phase(app, ForwardPhase::PickTargets { cursor, selected });
            }
            KeyCode::Enter => {
                let preview = match &app.mode {
                    Mode::Forward(fw) => fw.preview.lines().join("\n"),
                    _ => return EventOutcome::Continue,
                };
                let mut targets = selected.clone();
                if targets.is_empty() {
                    if let Some(a) = app.agents.get(cursor) {
                        targets.push(a.pane_id.clone());
                    }
                }
                match broadcast_send(app, &preview, &targets) {
                    Ok(()) => app.status = Some(format!("forwarded to {}", targets.len())),
                    Err(e) => app.status = Some(format!("forward error: {}", e)),
                }
                app.mode = Mode::Normal;
            }
            _ => {}
        },
    }
    EventOutcome::Continue
}

fn set_forward_phase(app: &mut App, phase: crate::app::mode::ForwardPhase) {
    if let Mode::Forward(fw) = &mut app.mode {
        fw.phase = phase;
    }
}

fn text_to_textarea(text: &str) -> tui_textarea::TextArea<'static> {
    let lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.lines().map(String::from).collect()
    };
    tui_textarea::TextArea::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn q_enters_quit_confirm() {
        let mut app = App::empty();
        matches!(handle_key(&mut app, key('q')), EventOutcome::Continue);
        assert!(matches!(app.mode, Mode::QuitConfirm { kill_all: false }));
    }

    #[test]
    fn y_in_quit_confirm_returns_quit() {
        let mut app = App::empty();
        app.mode = Mode::QuitConfirm { kill_all: false };
        assert!(matches!(handle_key(&mut app, key('y')), EventOutcome::Quit));
    }

    #[test]
    fn esc_in_quit_confirm_returns_to_normal() {
        let mut app = App::empty();
        app.mode = Mode::QuitConfirm { kill_all: false };
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        matches!(handle_key(&mut app, esc), EventOutcome::Continue);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn lowercase_l_reapplies_layout_without_cycling() {
        let mut app = App::empty();
        let before = app.layout_index;
        matches!(handle_key(&mut app, key('l')), EventOutcome::Continue);
        assert_eq!(app.layout_index, before, "l must not advance layout_index");
        assert!(app.status.is_some(), "apply_current_layout sets status");
    }

    #[test]
    fn shift_q_enters_quit_confirm_with_kill_all_true() {
        let mut app = App::empty();
        matches!(handle_key(&mut app, key('Q')), EventOutcome::Continue);
        assert!(matches!(app.mode, Mode::QuitConfirm { kill_all: true }));
    }

    #[test]
    fn y_in_kill_all_quit_confirm_returns_quit() {
        let mut app = App::empty();
        app.mode = Mode::QuitConfirm { kill_all: true };
        assert!(matches!(handle_key(&mut app, key('y')), EventOutcome::Quit));
    }

    fn key_with_mods(c: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(c, mods)
    }

    #[test]
    fn shift_enter_inserts_newline_and_stays_in_insert() {
        let mut app = App::empty();
        app.mode = Mode::Insert;
        let ev = key_with_mods(KeyCode::Enter, KeyModifiers::SHIFT);
        matches!(handle_key(&mut app, ev), EventOutcome::Continue);
        assert_eq!(
            app.input.lines().len(),
            2,
            "insert_newline produces 2 empty lines"
        );
        assert!(matches!(app.mode, Mode::Insert));
    }

    #[test]
    fn alt_enter_inserts_newline() {
        let mut app = App::empty();
        app.mode = Mode::Insert;
        let ev = key_with_mods(KeyCode::Enter, KeyModifiers::ALT);
        matches!(handle_key(&mut app, ev), EventOutcome::Continue);
        assert_eq!(
            app.input.lines().len(),
            2,
            "insert_newline produces 2 empty lines"
        );
        assert!(matches!(app.mode, Mode::Insert));
    }

    #[test]
    fn plain_enter_in_insert_still_returns_to_normal() {
        let mut app = App::empty();
        app.mode = Mode::Insert;
        let ev = key_with_mods(KeyCode::Enter, KeyModifiers::NONE);
        matches!(handle_key(&mut app, ev), EventOutcome::Continue);
        assert!(matches!(app.mode, Mode::Normal));
    }

    // ---------- Adopt mode ----------

    /// Make an App whose list-panes call yields exactly one adoptable pane (%3).
    fn app_with_one_adoptable() -> App {
        use crate::tmux::mock::MockTmux;
        // Format: pane_id|@qmux_role|pane_index|command|path
        let raw = "%3||2|zsh|/home/u/code/frontend\n";
        let m = MockTmux::new().with_ok(raw);
        App::new(Box::new(m), "@1".into())
    }

    /// Finder that reports every binary as installed — keeps tests
    /// independent of the host `$PATH`.
    struct AllInstalled;
    impl crate::agent::BinaryFinder for AllInstalled {
        fn exists(&self, _binary: &str) -> bool {
            true
        }
    }

    #[test]
    fn a_enters_adopt_mode_when_adoptable_panes_exist() {
        let mut app = app_with_one_adoptable();
        enter_adopt_mode_with(&mut app, &AllInstalled);
        match &app.mode {
            Mode::Adopt(st) => {
                assert_eq!(st.panes.len(), 1);
                assert_eq!(st.panes[0].pane_id, "%3");
                assert!(matches!(
                    st.phase,
                    crate::app::mode::AdoptPhase::PickPane { cursor: 0 }
                ));
                assert!(!st.presets.is_empty(), "presets should be populated");
            }
            other => panic!("expected Mode::Adopt, got {:?}", other),
        }
    }

    #[test]
    fn a_shows_status_and_stays_normal_when_no_adoptable_panes() {
        use crate::tmux::mock::MockTmux;
        // Only a controller — no adoptable panes.
        let raw = "%0|controller|0|qmux|/home/u\n";
        let m = MockTmux::new().with_ok(raw);
        let mut app = App::new(Box::new(m), "@1".into());
        handle_key(&mut app, key('a'));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(
            app.status.as_deref().unwrap_or("").contains("no panes"),
            "expected 'no panes...' status, got {:?}",
            app.status
        );
    }

    /// Construct an Adopt state directly for phase-transition tests.
    fn adopt_app(phase: crate::app::mode::AdoptPhase) -> App {
        use crate::agent::Preset;
        use crate::app::mode::AdoptState;
        use crate::tmux::{mock::MockTmux, AdoptablePane};

        let panes = vec![
            AdoptablePane {
                pane_id: "%3".into(),
                pane_index: 2,
                command: "zsh".into(),
                path: "/home/u/a".into(),
            },
            AdoptablePane {
                pane_id: "%4".into(),
                pane_index: 3,
                command: "node".into(),
                path: "/home/u/b".into(),
            },
        ];
        let presets = vec![
            (
                "claude".into(),
                Preset {
                    display_name: "Claude".into(),
                    binary: "claude".into(),
                    launch_cmd: "claude".into(),
                    response_start_marker: None,
                    response_end_marker: None,
                    post_cutoff_markers: Vec::new(),
                },
            ),
            (
                "codex".into(),
                Preset {
                    display_name: "Codex".into(),
                    binary: "codex".into(),
                    launch_cmd: "codex".into(),
                    response_start_marker: None,
                    response_end_marker: None,
                    post_cutoff_markers: Vec::new(),
                },
            ),
        ];
        let m = MockTmux::new();
        let mut app = App::new(Box::new(m), "@1".into());
        app.mode = Mode::Adopt(AdoptState {
            panes,
            presets,
            phase,
            name_input: String::new(),
        });
        app
    }

    fn adopt_phase(app: &App) -> crate::app::mode::AdoptPhase {
        match &app.mode {
            Mode::Adopt(st) => st.phase.clone(),
            _ => panic!("expected Mode::Adopt, got {:?}", app.mode),
        }
    }

    #[test]
    fn adopt_pick_pane_down_moves_cursor() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPane { cursor: 0 });
        handle_key(&mut app, key('j'));
        assert!(matches!(
            adopt_phase(&app),
            AdoptPhase::PickPane { cursor: 1 }
        ));
    }

    #[test]
    fn adopt_pick_pane_down_clamps_at_last() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPane { cursor: 1 }); // last
        handle_key(&mut app, key('j'));
        assert!(matches!(
            adopt_phase(&app),
            AdoptPhase::PickPane { cursor: 1 }
        ));
    }

    #[test]
    fn adopt_pick_pane_up_moves_cursor() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPane { cursor: 1 });
        handle_key(&mut app, key('k'));
        assert!(matches!(
            adopt_phase(&app),
            AdoptPhase::PickPane { cursor: 0 }
        ));
    }

    #[test]
    fn adopt_pick_pane_esc_returns_to_normal() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPane { cursor: 0 });
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, esc);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn adopt_pick_pane_enter_transitions_to_pick_preset_with_pane_id() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPane { cursor: 1 });
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, enter);
        match adopt_phase(&app) {
            AdoptPhase::PickPreset {
                cursor,
                target_pane_id,
            } => {
                assert_eq!(cursor, 0, "preset cursor starts at 0");
                assert_eq!(target_pane_id, "%4", "captures cursor's pane id");
            }
            other => panic!("expected PickPreset, got {:?}", other),
        }
    }

    #[test]
    fn adopt_pick_preset_down_moves_cursor() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPreset {
            cursor: 0,
            target_pane_id: "%3".into(),
        });
        handle_key(&mut app, key('j'));
        match adopt_phase(&app) {
            AdoptPhase::PickPreset { cursor, .. } => assert_eq!(cursor, 1),
            other => panic!("expected PickPreset, got {:?}", other),
        }
    }

    #[test]
    fn adopt_pick_preset_esc_returns_to_pick_pane() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPreset {
            cursor: 1,
            target_pane_id: "%4".into(),
        });
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, esc);
        assert!(matches!(
            adopt_phase(&app),
            AdoptPhase::PickPane { cursor: 0 }
        ));
    }

    #[test]
    fn adopt_pick_preset_enter_transitions_to_naming_with_prefilled_name() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::PickPreset {
            cursor: 0, // claude
            target_pane_id: "%3".into(),
        });
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, enter);
        match &app.mode {
            Mode::Adopt(st) => {
                assert!(matches!(
                    st.phase,
                    AdoptPhase::Naming {
                        ref target_pane_id,
                        ref preset_id,
                    } if target_pane_id == "%3" && preset_id == "claude"
                ));
                assert_eq!(st.name_input, "claude-1", "name prefilled via suggest_name");
            }
            other => panic!("expected Adopt, got {:?}", other),
        }
    }

    #[test]
    fn adopt_naming_char_input_edits_name() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::Naming {
            target_pane_id: "%3".into(),
            preset_id: "claude".into(),
        });
        if let Mode::Adopt(st) = &mut app.mode {
            st.name_input = "claud".into();
        }
        handle_key(&mut app, key('e'));
        match &app.mode {
            Mode::Adopt(st) => assert_eq!(st.name_input, "claude"),
            _ => panic!("expected Adopt"),
        }
    }

    #[test]
    fn adopt_naming_backspace_removes_last_char() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::Naming {
            target_pane_id: "%3".into(),
            preset_id: "claude".into(),
        });
        if let Mode::Adopt(st) = &mut app.mode {
            st.name_input = "claude".into();
        }
        let bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        handle_key(&mut app, bs);
        match &app.mode {
            Mode::Adopt(st) => assert_eq!(st.name_input, "claud"),
            _ => panic!("expected Adopt"),
        }
    }

    #[test]
    fn adopt_naming_esc_returns_to_pick_preset() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::Naming {
            target_pane_id: "%3".into(),
            preset_id: "claude".into(),
        });
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, esc);
        assert!(matches!(
            adopt_phase(&app),
            AdoptPhase::PickPreset { cursor: 0, .. }
        ));
    }

    #[test]
    fn adopt_naming_enter_calls_adopt_pane_and_returns_to_normal() {
        use crate::app::mode::AdoptPhase;
        use crate::tmux::mock::MockTmux;
        // Mock: three set-option calls then fetch_agents (refresh).
        let m = MockTmux::new()
            .with_ok("") // set-option @qmux_role
            .with_ok("") // set-option @qmux_name
            .with_ok("") // set-option @qmux_preset
            .with_ok(""); // refresh
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        app.mode = Mode::Adopt(crate::app::mode::AdoptState {
            panes: Vec::new(),
            presets: Vec::new(),
            phase: AdoptPhase::Naming {
                target_pane_id: "%7".into(),
                preset_id: "claude".into(),
            },
            name_input: "my-claude".into(),
        });
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, enter);
        assert!(matches!(app.mode, Mode::Normal), "should return to Normal");
        let calls = handle.borrow();
        let sets: Vec<&Vec<String>> = calls
            .iter()
            .filter(|c| c.first().map(String::as_str) == Some("set-option"))
            .collect();
        assert_eq!(sets.len(), 3, "three set-option calls");
        assert_eq!(
            sets[0],
            &vec!["set-option", "-p", "-t", "%7", "@qmux_role", "agent"]
        );
        assert_eq!(
            sets[1],
            &vec!["set-option", "-p", "-t", "%7", "@qmux_name", "my-claude"]
        );
        assert_eq!(
            sets[2],
            &vec!["set-option", "-p", "-t", "%7", "@qmux_preset", "claude"]
        );
    }

    #[test]
    fn adopt_naming_enter_with_empty_name_shows_status() {
        use crate::app::mode::AdoptPhase;
        let mut app = adopt_app(AdoptPhase::Naming {
            target_pane_id: "%3".into(),
            preset_id: "claude".into(),
        });
        // name_input defaults to "" from adopt_app
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, enter);
        assert!(matches!(adopt_phase(&app), AdoptPhase::Naming { .. }));
        assert!(
            app.status.as_deref().unwrap_or("").contains("empty"),
            "expected empty-name status"
        );
    }

    #[test]
    fn a_no_longer_enters_insert_mode() {
        // Regression: `a` was previously a vim-style "append" alias for Insert.
        // Now it should attempt Adopt instead; with no adoptable panes the app
        // should stay in Normal (never Insert).
        use crate::tmux::mock::MockTmux;
        let m = MockTmux::new().with_ok("");
        let mut app = App::new(Box::new(m), "@1".into());
        handle_key(&mut app, key('a'));
        assert!(!matches!(app.mode, Mode::Insert), "a must not enter Insert");
    }
}
