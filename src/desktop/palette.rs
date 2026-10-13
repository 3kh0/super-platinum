//! Palette projections retain person targets even before a DM exists.
use crate::state::{ChannelVm, Overlay, ShellState};
use super_platinum_core::palette::PaletteTarget;

#[derive(Clone)]
pub(crate) struct PaletteMatch {
    pub target: PaletteTarget,
    pub row: ChannelVm,
    pub handle: String,
}

impl ShellState {
    pub fn open_palette(&mut self) {
        self.shortcut_generation = self.shortcut_generation.wrapping_add(1);
        self.cancel_profile_dm();
        self.cancel_palette_open();
        self.overlay = Some(Overlay::Palette);
        self.palette_selected = 0;
        self.profile_hover = None;
    }
    pub fn cancel_palette_open(&mut self) {
        self.palette_generation = self.palette_generation.wrapping_add(1);
        self.palette_opening = None;
    }
    pub fn set_palette_query(&mut self, query: String) {
        if self.palette_query != query {
            self.cancel_palette_open();
            self.palette_query = query;
        }
        self.palette_selected = 0;
    }
    pub(crate) fn palette_matches(&self) -> Vec<PaletteMatch> {
        let workspace = self
            .core
            .active_team
            .as_ref()
            .and_then(|team| self.core.workspaces.get(team));
        if let Some(workspace) = workspace {
            let entries = super_platinum_core::palette::rank(
                workspace,
                &self.palette_query,
                &Default::default(),
            );
            let hits = entries
                .into_iter()
                .filter_map(|entry| {
                    let mut row = match &entry.target {
                        PaletteTarget::Channel(id) => {
                            self.channels.iter().find(|c| &c.id == id)?.clone()
                        }
                        PaletteTarget::User { user, dm } => {
                            self.channels
                                .iter()
                                .find(|c| {
                                    dm.as_ref() == Some(&c.id)
                                        || (c.is_im && c.user_id.as_ref() == Some(user))
                                })
                                .cloned()
                                .unwrap_or_else(|| {
                                    crate::channel_vm::channel_vm(
                                        workspace,
                                        &super_platinum_core::slack::models::Channel {
                                            // Projection only; never inserted into workspace state or sent to Slack.
                                            id: format!("palette-user-{user}"),
                                            is_im: true,
                                            user: Some(user.clone()),
                                            ..Default::default()
                                        },
                                        &self.media,
                                    )
                                })
                        }
                    };
                    if row.is_im && row.user_id.as_deref() == Some(workspace.self_user_id.as_str())
                    {
                        row.name.push_str(" (you)");
                    }
                    Some(PaletteMatch {
                        target: entry.target,
                        row,
                        handle: entry.sublabel,
                    })
                })
                .collect::<Vec<_>>();
            if !hits.is_empty() || self.palette_query.trim().is_empty() {
                return hits;
            }
        }
        let query = self.palette_query.trim().to_lowercase();
        self.channels
            .iter()
            .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
            .take(12)
            .map(|row| PaletteMatch {
                target: PaletteTarget::Channel(row.id.clone()),
                row: row.clone(),
                handle: String::new(),
            })
            .collect()
    }
    pub fn move_palette(&mut self, delta: isize) {
        let count = self.palette_matches().len();
        self.palette_selected = if count == 0 {
            0
        } else {
            (self.palette_selected as isize + delta).rem_euclid(count as isize) as usize
        };
    }
}
