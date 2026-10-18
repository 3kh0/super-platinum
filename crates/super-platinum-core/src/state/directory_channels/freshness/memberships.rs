//! Same eH action as Unknown recovery, under a live conditional batch capability.
use super::super::{Commit, Observation as ChannelObservation, Store};
use super::*;
use crate::slack::models::Channel;
use std::collections::BTreeMap;
impl Manager {
    /// Closed eq -> eV -> eH true-private response path. This method grants no
    /// network or response authority: caller retains original IO/RT lease and
    /// validates returned Page membership IDs before passing them here.
    #[allow(clippy::too_many_arguments)]
    pub fn joined_memberships(
        &self,
        scope: &Scope,
        batch: &Batch,
        store: &Store,
        observed: &ChannelObservation,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        joined: &[String],
    ) -> Result<Commit> {
        if !self.batch_current(scope, batch)
            || !observed.current(
                store,
                &scope.identity.account,
                &scope.session,
                scope.account_epoch,
                generation,
                native,
            )
            || joined.iter().any(|id| !batch.ids.contains(id))
        {
            return Err(Error::MissingObservation);
        }
        super::super::joined::apply_joined(store, native, joined)
    }
}
