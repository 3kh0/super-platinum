//! Complete acknowledged Slash only; any outstanding fresh witness keeps custody.
use super::super::family_retirement::{finish, keys, transferred};
use super::*;
impl Worker {
    pub(in crate::drafts::store::worker) fn observe_slash_family(
        &self,
        c: &SlashCompletion,
        retire: bool,
    ) -> Result<crate::slash_commands::controller::Record, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let keys = keys(
            &pending,
            &c.key,
            &["slash-reserve", "slash-claim", "slash-complete"],
        )?;
        self.exact_slash(&pending, c)?;
        let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Ready
            || !s.handed
            || s.building
            || s.disconnected
            || !s.failures.is_empty()
            || s.claim.is_some()
            || s.request.is_some()
            || s.fresh_outstanding
        {
            return Err(Reason::Incomplete);
        }
        let settlement = s
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(Reason::Incomplete)?;
        if settlement.has_fresh_clear()
            || settlement.pending()
            || !matches!(
                settlement.record().phase(),
                crate::slash_commands::controller::Phase::Completed(
                    crate::slash_commands::api::Reply::Acknowledged { .. }
                )
            )
        {
            return Err(Reason::Incomplete);
        }
        let record = settlement.record().clone();
        let ids = [
            transferred::<Arc<SlashInput>, ()>(&pending, &keys[0], &keys[1])?,
            transferred::<Arc<SlashInput>, p::slash_runtime::QueuedClaim>(
                &pending, &keys[1], &keys[2],
            )?,
            s.submitted.ok_or(Reason::Incomplete)?,
        ];
        finish(&mut pending, &keys, &ids, retire)?;
        Ok(record)
    }
}
