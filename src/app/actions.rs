use crate::agent::Preset;
use crate::app::App;
use crate::tmux::{capture_pane_to_buffer, show_buffer};
use crate::tmux::{delete_buffer, paste_buffer_bracketed, send_enter, set_buffer};
use crate::tmux::{
    ensure_controller_first, kill_pane, select_layout, set_pane_option, split_window, swap_pane,
};
use crate::tmux::{enter_copy_mode, select_pane, show_latest_buffer};
use anyhow::Result;

pub fn spawn_agent(app: &mut App, preset_id: &str, preset: &Preset, name: &str) -> Result<String> {
    let target = app.window_id.clone();
    // Chain an interactive shell after the agent so Ctrl+C'ing out of the app
    // (claude/codex/…) drops into zsh instead of letting tmux close the pane.
    let wrapped = format!("{}; exec ${{SHELL:-zsh}} -i", preset.launch_cmd);
    let pane_id = split_window(app.tmux.as_ref(), &target, &wrapped)?;
    set_pane_option(app.tmux.as_ref(), &pane_id, "@qmux_role", "agent")?;
    set_pane_option(app.tmux.as_ref(), &pane_id, "@qmux_name", name)?;
    set_pane_option(app.tmux.as_ref(), &pane_id, "@qmux_preset", preset_id)?;
    let layout = app.current_layout();
    let _ = ensure_controller_first(app.tmux.as_ref(), &target);
    select_layout(app.tmux.as_ref(), &target, layout)?;
    app.refresh();
    Ok(pane_id)
}

/// Reorder visible agent panes so that `new_pane_ids` appear at the tail of
/// the list in the given order, while existing (non-listed) panes keep their
/// current relative order. Used after spawn to make selection order match
/// visual order regardless of the current layout.
pub fn reorder_to_end(app: &mut App, new_pane_ids: &[String]) -> Result<()> {
    let current: Vec<String> = app.agents.iter().map(|a| a.pane_id.clone()).collect();
    let new_set: std::collections::HashSet<&String> = new_pane_ids.iter().collect();
    let existing: Vec<String> = current
        .iter()
        .filter(|id| !new_set.contains(id))
        .cloned()
        .collect();
    let desired: Vec<String> = existing
        .into_iter()
        .chain(new_pane_ids.iter().cloned())
        .collect();
    let plan = compute_swap_plan(&current, &desired);
    for (a, b) in plan {
        swap_pane(app.tmux.as_ref(), &a, &b)?;
    }
    app.refresh();
    Ok(())
}

/// Pure function that, given a current pane-id order and a desired pane-id
/// order, returns the sequence of swap-pane (src, dst) pairs needed to
/// transform current into desired. The algorithm greedily fills positions
/// left-to-right: for each target position, if the pane already there is not
/// the desired one, find the desired pane further right and swap.
pub fn compute_swap_plan(current: &[String], desired: &[String]) -> Vec<(String, String)> {
    let mut working = current.to_vec();
    let mut plan = Vec::new();
    for (target_pos, desired_id) in desired.iter().enumerate() {
        if working.get(target_pos) == Some(desired_id) {
            continue;
        }
        let Some(offset) = working
            .iter()
            .skip(target_pos)
            .position(|id| id == desired_id)
        else {
            continue;
        };
        let src_pos = target_pos + offset;
        plan.push((working[target_pos].clone(), working[src_pos].clone()));
        working.swap(target_pos, src_pos);
    }
    plan
}

pub fn suggest_name(existing: &[String], preset_id: &str) -> String {
    for i in 1..1000 {
        let candidate = format!("{}-{}", preset_id, i);
        if !existing.iter().any(|n| n == &candidate) {
            return candidate;
        }
    }
    format!("{}-x", preset_id)
}

pub const SEND_BUFFER: &str = "qmux-send";

/// Delay between bracketed paste and the submit Enter. Some agents
/// (notably Gemini CLI) still treat the trailing `\r` as part of the paste
/// when it arrives too soon after `\e[201~`, turning submit into a newline.
/// Applied uniformly — the cost is invisible for agents that don't need it.
const PASTE_SUBMIT_DELAY_MS: u64 = 50;

