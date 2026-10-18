//! Independent preference receipt; no credentials or reusable permit.
use super::*;
use super_platinum_core::slack_lists::ListDefaultViewPreference;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::slack_lists::rename) enum Preference {
    NotRequested,
    InFlight,
    Unconfirmed,
    Observed,
}
impl Preference {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotRequested => "Default-view preference was not sent.",
            Self::InFlight => "Saving your default-view preference…",
            Self::Unconfirmed => "Your default-view preference is unconfirmed.",
            Self::Observed => "Your current default-view preference matches this Save.",
        }
    }
}
#[derive(Clone)]
pub(in crate::slack_lists::rename) struct Receipt {
    pub token: u64,
    pub dismissed_on_ack: bool,
    pub preference: Preference,
    pub intent: Option<ListDefaultViewPreference>,
    pub custody: Option<Custody>,
    pub acknowledged_id: Option<String>,
}
#[derive(Clone)]
pub(in crate::slack_lists::rename) struct Custody {
    review: owner::Review,
    state: refinement::State,
    provider: Option<BoardLaneOrderDraft>,
    layout: u64,
    pending: Option<String>,
    find: (String, String, u64),
}
impl Custody {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let view = shell.slack_lists.view.as_ref()?;
        Some(Self {
            review: owner::Review::capture(shell)?,
            state: view.layout.refinements.clone(),
            provider: view.layout.board_order.clone(),
            layout: view.layout.generation,
            pending: view.layout.pending_refinement.clone(),
            find: (
                view.layout.find.input.clone(),
                view.layout.find.applied.clone(),
                view.layout.find.revision,
            ),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.review.navigation_current(shell)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.layout.refinements == self.state
                    && v.layout.board_order == self.provider
                    && v.layout.generation == self.layout
                    && v.layout.pending_refinement == self.pending
                    && (
                        v.layout.find.input.as_str(),
                        v.layout.find.applied.as_str(),
                        v.layout.find.revision,
                    ) == (self.find.0.as_str(), self.find.1.as_str(), self.find.2)
            })
    }
}
