use anyhow::Result;
use std::process::{Command, Output};

pub trait TmuxRunner {
    fn run(&self, args: &[&str]) -> Result<String>;
}

pub struct RealTmux;

impl TmuxRunner for RealTmux {
    fn run(&self, args: &[&str]) -> Result<String> {
        let output: Output = Command::new("tmux").args(args).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux {}: {}", args.join(" "), stderr.trim());
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

pub fn has_session(tmux: &dyn TmuxRunner, name: &str) -> bool {
    tmux.run(&["has-session", "-t", name]).is_ok()
}

pub fn new_session_detached(
    tmux: &dyn TmuxRunner,
    session: &str,
    window: &str,
    cmd: &str,
) -> Result<()> {
    tmux.run(&["new-session", "-ds", session, "-n", window, cmd])?;
    Ok(())
}

pub fn kill_session(tmux: &dyn TmuxRunner, name: &str) -> Result<()> {
    tmux.run(&["kill-session", "-t", name])?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRow {
    pub pane_id: String,
    pub role: String,
    pub name: String,
    pub preset: String,
    /// Top coordinate inside the window, in cells (0-based).
    pub pane_top: u32,
    /// Left coordinate inside the window, in cells (0-based).
    pub pane_left: u32,
}

pub fn split_window(tmux: &dyn TmuxRunner, target: &str, cmd: &str) -> Result<String> {
    // -d keeps focus on the current pane (the controller) rather than jumping
    // to the newly created pane.
    let out = tmux.run(&[
        "split-window",
        "-t",
        target,
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        cmd,
    ])?;
    Ok(out.trim().to_string())
}

pub fn set_pane_option(tmux: &dyn TmuxRunner, pane: &str, key: &str, value: &str) -> Result<()> {
    tmux.run(&["set-option", "-p", "-t", pane, key, value])?;
    Ok(())
}

pub fn set_window_option(
    tmux: &dyn TmuxRunner,
    target: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    tmux.run(&["set-option", "-w", "-t", target, key, value])?;
    Ok(())
}

pub fn list_qmux_panes(tmux: &dyn TmuxRunner, session: &str) -> Result<Vec<PaneRow>> {
    let fmt = "#{pane_id}|#{@qmux_role}|#{@qmux_name}|#{@qmux_preset}|#{pane_top}|#{pane_left}";
    let out = tmux.run(&["list-panes", "-s", "-t", session, "-F", fmt])?;
    Ok(parse_pane_rows(&out))
}

pub fn select_layout(tmux: &dyn TmuxRunner, target: &str, layout: &str) -> Result<()> {
    tmux.run(&["select-layout", "-t", target, layout])?;
    Ok(())
}

pub fn kill_pane(tmux: &dyn TmuxRunner, pane: &str) -> Result<()> {
    tmux.run(&["kill-pane", "-t", pane])?;
    Ok(())
}

pub fn set_buffer(tmux: &dyn TmuxRunner, name: &str, content: &str) -> Result<()> {
    tmux.run(&["set-buffer", "-b", name, content])?;
    Ok(())
}

pub fn paste_buffer_bracketed(tmux: &dyn TmuxRunner, buffer: &str, target: &str) -> Result<()> {
    tmux.run(&["paste-buffer", "-b", buffer, "-p", "-t", target])?;
    Ok(())
}

pub fn send_enter(tmux: &dyn TmuxRunner, target: &str) -> Result<()> {
    tmux.run(&["send-keys", "-t", target, "Enter"])?;
    Ok(())
}

pub fn capture_pane_to_buffer(
    tmux: &dyn TmuxRunner,
    pane: &str,
    buffer: &str,
    lines: u32,
) -> Result<()> {
    let start = format!("-{}", lines);
    tmux.run(&["capture-pane", "-t", pane, "-b", buffer, "-S", &start, "-J"])?;
    Ok(())
}

pub fn show_buffer(tmux: &dyn TmuxRunner, name: &str) -> Result<String> {
    tmux.run(&["show-buffer", "-b", name])
}

pub fn delete_buffer(tmux: &dyn TmuxRunner, name: &str) -> Result<()> {
    tmux.run(&["delete-buffer", "-b", name])?;
    Ok(())
}

pub fn current_pane_id() -> Result<String> {
    std::env::var("TMUX_PANE").map_err(|_| anyhow::anyhow!("$TMUX_PANE not set"))
}

/// Returns tmux session names sorted as tmux returns them. Empty vec on error
/// (including when no tmux server is running).
pub fn list_sessions(tmux: &dyn TmuxRunner) -> Vec<String> {
    tmux.run(&["list-sessions", "-F", "#{session_name}"])
        .map(|out| {
            out.lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the value of `@qmux_role` on the current pane (`#{pane_id}` of $TMUX_PANE).
/// Returns empty string if unset. Requires $TMUX_PANE to be set.
pub fn current_pane_role(tmux: &dyn TmuxRunner) -> Result<String> {
    let pane = current_pane_id()?;
    // `display-message -p -t <pane> '#{@qmux_role}'` returns blank if the option is unset.
    let out = tmux.run(&["display-message", "-p", "-t", &pane, "#{@qmux_role}"])?;
    Ok(out.trim().to_string())
}

pub fn current_window_id(tmux: &dyn TmuxRunner) -> Result<String> {
    let pane = current_pane_id()?;
    let out = tmux.run(&["display-message", "-p", "-t", &pane, "#{window_id}"])?;
    Ok(out.trim().to_string())
}

/// Lists panes in a specific window with qmux user-option fields.
pub fn list_panes_in_window(tmux: &dyn TmuxRunner, window: &str) -> Result<Vec<PaneRow>> {
    let fmt = "#{pane_id}|#{@qmux_role}|#{@qmux_name}|#{@qmux_preset}|#{pane_top}|#{pane_left}";
    let out = tmux.run(&["list-panes", "-t", window, "-F", fmt])?;
    Ok(parse_pane_rows(&out))
}

fn parse_pane_rows(out: &str) -> Vec<PaneRow> {
    let mut rows = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() != 6 {
            continue;
        }
        rows.push(PaneRow {
            pane_id: parts[0].to_string(),
            role: parts[1].to_string(),
            name: parts[2].to_string(),
            preset: parts[3].to_string(),
            pane_top: parts[4].parse().unwrap_or(0),
            pane_left: parts[5].parse().unwrap_or(0),
        });
    }
    rows
}

pub fn swap_pane(tmux: &dyn TmuxRunner, src: &str, dst: &str) -> Result<()> {
    // -d: do not change the active pane (keep focus on the controller).
    tmux.run(&["swap-pane", "-d", "-s", src, "-t", dst])?;
    Ok(())
}

/// A pane eligible to be adopted as a qmux agent — no `@qmux_role` set.
/// Carries metadata needed for the user to pick the right one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptablePane {
    pub pane_id: String,
    pub pane_index: u32,
    pub command: String,
    pub path: String,
}

/// List panes in `window` that have no `@qmux_role` — i.e. not the
/// controller and not already-registered agents.
pub fn list_adoptable_panes(tmux: &dyn TmuxRunner, window: &str) -> Result<Vec<AdoptablePane>> {
    let fmt = "#{pane_id}|#{@qmux_role}|#{pane_index}|#{pane_current_command}|#{pane_current_path}";
    let out = tmux.run(&["list-panes", "-t", window, "-F", fmt])?;
    let mut panes = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() != 5 {
            continue;
        }
        if !parts[1].is_empty() {
            continue;
        }
        panes.push(AdoptablePane {
            pane_id: parts[0].to_string(),
            pane_index: parts[2].parse().unwrap_or(0),
            command: parts[3].to_string(),
            path: parts[4].to_string(),
        });
    }
    Ok(panes)
}

/// Swap the `@qmux_role=controller` pane into pane-index 0 of the window so
/// that tmux's `main-*` layouts treat the controller as the main pane. No-op
/// if the controller is already first or if the window has no controller.
pub fn ensure_controller_first(tmux: &dyn TmuxRunner, window: &str) -> Result<()> {
    let panes = list_panes_in_window(tmux, window)?;
    let Some(controller) = panes.iter().find(|p| p.role == "controller") else {
        return Ok(());
    };
    if panes.first().map(|p| p.pane_id.as_str()) == Some(controller.pane_id.as_str()) {
        return Ok(());
    }
    let target = format!("{}.0", window);
    swap_pane(tmux, &controller.pane_id, &target)
}

pub fn select_pane(tmux: &dyn TmuxRunner, pane: &str) -> Result<()> {
    tmux.run(&["select-pane", "-t", pane])?;
    Ok(())
}

pub fn enter_copy_mode(tmux: &dyn TmuxRunner, pane: &str) -> Result<()> {
    tmux.run(&["copy-mode", "-t", pane])?;
    Ok(())
}

pub fn show_latest_buffer(tmux: &dyn TmuxRunner) -> Result<String> {
    // show-buffer with no -b flag returns the most-recent buffer (the top of the stack,
    // which is where `y` in copy-mode saves to).
    tmux.run(&["show-buffer"])
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    pub struct MockTmux {
        pub calls: std::rc::Rc<RefCell<Vec<Vec<String>>>>,
        pub responses: RefCell<VecDeque<Result<String>>>,
    }

    impl Default for MockTmux {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockTmux {
        pub fn new() -> Self {
            Self {
                calls: std::rc::Rc::new(RefCell::new(Vec::new())),
                responses: RefCell::new(VecDeque::new()),
            }
        }

        /// Shared handle to the calls log. Lets a test keep a reference to the
        /// call history after moving the mock into an owning struct (e.g. App).
        pub fn calls_handle(&self) -> std::rc::Rc<RefCell<Vec<Vec<String>>>> {
            self.calls.clone()
        }

        pub fn with_response(self, r: Result<String>) -> Self {
            self.responses.borrow_mut().push_back(r);
            self
        }

        pub fn with_ok(self, s: &str) -> Self {
            self.with_response(Ok(s.to_string()))
        }

        pub fn with_err(self, msg: &str) -> Self {
            self.with_response(Err(anyhow::anyhow!(msg.to_string())))
        }

        pub fn last_call(&self) -> Vec<String> {
            self.calls.borrow().last().cloned().unwrap_or_default()
        }

        pub fn call_count(&self) -> usize {
            self.calls.borrow().len()
        }
    }

    impl TmuxRunner for MockTmux {
        fn run(&self, args: &[&str]) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|s| s.to_string()).collect());
            match self.responses.borrow_mut().pop_front() {
                Some(r) => r,
                None => Ok(String::new()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockTmux;
    use super::*;

    #[test]
    fn mock_records_calls_in_order() {
        let m = MockTmux::new().with_ok("out1").with_ok("out2");
        let out1 = m.run(&["foo", "bar"]).unwrap();
        let out2 = m.run(&["baz"]).unwrap();
        assert_eq!(out1, "out1");
        assert_eq!(out2, "out2");
        assert_eq!(m.call_count(), 2);
        assert_eq!(m.calls.borrow()[0], vec!["foo", "bar"]);
        assert_eq!(m.calls.borrow()[1], vec!["baz"]);
    }

    #[test]
    fn mock_returns_empty_when_no_response_queued() {
        let m = MockTmux::new();
        let out = m.run(&["anything"]).unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn has_session_true_when_tmux_returns_ok() {
        let m = MockTmux::new().with_ok("");
        assert!(has_session(&m, "qmux"));
        assert_eq!(m.last_call(), vec!["has-session", "-t", "qmux"]);
    }

    #[test]
    fn has_session_false_when_tmux_errors() {
        let m = MockTmux::new().with_err("no session");
        assert!(!has_session(&m, "qmux"));
    }

    #[test]
    fn new_session_detached_calls_expected_args() {
        let m = MockTmux::new().with_ok("");
        new_session_detached(&m, "qmux", "main", "qmux").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["new-session", "-ds", "qmux", "-n", "main", "qmux"]
        );
    }

    #[test]
    fn kill_session_calls_expected_args() {
        let m = MockTmux::new().with_ok("");
        kill_session(&m, "qmux").unwrap();
        assert_eq!(m.last_call(), vec!["kill-session", "-t", "qmux"]);
    }

    #[test]
    fn split_window_returns_pane_id() {
        let m = MockTmux::new().with_ok("%42\n");
        let id = split_window(&m, "qmux:main", "claude").unwrap();
        assert_eq!(id, "%42");
        assert_eq!(
            m.last_call(),
            vec![
                "split-window",
                "-t",
                "qmux:main",
                "-d",
                "-P",
                "-F",
                "#{pane_id}",
                "claude"
            ]
        );
    }

    #[test]
    fn set_pane_option_uses_user_option_prefix() {
        let m = MockTmux::new().with_ok("");
        set_pane_option(&m, "%42", "@qmux_role", "agent").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["set-option", "-p", "-t", "%42", "@qmux_role", "agent"]
        );
    }

    #[test]
    fn set_window_option_uses_window_flag() {
        let m = MockTmux::new().with_ok("");
        set_window_option(&m, "qmux:main", "main-pane-height", "12").unwrap();
        assert_eq!(
            m.last_call(),
            vec![
                "set-option",
                "-w",
                "-t",
                "qmux:main",
                "main-pane-height",
                "12"
            ]
        );
    }

    #[test]
    fn list_panes_parses_pipe_delimited_rows() {
        let raw = "\
%0|controller|||20|0\n\
%1|agent|alice|claude|0|0\n\
%2|agent|bob|codex|0|40\n";
        let m = MockTmux::new().with_ok(raw);
        let rows = list_qmux_panes(&m, "qmux").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].pane_id, "%0");
        assert_eq!(rows[0].role, "controller");
        assert_eq!(rows[0].name, "");
        assert_eq!(rows[0].pane_top, 20);
        assert_eq!(rows[0].pane_left, 0);
        assert_eq!(rows[1].pane_id, "%1");
        assert_eq!(rows[1].role, "agent");
        assert_eq!(rows[1].name, "alice");
        assert_eq!(rows[1].preset, "claude");
        assert_eq!(rows[1].pane_top, 0);
        assert_eq!(rows[2].pane_left, 40);
    }

    #[test]
    fn select_layout_uses_expected_args() {
        let m = MockTmux::new().with_ok("");
        select_layout(&m, "qmux:main", "tiled").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["select-layout", "-t", "qmux:main", "tiled"]
        );
    }

    #[test]
    fn list_adoptable_panes_filters_tagged_panes() {
        // Controller and existing agents have @qmux_role set; only the plain
        // shell pane %3 should come back as adoptable.
        let raw = "\
%0|controller|0|qmux|/home/u/code/qmux\n\
%1|agent|1|claude|/home/u/code/myapp\n\
%3||2|zsh|/home/u/code/frontend\n";
        let m = MockTmux::new().with_ok(raw);
        let panes = list_adoptable_panes(&m, "@1").unwrap();
        assert_eq!(panes.len(), 1);
        let p = &panes[0];
        assert_eq!(p.pane_id, "%3");
        assert_eq!(p.pane_index, 2);
        assert_eq!(p.command, "zsh");
        assert_eq!(p.path, "/home/u/code/frontend");
    }

    #[test]
    fn list_adoptable_panes_returns_empty_when_all_tagged() {
        let raw = "\
%0|controller|0|qmux|/home/u\n\
%1|agent|1|claude|/home/u\n";
        let m = MockTmux::new().with_ok(raw);
        let panes = list_adoptable_panes(&m, "@1").unwrap();
        assert!(panes.is_empty());
    }

    #[test]
    fn list_adoptable_panes_uses_expected_format() {
        let m = MockTmux::new().with_ok("");
        list_adoptable_panes(&m, "@1").unwrap();
        let call = m.last_call();
        assert_eq!(call[0], "list-panes");
        assert_eq!(call[1], "-t");
        assert_eq!(call[2], "@1");
        assert_eq!(call[3], "-F");
        assert!(call[4].contains("#{pane_id}"));
        assert!(call[4].contains("#{@qmux_role}"));
        assert!(call[4].contains("#{pane_index}"));
        assert!(call[4].contains("#{pane_current_command}"));
        assert!(call[4].contains("#{pane_current_path}"));
    }

    #[test]
    fn kill_pane_uses_expected_args() {
        let m = MockTmux::new().with_ok("");
        kill_pane(&m, "%42").unwrap();
        assert_eq!(m.last_call(), vec!["kill-pane", "-t", "%42"]);
    }

    #[test]
    fn swap_pane_uses_detached_flag() {
        let m = MockTmux::new().with_ok("");
        swap_pane(&m, "%1", "%2").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["swap-pane", "-d", "-s", "%1", "-t", "%2"]
        );
    }

