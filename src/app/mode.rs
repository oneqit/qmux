#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Insert,
    QuitConfirm { kill_all: bool },
    Help,
    Spawn(SpawnState),
    KillConfirm { pane_id: String, name: String },
    Forward(Box<ForwardState>),
    Adopt(AdoptState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardPhase {
    PickSource {
        cursor: usize,
    },
    EditPreview,
    PickTargets {
        cursor: usize,
        selected: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ForwardState {
    pub phase: ForwardPhase,
    pub source_pane_id: Option<String>,
    pub source_name: Option<String>,
    pub preview: tui_textarea::TextArea<'static>,
    pub preview_viewport_top: (u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnState {
    pub presets: Vec<(String, crate::agent::Preset)>,
    pub cursor: usize,
    pub selected: Vec<usize>,
    pub name_input: String,
    pub naming: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptPhase {
    PickPane {
        cursor: usize,
    },
    PickPreset {
        cursor: usize,
        target_pane_id: String,
    },
    Naming {
        target_pane_id: String,
        preset_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptState {
    pub panes: Vec<crate::tmux::AdoptablePane>,
    pub presets: Vec<(String, crate::agent::Preset)>,
    pub phase: AdoptPhase,
    pub name_input: String,
}