pub fn broadcast_send(app: &mut App, prompt: &str, target_pane_ids: &[String]) -> Result<()> {
    if prompt.is_empty() {
        anyhow::bail!("empty prompt");
    }
    if target_pane_ids.is_empty() {
        anyhow::bail!("no targets selected");
    }
    set_buffer(app.tmux.as_ref(), SEND_BUFFER, prompt)?;
    let mut last_err: Option<anyhow::Error> = None;
    for pane in target_pane_ids {
        if let Err(e) = paste_buffer_bracketed(app.tmux.as_ref(), SEND_BUFFER, pane) {
            last_err = Some(e);
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(PASTE_SUBMIT_DELAY_MS));
        if let Err(e) = send_enter(app.tmux.as_ref(), pane) {
            last_err = Some(e);
        }
    }
    let _ = delete_buffer(app.tmux.as_ref(), SEND_BUFFER);
    if let Some(e) = last_err {
        return Err(e);
    }
    Ok(())
}

/// Register a user-created pane as a qmux agent by tagging it with the
/// three `@qmux_*` options and refreshing the agent list.
pub fn adopt_pane(app: &mut App, pane_id: &str, preset_id: &str, name: &str) -> Result<()> {
    set_pane_option(app.tmux.as_ref(), pane_id, "@qmux_role", "agent")?;
    set_pane_option(app.tmux.as_ref(), pane_id, "@qmux_name", name)?;
    set_pane_option(app.tmux.as_ref(), pane_id, "@qmux_preset", preset_id)?;
    app.refresh();
    Ok(())
}

pub fn kill_agent(app: &mut App, pane_id: &str) -> Result<()> {
    // Always re-apply layout and refresh, even if kill fails — a "pane not
    // found" error usually means it already died out-of-band, and the UI
    // still benefits from re-flowing the remaining panes.
    let kill_result = kill_pane(app.tmux.as_ref(), pane_id);
    let layout = app.current_layout();
    let target = app.window_id.clone();
    let _ = ensure_controller_first(app.tmux.as_ref(), &target);
    let _ = select_layout(app.tmux.as_ref(), &target, layout);
    app.refresh();
    kill_result
}

#[derive(Copy, Clone, Debug)]
pub enum SwapDir {
    Up,
    Down,
}

pub fn swap_agent(app: &mut App, dir: SwapDir) -> Result<()> {
    let Some(cursor) = app.cursor else {
        return Ok(());
    };
    let target_idx = match dir {
        SwapDir::Up if cursor == 0 => return Ok(()),
        SwapDir::Up => cursor - 1,
        SwapDir::Down if cursor + 1 >= app.agents.len() => return Ok(()),
        SwapDir::Down => cursor + 1,
    };
    let src = app.agents[cursor].pane_id.clone();
    let tgt = app.agents[target_idx].pane_id.clone();
    swap_pane(app.tmux.as_ref(), &src, &tgt)?;
    app.refresh();
    if target_idx < app.agents.len() {
        app.cursor = Some(target_idx);
    }
    Ok(())
}

pub const CAPTURE_BUFFER: &str = "qmux-capture";
pub const DEFAULT_CAPTURE_LINES: u32 = 500;

pub fn capture_for_forward(
    app: &mut App,
    pane_id: &str,
    markers: Option<(&str, &str, &[String])>,
) -> Result<String> {
    capture_pane_to_buffer(
        app.tmux.as_ref(),
        pane_id,
        CAPTURE_BUFFER,
        DEFAULT_CAPTURE_LINES,
    )?;
    let raw = show_buffer(app.tmux.as_ref(), CAPTURE_BUFFER)?;
    let _ = crate::tmux::delete_buffer(app.tmux.as_ref(), CAPTURE_BUFFER);
    let cleaned = strip_ansi(&raw);
    match markers {
        Some((start, end, body_end)) => match extract_last_response(&cleaned, start, end, body_end)
        {
            Ok(text) => Ok(text),
            // Soft fail — agent has not produced a response yet. Surfaced
            // as an informational status, not as a red error line.
            Err(ExtractError::NoResponse) => Err(anyhow::anyhow!("no response to forward yet")),
            // Hard error — end marker missing means we cannot delimit the
            // response reliably, which usually points at a misconfigured
            // preset marker.
            Err(ExtractError::MissingEnd) => Err(anyhow::anyhow!(
                "forward error: end marker '{}' not found — check preset config",
                end
            )),
        },
        None => Ok(cleaned),
    }
}

