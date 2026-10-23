//! A pre-authorized typed publication may change only its own metadata stamp.
//! The host prepares and applies this inside one serial canonical commit, never
//! by recapturing request authority after an await.
use super::*;
use crate::slack::models::Channel;

pub struct Publication {
    read: ChannelRead,
    raw: Value,
}
impl ChannelRead {
    pub fn prepare_publication(
        mut self,
        core: &CoreAppState,
        raw: Value,
        typed: &Channel,
    ) -> Result<Publication> {
        if !self.current(core)
            || !raw.is_object()
            || raw["id"].as_str() != Some(self.channel.as_str())
            || typed.id != self.channel
        {
            return Err(Unknown::Stale);
        }
        // Derive the one allowed after-stamp BEFORE installing the staged typed
        // channel. Lifecycle remains the original read's lifecycle.
        self.stamp = self.stamp.with_channel(typed);
        Ok(Publication { read: self, raw })
    }
}
impl Publication {
    pub fn apply(self, core: &mut CoreAppState) -> Result<RawChannel> {
        // apply checks exact account/config/reservation/lifecycle and the typed
        // after-stamp. An unexpected mutation cannot become an observation.
        self.read.apply(core, self.raw)
    }
}
