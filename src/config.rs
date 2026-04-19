use crate::agent::Preset;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
}

pub fn default_config_path() -> Option<PathBuf> {
    // $HOME/.config/oneqit/qmux/config.yaml (on all platforms via `dirs::home_dir`,
    // we hard-code the `.config/oneqit` layout because that is where this tool lives).
    let home = dirs::home_dir()?;
    Some(
        home.join(".config")
            .join("oneqit")
            .join("qmux")
            .join("config.yaml"),
    )
}

pub fn load_user_config(path: &Path) -> Result<UserConfig> {
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: UserConfig =
        serde_yaml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(cfg)
}

pub fn merged_presets(user: &UserConfig) -> HashMap<String, Preset> {
    let mut merged = crate::agent::builtin_presets();
    for (k, v) in &user.presets {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_default() {
        let path = PathBuf::from("/definitely/nonexistent/qmux-config.yaml");
        let cfg = load_user_config(&path).unwrap();
        assert!(cfg.presets.is_empty());
    }

    #[test]
    fn malformed_yaml_returns_err() {
        let tmp = std::env::temp_dir().join("qmux-bad.yaml");
        std::fs::write(&tmp, "presets: [not a map]").unwrap();
        let err = load_user_config(&tmp).unwrap_err();
        assert!(err.to_string().contains("parsing"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn user_preset_overrides_builtin() {
        let mut user = UserConfig::default();
        user.presets.insert(
            "claude".to_string(),
            Preset {
                display_name: "Claude (custom)".into(),
                binary: "myclaude".into(),
                launch_cmd: "myclaude --fast".into(),
                response_start_marker: None,
                response_end_marker: None,
                post_cutoff_markers: Vec::new(),
            },
        );
        let m = merged_presets(&user);
        let c = m.get("claude").unwrap();
        assert_eq!(c.binary, "myclaude");
        assert_eq!(c.launch_cmd, "myclaude --fast");
    }

    #[test]
    fn user_can_add_new_preset() {
        let mut user = UserConfig::default();
        user.presets.insert(
            "aider".to_string(),
            Preset {
                display_name: "Aider".into(),
                binary: "aider".into(),
                launch_cmd: "aider".into(),
                response_start_marker: None,
                response_end_marker: None,
                post_cutoff_markers: Vec::new(),
            },
        );
        let m = merged_presets(&user);
        assert_eq!(m.len(), 5);
        assert!(m.contains_key("aider"));
    }
}