/// Look up the (start, end, post_cutoff) bundle for a preset id, merging
/// builtins with the user's config. Returns `None` when the preset has no
/// start marker (smart extraction disabled).
pub fn resolve_markers(preset_id: &str) -> Option<(String, String, Vec<String>)> {
    use crate::config::{default_config_path, load_user_config, merged_presets};
    let user_cfg = default_config_path()
        .and_then(|p| load_user_config(&p).ok())
        .unwrap_or_default();
    let preset = merged_presets(&user_cfg).get(preset_id)?.clone();
    let start = preset.response_start_marker.clone()?;
    Some((
        start,
        preset.effective_end_marker(),
        preset.post_cutoff_markers.clone(),
    ))
}

/// Clip captured buffer text down to the last assistant response:
///   1. strip a leading 2-space indent from every line (common paste artifact)
///   2. find the last line whose trimmed form starts with `start_marker`
///      (the most recent response header)
///   3. find the end marker's LAST occurrence in the buffer (rposition
///      scanning from the end) — that's the current shell prompt, which
///      always sits below all response content. Marker occurrences inside
///      the response body (e.g. `if x > 3:` code with `>` as marker) are
///      naturally included because they land between start and end.
///   4. trim_end each line; drop trailing blank lines
///
/// If the start marker is absent in the buffer, return the input unchanged
/// (fallback).
#[derive(Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// Start marker not found — agent has produced no response yet.
    /// Expected / soft fail case.
    NoResponse,
    /// End marker (shell prompt char) missing — usually indicates a
    /// misconfigured preset marker. Hard error.
    MissingEnd,
}

pub fn extract_last_response(
    raw: &str,
    start_marker: &str,
    end_marker: &str,
    post_cutoff_markers: &[String],
) -> Result<String, ExtractError> {
    let lines: Vec<String> = raw
        .lines()
        .map(|l| l.strip_prefix("  ").unwrap_or(l).to_string())
        .collect();

    // End marker (current shell prompt) is the reliable anchor — it is
    // always rendered once the agent is idle. Its absence usually means the
    // preset's end marker is misconfigured.
    let end = lines
        .iter()
        .rposition(|l| l.contains(end_marker))
        .ok_or(ExtractError::MissingEnd)?;

    // Start marker absent = agent has produced no response yet. That is an
    // expected state, not a misconfiguration.
    let start = lines[..end]
        .iter()
        .rposition(|l| l.trim_start().starts_with(start_marker))
        .ok_or(ExtractError::NoResponse)?;

    let mut out: Vec<String> = lines[start..end]
        .iter()
        .map(|l| l.trim_end().to_string())
        .collect();

    // Post-processing cutoff: inside the already-bounded [start, end)
    // window, cut at the first line containing any cutoff marker. Scoped
    // to this window so response-body text above the primary boundary is
    // never affected.
    if !post_cutoff_markers.is_empty() {
        if let Some(cut) = out
            .iter()
            .position(|l| post_cutoff_markers.iter().any(|m| l.contains(m.as_str())))
        {
            out.truncate(cut);
        }
    }

    while out.last().is_some_and(|l| is_tail_chrome(l)) {
        out.pop();
    }
    Ok(out.join("\n"))
}

/// Tail-only cleanup: a line is considered chrome and should be popped off
/// the end of an extracted response if it is empty *or* consists entirely of
/// separator characters (box drawing / block elements / ASCII dash-equals-etc).
/// Applied only at the tail so the same characters inside the response body
/// (e.g. a markdown `---` horizontal rule in the middle of an answer) are
/// preserved.
fn is_tail_chrome(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return true;
    }
    t.chars().all(|c| {
        matches!(c,
            '\u{2500}'..='\u{257F}' | // Box Drawing
            '\u{2580}'..='\u{259F}' | // Block Elements
            '-' | '=' | '_' | '*' | '.'
        )
    })
}

pub fn delegate_to_copy_mode(app: &mut App, pane_id: &str) -> Result<()> {
    select_pane(app.tmux.as_ref(), pane_id)?;
    enter_copy_mode(app.tmux.as_ref(), pane_id)?;
    Ok(())
}

