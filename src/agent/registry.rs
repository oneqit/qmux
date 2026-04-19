use crate::app::AgentRow;
use crate::tmux::{list_panes_in_window, PaneRow, TmuxRunner};
use anyhow::Result;

pub fn fetch_agents(tmux: &dyn TmuxRunner, window_id: &str) -> Result<Vec<AgentRow>> {
    let mut rows: Vec<PaneRow> = list_panes_in_window(tmux, window_id)?
        .into_iter()
        .filter(|r| r.role == "agent")
        .collect();
    // Reading order: top-to-bottom first, then left-to-right within the same row.
    // This way the list matches the visual pane order regardless of layout.
    rows.sort_by(|a, b| {
        a.pane_top
            .cmp(&b.pane_top)
            .then_with(|| a.pane_left.cmp(&b.pane_left))
    });
    Ok(rows
        .into_iter()
        .map(|r| AgentRow {
            pane_id: r.pane_id,
            name: r.name,
            preset: r.preset,
            selected: false,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::mock::MockTmux;

    #[test]
    fn filters_to_agent_role_and_sorts_by_visual_position() {
        // Controller at bottom (top=20), two agents in top row (top=0) left-to-right.
        // Alphabetically bob<alice would flip, but visual order is bob (left=0) then
        // alice (left=40).
        let raw = "\
%0|controller|||20|0\n\
%2|agent|alice|claude|0|40\n\
%1|agent|bob|codex|0|0\n";
        let m = MockTmux::new().with_ok(raw);
        let agents = fetch_agents(&m, "@1").unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "bob", "leftmost pane first");
        assert_eq!(agents[1].name, "alice");
    }

    #[test]
    fn sorts_stacked_agents_top_to_bottom() {
        // Vertical layout: controller on the left (top=0, left=0),
        // agents stacked on the right column (same left, different tops).
        let raw = "\
%0|controller|||0|0\n\
%2|agent|bottom|claude|10|40\n\
%1|agent|top|codex|0|40\n";
        let m = MockTmux::new().with_ok(raw);
        let agents = fetch_agents(&m, "@1").unwrap();
        assert_eq!(agents[0].name, "top");
        assert_eq!(agents[1].name, "bottom");
    }
}
