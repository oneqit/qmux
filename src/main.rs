use anyhow::Result;
use qmux::launcher::{execute_launch, plan_launch};
use qmux::tmux::{current_pane_role, list_sessions, RealTmux};
use qmux::update::run_update;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("update")) {
        run_update(&args[1..])?;
        return Ok(());
    }
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--version" | "-v" | "-V"))
    {
        println!("qmux {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-h" | "help")
    ) {
        print!("{}", main_help());
        return Ok(());
    }

    let tmux = RealTmux;
    let env_tmux = std::env::var("TMUX").ok();

    let current_role = if env_tmux.is_some() {
        current_pane_role(&tmux).unwrap_or_default()
    } else {
        String::new()
    };

    let sessions = if env_tmux.is_none() {
        list_sessions(&tmux)
    } else {
        Vec::new()
    };

    let exe = std::env::current_exe()?.to_string_lossy().into_owned();

    let action = plan_launch(env_tmux.as_deref(), &current_role, &sessions, exe);

    if execute_launch(&tmux, action)? {
        qmux::app::run()?;
    }
    Ok(())
}

fn main_help() -> &'static str {
    "qmux - tmux-based orchestrator for CLI AI agents\n\
     \n\
     Usage:\n\
       qmux                 Launch controller TUI\n\
       qmux update [opts]   Update qmux binary in-place\n\
       qmux --version       Print version\n\
     \n\
     For update options:\n\
       qmux update --help\n"
}
