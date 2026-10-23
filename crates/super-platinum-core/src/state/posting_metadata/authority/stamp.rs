use super::*;

/// An invalidation witness for changes adopted through existing typed channel
/// APIs (genericInfo, details, membership, etc). It can only reject older raw
/// evidence. Its default-filled fields are NEVER inputs to posting selectors.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Stamp {
    lifecycle: u64,
    metadata: Option<Value>,
}
impl Stamp {
    pub(super) fn with_channel(&self, channel: &crate::slack::models::Channel) -> Self {
        Self {
            lifecycle: self.lifecycle,
            metadata: Some(metadata(channel)),
        }
    }
    pub(super) fn capture(core: &CoreAppState, team: &str, channel: &str) -> Self {
        let workspace = core.workspaces.get(team);
        Self {
            lifecycle: workspace
                .map(|w| w.channel_lifecycle.channel_revision(channel))
                .unwrap_or(0),
            metadata: workspace
                .and_then(|w| w.channels.get(channel))
                .map(metadata),
        }
    }
}
fn metadata(c: &crate::slack::models::Channel) -> Value {
    serde_json::json!({
        "is_channel":c.is_channel,"is_group":c.is_group,"is_im":c.is_im,"is_mpim":c.is_mpim,
        "is_private":c.is_private,"is_archived":c.is_archived,"is_ext_shared":c.is_ext_shared,
        "updated":c.updated,"user":c.user,"extra":c.extra
    })
}
