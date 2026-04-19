//! Integration tests that spin up a real tmux session.
//! Requires tmux >= 3.0 on PATH.
//! Each test uses a unique session name and kills it on drop.

use qmux::tmux::{
    capture_pane_to_buffer, delete_buffer, has_session, kill_session, list_qmux_panes,
    new_session_detached, paste_buffer_bracketed, send_enter, set_buffer, set_pane_option,
    show_buffer, split_window, RealTmux,
};
use std::{thread, time::Duration};

struct Session(String);
impl Drop for Session {
    fn drop(&mut self) {
        let _ = kill_session(&RealTmux, &self.0);
    }
}

fn unique_name(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}-{}", prefix, n)
}

#[test]
fn session_lifecycle() {
    let tmux = RealTmux;
    let name = unique_name("qmux-test");
    let _sess = Session(name.clone());

    assert!(!has_session(&tmux, &name));
    new_session_detached(&tmux, &name, "main", "cat").unwrap();
    assert!(has_session(&tmux, &name));
}

#[test]
fn set_option_and_list_roundtrip() {
    let tmux = RealTmux;
    let name = unique_name("qmux-test");
    let _sess = Session(name.clone());

    new_session_detached(&tmux, &name, "main", "cat").unwrap();
    // Tag the first pane.
    let target = format!("{}:main.0", name);
    set_pane_option(&tmux, &target, "@qmux_role", "controller").unwrap();

    let rows = list_qmux_panes(&tmux, &name).unwrap();
    let ctrl = rows.iter().find(|r| r.role == "controller");
    assert!(ctrl.is_some(), "no controller row in {:?}", rows);
}

#[test]
fn paste_buffer_delivers_text_to_pane() {
    let tmux = RealTmux;
    let name = unique_name("qmux-test");
    let _sess = Session(name.clone());

    // Create a session whose pane runs `cat` — it will echo pasted input.
    new_session_detached(&tmux, &name, "main", "cat").unwrap();
    let pane = format!("{}:main.0", name);

    set_buffer(&tmux, "qmux-it", "hello world\n").unwrap();
    paste_buffer_bracketed(&tmux, "qmux-it", &pane).unwrap();
    send_enter(&tmux, &pane).unwrap();
    thread::sleep(Duration::from_millis(200));

    capture_pane_to_buffer(&tmux, &pane, "qmux-cap", 50).unwrap();
    let captured = show_buffer(&tmux, "qmux-cap").unwrap();
    assert!(captured.contains("hello world"), "captured: {:?}", captured);

    delete_buffer(&tmux, "qmux-it").unwrap();
    delete_buffer(&tmux, "qmux-cap").unwrap();
}

#[test]
fn split_window_returns_pane_id() {
    let tmux = RealTmux;
    let name = unique_name("qmux-test");
    let _sess = Session(name.clone());

    new_session_detached(&tmux, &name, "main", "cat").unwrap();
    let target = format!("{}:main", name);
    let new_pane = split_window(&tmux, &target, "cat").unwrap();
    assert!(
        new_pane.starts_with('%'),
        "unexpected pane id: {}",
        new_pane
    );
}
