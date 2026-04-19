use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Preset {
    pub display_name: String,
    pub binary: String,
    pub launch_cmd: String,
    /// String that marks the start of an assistant response line in this
    /// agent's console output. Matched with `trim_start().starts_with(..)`.
    /// `None` disables smart extraction — the Forward flow falls back to the
    /// full captured buffer.
    #[serde(default)]
    pub response_start_marker: Option<String>,
    /// String that marks the shell prompt line that ends a response. Matched
    /// with `trim_start().starts_with(..)`. `None` defaults to `❯`.
    #[serde(default)]
    pub response_end_marker: Option<String>,
    /// Post-processing cutoff patterns. Applied AFTER the primary start/end
    /// bounding: within the extracted window, the first line *containing*
    /// any of these strings becomes the cutoff — that line and everything
    /// below it are dropped. Use this to strip agent UI chrome that sits
    /// between the response body and the shell prompt (e.g. copilot's git
    /// prompt line, gemini's update banner or footer hint). Matched with
    /// `contains`, not starts_with.
    #[serde(default)]
    pub post_cutoff_markers: Vec<String>,
}

impl Preset {
    /// Effective end marker used by the Forward extractor. Defaults to `❯`
    /// when the preset does not specify one — that covers claude / codex /
    /// copilot without needing to restate it in user YAML.
    pub fn effective_end_marker(&self) -> String {
        self.response_end_marker
            .clone()
            .unwrap_or_else(|| "\u{276F}".into())
    }
}

pub fn builtin_presets() -> HashMap<String, Preset> {
    let entries = [
        (
            "claude",
            Preset {
                display_name: "Claude Code".into(),
                binary: "claude".into(),
                launch_cmd: "claude".into(),
                response_start_marker: Some("\u{23FA}".into()), // ⏺
                response_end_marker: Some("\u{276F}".into()),   // ❯
                post_cutoff_markers: Vec::new(),
            },
        ),
        (
            "codex",
            Preset {
                display_name: "OpenAI Codex".into(),
                binary: "codex".into(),
                launch_cmd: "codex".into(),
                response_start_marker: Some("\u{2022}".into()), // •
                response_end_marker: Some("\u{203A}".into()),   // ›
                post_cutoff_markers: Vec::new(),
            },
        ),
        (
            "gemini",
            Preset {
                display_name: "Gemini CLI".into(),
                binary: "gemini".into(),
                launch_cmd: "gemini".into(),
                response_start_marker: Some("\u{2726}".into()), // ✦
                response_end_marker: Some(">".into()),
                post_cutoff_markers: vec![
                    "\u{2139}".into(),  // ℹ  — update/info banner
                    "Shift+Tab".into(), // footer hint line
                ],
            },
        ),
        (
            "copilot",
            Preset {
                display_name: "GitHub Copilot CLI".into(),
                binary: "copilot".into(),
                launch_cmd: "copilot".into(),
                response_start_marker: Some("\u{25CF}".into()), // ●
                response_end_marker: Some("\u{276F}".into()),   // ❯
                post_cutoff_markers: vec![
                    "\u{2387}".into(),  // ⎇  — git prompt branch glyph (idle state)
                    "\u{25CB} ".into(), // ○  — action indicator (Asking user / Running / ...)
                ],
            },
        ),
    ];
    entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_contains_four_majors() {
        let m = builtin_presets();
        assert_eq!(m.len(), 4);
        for k in ["claude", "codex", "gemini", "copilot"] {
            assert!(m.contains_key(k), "missing preset {}", k);
        }
    }

    #[test]
    fn copilot_launch_cmd_is_copilot() {
        let m = builtin_presets();
        assert_eq!(m.get("copilot").unwrap().launch_cmd, "copilot");
    }

    #[test]
    fn preset_round_trips_from_yaml() {
        let yaml = r#"
display_name: "Custom Agent"
binary: "myagent"
launch_cmd: "myagent --repl"
"#;
        let p: Preset = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.display_name, "Custom Agent");
        assert_eq!(p.binary, "myagent");
        assert_eq!(p.launch_cmd, "myagent --repl");
        assert_eq!(p.response_start_marker, None);
        assert_eq!(p.response_end_marker, None);
        assert!(p.post_cutoff_markers.is_empty());
        // Default end marker for a preset that declares no override.
        assert_eq!(p.effective_end_marker(), "\u{276F}");
    }

    #[test]
    fn preset_can_declare_response_markers_in_yaml() {
        let yaml = r#"
display_name: "Custom"
binary: "c"
launch_cmd: "c"
response_start_marker: "▶"
response_end_marker: "$"
"#;
        let p: Preset = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.response_start_marker.as_deref(), Some("\u{25B6}"));
        assert_eq!(p.effective_end_marker(), "$");
    }
}