pub fn pull_latest_buffer(app: &App) -> Result<String> {
    let raw = show_latest_buffer(app.tmux.as_ref())?;
    Ok(strip_ansi(&raw))
}

/// Remove common ANSI CSI sequences. Not exhaustive, but enough for MVP previews.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bytes = s.chars().peekable();
    while let Some(c) = bytes.next() {
        if c == '\u{1b}' {
            if let Some(&n) = bytes.peek() {
                if n == '[' {
                    bytes.next();
                    while let Some(&b) = bytes.peek() {
                        bytes.next();
                        if ('\x40'..='\x7e').contains(&b) {
                            break;
                        }
                    }
                    continue;
                } else if n == ']' {
                    bytes.next();
                    while let Some(&b) = bytes.peek() {
                        bytes.next();
                        if b == '\x07' {
                            break;
                        }
                        if b == '\u{1b}' {
                            if let Some(&x) = bytes.peek() {
                                bytes.next();
                                if x == '\\' {
                                    break;
                                }
                            }
                        }
                    }
                    continue;
                } else {
                    bytes.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests_forward {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_colors() {
        let s = "\u{1b}[31mred\u{1b}[0m plain";
        assert_eq!(strip_ansi(s), "red plain");
    }

    #[test]
    fn strip_ansi_removes_osc() {
        let s = "\u{1b}]0;title\u{07}after";
        assert_eq!(strip_ansi(s), "after");
    }

    #[test]
    fn strip_ansi_preserves_non_escapes() {
        assert_eq!(strip_ansi("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn extract_errors_missing_end_when_no_prompt_in_buffer() {
        // Both markers absent — but end is checked first, so MissingEnd wins.
        let raw = "no marker here\njust plain text";
        assert_eq!(
            extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]),
            Err(ExtractError::MissingEnd)
        );
    }

    #[test]
    fn extract_errors_no_response_when_only_prompt_present() {
        // End marker present (freshly launched agent waiting for input) but
        // no start marker — soft fail: "no response yet".
        let raw = "Welcome banner\nTips go here\n\u{276F}\n/ commands";
        assert_eq!(
            extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]),
            Err(ExtractError::NoResponse)
        );
    }

    #[test]
    fn extract_errors_missing_end_when_buffer_has_only_response() {
        // Start present but end marker nowhere — hard error, caller should
        // flag the preset config.
        let raw = "\u{25CF} only\n  body";
        assert_eq!(
            extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]),
            Err(ExtractError::MissingEnd)
        );
    }

    #[test]
    fn extract_takes_last_marker_block_until_prompt() {
        let raw = "\
\u{25CF} first response
  body one

\u{25CF} Read event.rs
  \u{2502} src/app/event.rs
  \u{2514} L1:260

\u{25CF} final summary
  line one
  line two
~/code/oneqit/qmux
\u{276F}
 / commands    model info
";
        let got = extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]).unwrap();
        assert_eq!(
            got,
            "\u{25CF} final summary\nline one\nline two\n~/code/oneqit/qmux"
        );
    }

    #[test]
    fn extract_strips_only_one_2space_level() {
        // "    nested" has 4 leading spaces — only the first 2 are stripped.
        let raw = "\u{25CF} head\n  direct body\n    nested\n\u{276F}";
        let got = extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]).unwrap();
        assert_eq!(got, "\u{25CF} head\ndirect body\n  nested");
    }

    #[test]
    fn extract_uses_last_marker_not_first() {
        let raw = "\u{25CF} older\n  body1\n\u{25CF} newer\n  body2\n\u{276F}";
        assert_eq!(
            extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]).unwrap(),
            "\u{25CF} newer\nbody2"
        );
    }

    #[test]
    fn extract_with_gt_end_marker_includes_code_gt_sign() {
        // Gemini uses '>' as end. We search end from the BACK of the buffer,
        // so the current shell prompt wins over any '>' embedded in code.
        // The `if x > 3:` line stays inside the extracted response.
        let raw = "\
\u{2726} here is code
  1 if x > 3:
  2     print(x)
 >   Type your message or @path/to/file
";
        let got = extract_last_response(raw, "\u{2726}", ">", &[]).unwrap();
        assert_eq!(got, "\u{2726} here is code\n1 if x > 3:\n2     print(x)");
    }

    #[test]
    fn extract_real_claude_capture() {
        let raw = include_str!("testdata/claude.txt");
        let got = extract_last_response(raw, "\u{23FA}", "\u{276F}", &[]).unwrap();
        // Last ⏺ in the real capture is a Python script response.
        assert!(got.starts_with("\u{23FA} import random"), "got:\n{}", got);
        assert!(got.contains("guess_number()"));
        // Previous turn (동작 테스트) must NOT be included.
        assert!(
            !got.contains("네, 동작합니다"),
            "older response leaked: {}",
            got
        );
        // Tail chrome cleanup: the trailing ─── separator is gone.
        assert!(
            !got.trim_end().ends_with('\u{2500}'),
            "tail ─── leaked: {:?}",
            got
        );
    }

    #[test]
    fn extract_pops_dash_and_blank_tail() {
        // The exact shape the user reported for Claude: response line, blank,
        // then a long ─── separator. Both tail lines must be dropped.
        let raw = "\
\u{23FA} 테스트 응답입니다. 무엇을 도와드릴까요?

\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}
\u{276F}
";
        let got = extract_last_response(raw, "\u{23FA}", "\u{276F}", &[]).unwrap();
        assert_eq!(got, "\u{23FA} 테스트 응답입니다. 무엇을 도와드릴까요?");
    }

    #[test]
    fn extract_real_copilot_capture() {
        let raw = include_str!("testdata/copilot.txt");
        let got = extract_last_response(raw, "\u{25CF}", "\u{276F}", &[]).unwrap();
        // Copilot puts the bullet with a trailing space: `●  import random`.
        assert!(got.starts_with("\u{25CF}  import random"), "got:\n{}", got);
        assert!(got.contains("main()"));
        assert!(
            !got.contains("정상 동작 중입니다"),
            "older response leaked: {}",
            got
        );
    }

    #[test]
    fn extract_real_gemini_capture() {
        let raw = include_str!("testdata/gemini.txt");
        let got = extract_last_response(raw, "\u{2726}", ">", &[]).unwrap();
        assert!(got.starts_with("\u{2726} 간단한 HTTP"), "got:\n{}", got);
        assert!(got.contains("import requests"));
        // Earlier cancelled turn must NOT leak in.
        assert!(
            !got.contains("operational status"),
            "older ✦ block leaked: {}",
            got
        );
    }

    #[test]
    fn extract_real_codex_capture() {
        let raw = include_str!("testdata/codex.txt");
        let got = extract_last_response(raw, "\u{2022}", "\u{203A}", &[]).unwrap();
        // Last • block is the 5-line Python sample.
        assert!(
            got.starts_with("\u{2022} name = \"Codex\""),
            "got:\n{}",
            got
        );
        assert!(got.contains("print(\"sum =\", total)"));
        // Earlier turns must not leak in.
        assert!(!got.contains("import math;print(math.pi)"), "got:\n{}", got);
        assert!(!got.contains("정상 동작 중입니다"), "got:\n{}", got);
        // The current placeholder prompt (`› Explain this codebase`) and
        // post-prompt chrome are excluded.
        assert!(!got.contains("Explain this codebase"), "got:\n{}", got);
    }

    #[test]
    fn extract_real_codex_buffer_from_user() {
        // Verbatim Codex pane buffer the user provided while reporting the
        // "forward pulls the whole buffer" regression. Codex markers are
        // • (U+2022) for assistant and › (U+203A) for the user prompt.
        let raw = concat!(
            "\u{256D}\u{2500}\u{2500}\u{2500}\u{256E}\n",
            "\u{2502} >_ OpenAI Codex (v0.121.0)                         \u{2502}\n",
            "\u{2502}                                                    \u{2502}\n",
            "\u{2502} model:     gpt-5.3-codex medium   /model to change \u{2502}\n",
            "\u{2502} directory: ~/code/oneqit/qmux                      \u{2502}\n",
            "\u{2570}\u{2500}\u{2500}\u{2500}\u{256F}\n",
            "\n",
            "  Tip: Run /review to get a code review of your current changes.\n",
            "\n",
            "\u{26A0} Heads up, you have less than 5% of your weekly limit left. Run /status for a breakdown.\n",
            "\n",
            "\u{256D}\u{2500}\u{2500}\u{2500}\u{256E}\n",
            "\u{2502} >_ OpenAI Codex (v0.121.0)                         \u{2502}\n",
            "\u{2570}\u{2500}\u{2500}\u{2500}\u{256F}\n",
            "\n",
            "  Tip: New For a limited time, Codex is included in your plan for free.\n",
            "\n",
            "\n",
            "\u{203A} codex 응답 테스트\n",
            "\n",
            "\n",
            "\u{2022} 테스트 응답 정상입니다. 필요한 작업을 말씀해 주세요.\n",
            "\n",
            "\n",
            "\u{203A} python으로 원주율 계산하는 가장 짧은 스크립트 출력해줘\n",
            "\n",
            "\n",
            "\u{2022} import math;print(math.pi)\n",
            "\n",
            "\n",
            "\u{203A} python 스크립트 예제 5줄짜리 아무거나 출력해줘\n",
            "\n",
            "\n",
            "\u{2022} name = \"Codex\"\n",
            "  for i in range(1, 4):\n",
            "      print(f\"{i}: Hello, {name}!\")\n",
            "  total = sum(range(1, 11))\n",
            "  print(\"sum =\", total)\n",
            "\n",
            "To continue this session, run codex resume 019d9f70-7d5f-7412-8729-0b8e081932bc\n",
            "\n",
            "\n",
            "\u{203A} Explain this codebase\n",
            "\n",
            "  gpt-5.3-codex medium \u{00B7} ~/code/oneqit/qmux\n",
        );

        let got = extract_last_response(raw, "\u{2022}", "\u{203A}", &[]).unwrap();
        let expected = concat!(
            "\u{2022} name = \"Codex\"\n",
            "for i in range(1, 4):\n",
            "    print(f\"{i}: Hello, {name}!\")\n",
            "total = sum(range(1, 11))\n",
            "print(\"sum =\", total)\n",
            "\n",
            "To continue this session, run codex resume 019d9f70-7d5f-7412-8729-0b8e081932bc",
        );
        assert_eq!(got, expected);
    }

    #[test]
    fn extract_gemini_stops_at_prompt_line_even_with_chrome_between() {
        let raw = "\
\u{2726} short answer
  details here
ℹ Gemini update notice
  follow-up detail

                                                                                    ? for shortcuts
────────
 Shift+Tab to accept edits
▀▀▀▀▀▀▀▀
 >   Type your message
▄▄▄▄▄▄▄▄
";
        let got = extract_last_response(raw, "\u{2726}", ">", &[]).unwrap();
        // Chrome (notice, separators, box borders) is included since only the
        // prompt line '>' matches. That's the trade-off — easy to clean up in
        // the editor textarea.
        assert!(got.starts_with("\u{2726} short answer"));
        assert!(got.contains("details here"));
        assert!(
            !got.contains("Type your message"),
            "prompt line must be excluded"
        );
    }

    #[test]
    fn post_cutoff_trims_gemini_footer_and_update_banner() {
        let raw = "\
\u{2726} 안녕하세요! qmux 프로젝트에서 도와드릴 일이
  있을까요?

\u{2139} Gemini CLI update available! 0.38.1 -> 0.38.2

───────────────────────────────────────────────
 Shift+Tab to accept edits

 > Type your message
";
        let cutoffs = vec!["\u{2139}".to_string(), "Shift+Tab".to_string()];
        let got = extract_last_response(raw, "\u{2726}", ">", &cutoffs).unwrap();
        assert!(got.starts_with("\u{2726} 안녕하세요!"));
        assert!(got.contains("있을까요?"));
        assert!(
            !got.contains("update available"),
            "update banner must be dropped"
        );
        assert!(!got.contains("Shift+Tab"), "footer must be dropped");
        assert!(
            !got.contains("─"),
            "trailing separator must be gone after tail-chrome pass"
        );
    }

    #[test]
    fn post_cutoff_trims_copilot_git_prompt_line() {
        let raw = "\
\u{25CF} 테스트 성공입니다.




 ~/.../qmux [\u{2387} main*]
\u{276F}
";
        let cutoffs = vec!["\u{2387}".to_string()];
        let got = extract_last_response(raw, "\u{25CF}", "\u{276F}", &cutoffs).unwrap();
        assert!(got.starts_with("\u{25CF} 테스트 성공입니다."));
        assert!(!got.contains("\u{2387}"), "git prompt line must be dropped");
    }

    #[test]
    fn post_cutoff_does_not_affect_body_above_primary_boundary() {
        // Cutoff marker that happens to appear inside the response body must
        // still cut — but only from the first match forward. Content above
        // the match is preserved.
        let raw = "\
\u{25CF} line one
  line two
  ⎇ accidental glyph in body
  line four
\u{276F}
";
        let cutoffs = vec!["\u{2387}".to_string()];
        let got = extract_last_response(raw, "\u{25CF}", "\u{276F}", &cutoffs).unwrap();
        assert!(got.starts_with("\u{25CF} line one"));
        assert!(got.contains("line two"));
        assert!(
            !got.contains("line four"),
            "lines after the cutoff are dropped"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_swap_plan_noop_when_orders_match() {
        let current = vec!["%1".to_string(), "%2".to_string(), "%3".to_string()];
        let plan = compute_swap_plan(&current, &current);
        assert!(plan.is_empty());
    }

    #[test]
    fn compute_swap_plan_single_swap() {
        // current: A, B, C — desired: A, C, B → one swap (%2 with %3).
        let current = vec!["%1".to_string(), "%2".to_string(), "%3".to_string()];
        let desired = vec!["%1".to_string(), "%3".to_string(), "%2".to_string()];
        let plan = compute_swap_plan(&current, &desired);
        assert_eq!(plan, vec![("%2".to_string(), "%3".to_string())]);
    }

    #[test]
    fn compute_swap_plan_reverses_order_with_two_swaps() {
        // current: %1 %2 %3 — desired: %3 %2 %1
        // step 1: position 0 wants %3. Swap %1 ↔ %3 → working = %3 %2 %1.
        // step 2: position 1 wants %2. Already correct.
        // step 3: position 2 wants %1. Already correct.
        let current = vec!["%1".to_string(), "%2".to_string(), "%3".to_string()];
        let desired = vec!["%3".to_string(), "%2".to_string(), "%1".to_string()];
        let plan = compute_swap_plan(&current, &desired);
        assert_eq!(plan, vec![("%1".to_string(), "%3".to_string())]);
    }

    #[test]
    fn compute_swap_plan_spawn_scenario() {
        // Before spawn: existing agents at positions 0,1 in main-h-mirrored.
        // Two new agents %3 and %4 were spawned in that selection order, but
        // tmux placed them reversed (newest first) → current: %4 %3 %1 %2.
        // Desired: %1 %2 %3 %4 (existing first, then selection order).
        let current = vec![
            "%4".to_string(),
            "%3".to_string(),
            "%1".to_string(),
            "%2".to_string(),
        ];
        let desired = vec![
            "%1".to_string(),
            "%2".to_string(),
            "%3".to_string(),
            "%4".to_string(),
        ];
        let plan = compute_swap_plan(&current, &desired);
        // Walk it: step 0 wants %1 (at index 2) → swap %4↔%1 → [%1 %3 %4 %2].
        //          step 1 wants %2 (at index 3) → swap %3↔%2 → [%1 %2 %4 %3].
        //          step 2 wants %3 (at index 3) → swap %4↔%3 → [%1 %2 %3 %4].
        //          step 3: already matches.
        assert_eq!(
            plan,
            vec![
                ("%4".to_string(), "%1".to_string()),
                ("%3".to_string(), "%2".to_string()),
                ("%4".to_string(), "%3".to_string()),
            ]
        );
    }

    #[test]
    fn compute_swap_plan_ignores_missing_ids() {
        // Desired contains an ID not in current — skip it without failing.
        let current = vec!["%1".to_string(), "%2".to_string()];
        let desired = vec!["%1".to_string(), "%99".to_string(), "%2".to_string()];
        let plan = compute_swap_plan(&current, &desired);
        assert!(plan.is_empty());
    }

    #[test]
    fn spawn_agent_swaps_controller_to_first_before_select_layout() {
        use crate::agent::Preset;
        use crate::app::App;
        use crate::tmux::mock::MockTmux;

        const CTRL_NOT_FIRST: &str = "\
%1|agent|alice|claude|0|0\n\
%0|controller|||20|0\n";

        let m = MockTmux::new()
            .with_ok("%9\n") // split-window → new pane %9
            .with_ok("") // set-option @qmux_role
            .with_ok("") // set-option @qmux_name
            .with_ok("") // set-option @qmux_preset
            .with_ok(CTRL_NOT_FIRST) // list-panes (ensure_controller_first)
            .with_ok("") // swap-pane
            .with_ok("") // select-layout
            .with_ok(""); // refresh → fetch_agents list-panes
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        let preset = Preset {
            display_name: "Claude".into(),
            binary: "claude".into(),
            launch_cmd: "claude".into(),
            response_start_marker: None,
            response_end_marker: None,
            post_cutoff_markers: Vec::new(),
        };
        let _ = spawn_agent(&mut app, "claude", &preset, "a1");
        let calls = handle.borrow();
        let swap_idx = calls
            .iter()
            .position(|c| c.first().map(String::as_str) == Some("swap-pane"))
            .expect("swap-pane should be called");
        let sel_idx = calls
            .iter()
            .position(|c| c.first().map(String::as_str) == Some("select-layout"))
            .expect("select-layout should be called");
        assert!(swap_idx < sel_idx, "swap-pane must precede select-layout");
    }

    #[test]
    fn kill_agent_swaps_controller_to_first_before_select_layout() {
        use crate::app::App;
        use crate::tmux::mock::MockTmux;

        const CTRL_NOT_FIRST: &str = "\
%1|agent|alice|claude|0|0\n\
%0|controller|||20|0\n";

        let m = MockTmux::new()
            .with_ok("") // kill-pane
            .with_ok(CTRL_NOT_FIRST) // list-panes (ensure_controller_first)
            .with_ok("") // swap-pane
            .with_ok("") // select-layout
            .with_ok(""); // refresh
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        let _ = kill_agent(&mut app, "%1");
        let calls = handle.borrow();
        let swap_idx = calls
            .iter()
            .position(|c| c.first().map(String::as_str) == Some("swap-pane"))
            .expect("swap-pane should be called");
        let sel_idx = calls
            .iter()
            .position(|c| c.first().map(String::as_str) == Some("select-layout"))
            .expect("select-layout should be called");
        assert!(swap_idx < sel_idx, "swap-pane must precede select-layout");
    }

    #[test]
    fn adopt_pane_sets_role_name_and_preset_options() {
        use crate::app::App;
        use crate::tmux::mock::MockTmux;

        let m = MockTmux::new()
            .with_ok("") // set-option @qmux_role
            .with_ok("") // set-option @qmux_name
            .with_ok("") // set-option @qmux_preset
            .with_ok(""); // refresh → fetch_agents
        let handle = m.calls_handle();
        let mut app = App::new(Box::new(m), "@1".into());
        adopt_pane(&mut app, "%5", "claude", "my-claude").unwrap();
        let calls = handle.borrow();
        let set_calls: Vec<&Vec<String>> = calls
            .iter()
            .filter(|c| c.first().map(String::as_str) == Some("set-option"))
            .collect();
        assert_eq!(set_calls.len(), 3, "three set-option calls expected");
        assert_eq!(
            set_calls[0],
            &vec!["set-option", "-p", "-t", "%5", "@qmux_role", "agent"]
        );
        assert_eq!(
            set_calls[1],
            &vec!["set-option", "-p", "-t", "%5", "@qmux_name", "my-claude"]
        );
        assert_eq!(
            set_calls[2],
            &vec!["set-option", "-p", "-t", "%5", "@qmux_preset", "claude"]
        );
    }

    #[test]
    fn suggest_name_picks_lowest_free_suffix() {
        assert_eq!(suggest_name(&[], "claude"), "claude-1");
        assert_eq!(suggest_name(&["claude-1".into()], "claude"), "claude-2");
        assert_eq!(
            suggest_name(&["claude-1".into(), "claude-2".into()], "claude"),
            "claude-3"
        );
        assert_eq!(suggest_name(&["claude-2".into()], "claude"), "claude-1");
    }
}
