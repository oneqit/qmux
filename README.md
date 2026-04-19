# qmux

A tmux-based orchestrator for CLI AI agents (Claude Code, Codex, Gemini CLI, GitHub Copilot CLI).

Run multiple agents side by side in a single tmux session, broadcast the same prompt to several of them at once, or forward one agent's output to another — all from a single controller pane.

## Features

- **Single binary** — one `qmux` command, no daemons, no IPC
- **Auto-detection** — only agents found on `$PATH` appear in the menu
- **Built-in presets** for `claude`, `codex`, `gemini`, `copilot`
- **User-defined presets** via `~/.config/oneqit/qmux/config.yaml`
- **Adopt** existing tmux panes as managed agents
- **Broadcast** a prompt to multiple agents simultaneously
- **Forward** captured output from one agent to another, with a preview/edit step
- **Layout + ordering controls** for quick pane reshaping and agent reordering
- **Session-resident** — quit the controller, agent panes keep running

## Requirements

- `tmux` 3.2+
- Rust 1.75+ (only to build)
- One or more agent CLIs installed and on `$PATH`

## Install (Prebuilt Binary)

Recommended: run the installer script.

```sh
curl -fsSL https://raw.githubusercontent.com/oneqit/qmux/main/install.sh | sh
```

Install a specific version:

```sh
curl -fsSL https://raw.githubusercontent.com/oneqit/qmux/main/install.sh | sh -s -- --version v0.1.0
```

Optional flags:

- `--bin-dir <path>`: install location override
- `--no-verify`: skip checksum verification (not recommended)

Current prebuilt targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`

## Install (Build From Source)

```sh
git clone https://github.com/oneqit/qmux.git
cd qmux
cargo install --path .
```

## Usage

```sh
qmux
```

- If you are **outside** tmux:
  - with existing tmux sessions: opens a new `qmux` window in the first session and attaches.
  - with no tmux server: creates a new `qmux` session.
- If you are **inside** tmux:
  - in controller pane: launches the TUI.
  - in a plain pane: takes over that pane as the controller.
  - in an agent pane: exits with an error (to prevent nested agent/controller confusion).

### Update

```sh
qmux update
```

Install a specific release tag:

```sh
qmux update --version v0.1.0
```

Optional flags:

- `--bin-dir <path>`: install directory override (default: current `qmux` binary directory)
- `--no-verify`: skip checksum verification (not recommended)

### Keys (controller TUI)

| Key | Action |
|---|---|
| `i` | Enter insert mode (edit prompt) |
| `s` | Spawn a new agent |
| `a` | Adopt an existing pane as an agent |
| `space` | Toggle selection on focused agent |
| `Enter` | Send current input to selected agents |
| `f` | Forward captured output to another agent |
| `x` | Kill focused agent (with confirmation) |
| `L` | Cycle layout (`main-horizontal-mirrored` → `main-horizontal` → `main-vertical` → `main-vertical-mirrored`) |
| `l` | Re-apply current layout (no cycle) |
| `R` | Refresh agent list |
| `J` / `K` | Move focused agent down / up (swap panes) |
| `q` | Quit controller (agents keep running) |
| `Q` | Quit + kill all agent panes |
| `?` | Help |

Internal planning/design docs are kept out of the repository.

## Configuration

Create `~/.config/oneqit/qmux/config.yaml` to add or override presets:

```yaml
presets:
  claude:
    display_name: "Claude (fast)"
    binary: "claude"
    launch_cmd: "claude --fast"
  myagent:
    display_name: "My Agent"
    binary: "myagent"
    launch_cmd: "myagent"
    # Optional: improve Forward extraction for non-built-in CLIs
    response_start_marker: "▶"
    response_end_marker: "$"
    post_cutoff_markers:
      - "Footer line to strip"
      - "Another cutoff pattern"
```

User presets override built-ins with the same key.
If `response_start_marker` is omitted, Forward falls back to the full captured buffer.
If `response_end_marker` is omitted, `❯` is used.

## Development

```sh
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## License

MIT — see [LICENSE](LICENSE).
