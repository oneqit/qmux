use crate::agent::Preset;
use std::collections::HashMap;

/// Trait so tests can inject a fake binary lookup.
pub trait BinaryFinder {
    fn exists(&self, binary: &str) -> bool;
}

pub struct WhichFinder;

impl BinaryFinder for WhichFinder {
    fn exists(&self, binary: &str) -> bool {
        which::which(binary).is_ok()
    }
}

pub fn available_presets(
    finder: &dyn BinaryFinder,
    all: &HashMap<String, Preset>,
) -> Vec<(String, Preset)> {
    let mut v: Vec<(String, Preset)> = all
        .iter()
        .filter(|(_, p)| finder.exists(&p.binary))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct FakeFinder {
        installed: HashSet<String>,
    }

    impl BinaryFinder for FakeFinder {
        fn exists(&self, binary: &str) -> bool {
            self.installed.contains(binary)
        }
    }

    #[test]
    fn only_installed_are_returned() {
        let finder = FakeFinder {
            installed: ["claude", "codex"].iter().map(|s| s.to_string()).collect(),
        };
        let all = crate::agent::builtin_presets();
        let av = available_presets(&finder, &all);
        let keys: Vec<&str> = av.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["claude", "codex"]);
    }

    #[test]
    fn empty_when_nothing_installed() {
        let finder = FakeFinder {
            installed: HashSet::new(),
        };
        let all = crate::agent::builtin_presets();
        let av = available_presets(&finder, &all);
        assert!(av.is_empty());
    }

    #[test]
    fn results_are_sorted_by_key() {
        let finder = FakeFinder {
            installed: ["claude", "gemini", "codex", "copilot"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        let all = crate::agent::builtin_presets();
        let av = available_presets(&finder, &all);
        let keys: Vec<&str> = av.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["claude", "codex", "copilot", "gemini"]);
    }
}
