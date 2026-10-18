//! Captured credentials are handed directly to the closed core discovery path.
use super::*;
use super_platinum_core::drafts::store::protected::{self as p, owned_store as o};
impl Binding {
    pub(crate) async fn discover_slash(
        &self,
        ops: &o::Operations,
        stream: p::Stream,
        scope: super_platinum_core::drafts::sync::Scope,
    ) -> Result<o::SlashContext, o::SlashDiscoveryError> {
        if self.account != ops.account()
            || self.account != scope.account_id
            || self.workspace.team_id != scope.key.team
        {
            return Err(o::SlashDiscoveryError::Stale);
        }
        // The reversible protected local Snapshot contains text/style runs only;
        // no embedded emoji lookup is claimed or synthesized by this adapter.
        ops.discover_slash(
            stream,
            scope,
            self.workspace.clone(),
            self.client.clone(),
            self.transport.clone(),
            Arc::new(serde_json::json!({})),
        )
        .await
    }
}
