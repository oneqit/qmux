use crate::tmux::{current_pane_id, set_pane_option, TmuxRunner};
use anyhow::Result;
use std::os::unix::process::CommandExt;

pub const DEFAULT_SESSION: &str = "qmux";
pub const WINDOW_NAME: &str = "qmux";

/// All the ways qmux can decide to launch.
#[derive(Debug, PartialEq, Eq)]
pub enum LaunchAction {
    /// Current pane is already tagged as controller — just run the TUI.
    RunTui,
    /// Current pane is an agent; user probably ran qmux inside an agent by accident.
    RefuseAgentPane,
    /// Inside tmux, current pane untagged — tag it controller and run TUI here.
    TakeOver,
    /// Outside tmux, an existing tmux session exists — create a new window in it
    /// running the controller, then attach.
    NewWindowInSession { session: String, exe: String },
    /// Outside tmux, no server running — create a new session with the controller.
    CreateSession { session: String, exe: String },
}

pub fn plan_launch(
    env_tmux: Option<&str>,
    current_role: &str,
    existing_sessions: &[String],
    exe: String,
) -> LaunchAction {
    if env_tmux.is_some() {
        return match current_role {
            "controller" => LaunchAction::RunTui,
            "agent" => LaunchAction::RefuseAgentPane,
            _ => LaunchAction::TakeOver,
        };
    }
    match existing_sessions.first() {
        Some(s) => LaunchAction::NewWindowInSession {
            session: s.clone(),
            exe,
        },
        None => LaunchAction::CreateSession {
            session: DEFAULT_SESSION.to_string(),
            exe,
        },
    }
}

/// Returns Ok(true) if the caller should now run the TUI in-process,
/// Ok(false) if it already handed off to tmux via execvp.
pub fn execute_launch(tmux: &dyn TmuxRunner, action: LaunchAction) -> Result<bool> {
    match action {
        LaunchAction::RunTui => Ok(true),
        LaunchAction::RefuseAgentPane => {
            anyhow::bail!("this pane is a qmux agent; run qmux from a plain pane");
        }
        LaunchAction::TakeOver => {
            let pane = current_pane_id()?;
            set_pane_option(tmux, &pane, "@qmux_role", "controller")?;
            Ok(true)
        }
        LaunchAction::NewWindowInSession { session, exe } => {
            let target = format!("{}:", session);
            tmux.run(&["new-window", "-t", &target, "-n", WINDOW_NAME, &exe])?;
            exec_tmux(&["attach-session", "-t", &session])?;
            Ok(false)
        }
        LaunchAction::CreateSession { session, exe } => {
            exec_tmux(&["new-session", "-s", &session, "-n", WINDOW_NAME, &exe])?;
            Ok(false)
        }
    }
}

fn exec_tmux(args: &[&str]) -> Result<()> {
    let err = std::process::Command::new("tmux").args(args).exec();
    Err(anyhow::anyhow!("execvp failed: {}", err))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/path/to/qmux";

    fn exe() -> String {
        EXE.to_string()
    }

    #[test]
    fn inside_tmux_controller_runs_tui() {
        assert_eq!(
            plan_launch(Some("sock"), "controller", &[], exe()),
            LaunchAction::RunTui
        );
    }

    #[test]
    fn inside_tmux_agent_is_refused() {
        assert_eq!(
            plan_launch(Some("sock"), "agent", &[], exe()),
            LaunchAction::RefuseAgentPane
        );
    }

    #[test]
    fn inside_tmux_untagged_takes_over() {
        assert_eq!(
            plan_launch(Some("sock"), "", &[], exe()),
            LaunchAction::TakeOver
        );
    }

    #[test]
    fn outside_tmux_no_sessions_creates_session() {
        assert_eq!(
            plan_launch(None, "", &[], exe()),
            LaunchAction::CreateSession {
                session: "qmux".into(),
                exe: EXE.into(),
            }
        );
    }

    #[test]
    fn outside_tmux_with_sessions_opens_new_window() {
        assert_eq!(
            plan_launch(None, "", &["work".into(), "play".into()], exe()),
            LaunchAction::NewWindowInSession {
                session: "work".into(),
                exe: EXE.into(),
            }
        );
    }
}