    #[test]
    fn ensure_controller_first_swaps_when_controller_not_first() {
        // Controller is %0 but list-panes order shows %1 first (pane-index 0).
        let raw = "\
%1|agent|alice|claude|0|0\n\
%0|controller|||20|0\n";
        let m = MockTmux::new().with_ok(raw).with_ok("");
        ensure_controller_first(&m, "@1").unwrap();
        let calls = m.calls.borrow();
        assert_eq!(calls.len(), 2, "expected list-panes + swap-pane");
        assert_eq!(calls[0][0], "list-panes");
        assert_eq!(calls[1], vec!["swap-pane", "-d", "-s", "%0", "-t", "@1.0"]);
    }

    #[test]
    fn ensure_controller_first_noop_when_controller_already_first() {
        let raw = "\
%0|controller|||0|0\n\
%1|agent|alice|claude|20|0\n";
        let m = MockTmux::new().with_ok(raw);
        ensure_controller_first(&m, "@1").unwrap();
        let calls = m.calls.borrow();
        assert_eq!(calls.len(), 1, "only list-panes, no swap");
        assert_eq!(calls[0][0], "list-panes");
    }

    #[test]
    fn ensure_controller_first_noop_when_no_controller_in_window() {
        let raw = "%1|agent|alice|claude|0|0\n";
        let m = MockTmux::new().with_ok(raw);
        ensure_controller_first(&m, "@1").unwrap();
        assert_eq!(m.calls.borrow().len(), 1);
    }

