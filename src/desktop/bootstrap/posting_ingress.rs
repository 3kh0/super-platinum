//! Positive external routing history from ordinary accepted channel responses.
//! This hook does not supply default model fields to posting permissions.
use crate::state::ShellState;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::collections::BTreeSet;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Transport, models::Channel},
    state::posting_metadata::authority::Authority,
};

pub(crate) struct Ingress {
    authority: Option<Authority>,
    team: String,
    requested: Option<BTreeSet<String>>,
}
pub(crate) fn capture(
    state: &Signal<ShellState>,
    transport: &Transport,
    session: &WorkspaceSession,
) -> Ingress {
    Ingress {
        authority: Authority::capture_for_request(&state.peek().core, transport, session).ok(),
        team: session.team_id.clone(),
        requested: None,
    }
}
impl Ingress {
    /// Snapshot the exact requested IDs before await for a known-target read.
    /// Catalog/search/creation requests keep their separate response contract.
    pub(crate) fn for_channels(mut self, ids: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.requested = Some(ids.into_iter().map(|id| id.as_ref().to_owned()).collect());
        self
    }

    fn accepts(&self, channels: &[Channel]) -> bool {
        self.requested
            .as_ref()
            .is_none_or(|ids| channels.iter().all(|c| ids.contains(&c.id)))
    }

    /// Call only inside the response's existing account/query acceptance branch,
    /// at its typed metadata adoption. No Signal borrow is held across await.
    pub(crate) fn channels(&self, state: &Signal<ShellState>, channels: &[Channel]) {
        if !self.accepts(channels) {
            return;
        }
        let Some(authority) = &self.authority else {
            return;
        };
        if !channels.iter().any(|c| {
            c.extra.contains_key("connected_team_ids")
                || c.extra.contains_key("connected_limited_team_ids")
        }) {
            return;
        }
        let accepted = {
            let mut state = *state;
            let mut shell = state.write();
            authority
                .observe_external_channel_connections(&mut shell.core, channels)
                .is_ok()
        };
        if accepted {
            super::persist_workspace_metadata(state, &self.team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_ids_are_owned_and_foreign_rows_reject_the_whole_history_batch() {
        let mut ids = vec!["C1".to_owned(), "C2".to_owned()];
        let ingress = Ingress {
            authority: None,
            team: "T1".into(),
            requested: None,
        }
        .for_channels(&ids);
        ids.push("C9".into());
        let channel = |id: &str| Channel {
            id: id.into(),
            ..Default::default()
        };
        assert!(ingress.accepts(&[]));
        assert!(ingress.accepts(&[channel("C2")]));
        assert!(ingress.accepts(&[channel("C1"), channel("C2")]));
        assert!(!ingress.accepts(&[channel("C1"), channel("C9")]));
        assert!(!ingress.accepts(&[channel("")]));
        let detail = Ingress {
            authority: None,
            team: "T1".into(),
            requested: None,
        }
        .for_channels(["C1"]);
        assert!(detail.accepts(&[channel("C1")]));
        assert!(!detail.accepts(&[channel("C2")]));
    }
}