    #[test]
    fn set_buffer_uses_named_buffer() {
        let m = MockTmux::new().with_ok("");
        set_buffer(&m, "qmux-send", "hello\nworld").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["set-buffer", "-b", "qmux-send", "hello\nworld"]
        );
    }

    #[test]
    fn paste_buffer_uses_bracketed_flag() {
        let m = MockTmux::new().with_ok("");
        paste_buffer_bracketed(&m, "qmux-send", "%42").unwrap();
        assert_eq!(
            m.last_call(),
            vec!["paste-buffer", "-b", "qmux-send", "-p", "-t", "%42"]
        );
    }

    #[test]
    fn send_enter_sends_enter_key() {
        let m = MockTmux::new().with_ok("");
        send_enter(&m, "%42").unwrap();
        assert_eq!(m.last_call(), vec!["send-keys", "-t", "%42", "Enter"]);
    }

    #[test]
    fn capture_pane_to_buffer_uses_scrollback() {
        let m = MockTmux::new().with_ok("");
        capture_pane_to_buffer(&m, "%1", "qmux-capture", 200).unwrap();
        assert_eq!(
            m.last_call(),
            vec![
                "capture-pane",
                "-t",
                "%1",
                "-b",
                "qmux-capture",
                "-S",
                "-200",
                "-J"
            ]
        );
    }

    #[test]
    fn show_buffer_returns_content() {
        let m = MockTmux::new().with_ok("line1\nline2\n");
        let s = show_buffer(&m, "qmux-capture").unwrap();
        assert_eq!(s, "line1\nline2\n");
        assert_eq!(m.last_call(), vec!["show-buffer", "-b", "qmux-capture"]);
    }

    #[test]
    fn delete_buffer_uses_expected_args() {
        let m = MockTmux::new().with_ok("");
        delete_buffer(&m, "qmux-send").unwrap();
        assert_eq!(m.last_call(), vec!["delete-buffer", "-b", "qmux-send"]);
    }

    #[test]
    fn current_pane_role_sends_display_message() {
        // Temporarily set $TMUX_PANE for the test.
        std::env::set_var("TMUX_PANE", "%9");
        let m = MockTmux::new().with_ok("controller\n");
        let r = current_pane_role(&m).unwrap();
        assert_eq!(r, "controller");
        assert_eq!(
            m.last_call(),
            vec!["display-message", "-p", "-t", "%9", "#{@qmux_role}"]
        );
        std::env::remove_var("TMUX_PANE");
    }

    #[test]
    fn select_pane_uses_expected_args() {
        let m = MockTmux::new().with_ok("");
        select_pane(&m, "%3").unwrap();
        assert_eq!(m.last_call(), vec!["select-pane", "-t", "%3"]);
    }

    #[test]
    fn enter_copy_mode_uses_expected_args() {
        let m = MockTmux::new().with_ok("");
        enter_copy_mode(&m, "%3").unwrap();
        assert_eq!(m.last_call(), vec!["copy-mode", "-t", "%3"]);
    }
}
