use super::*;
use crate::thread_subscriptions::Observation;
#[derive(PartialEq)]
pub(super) struct Before {
    transcript: Option<Value>,
    threads: BTreeMap<ThreadKey, Value>,
    subscriptions: Observation,
}
impl Before {
    pub(super) fn capture(core: &CoreAppState, team: &str, channel: &str) -> Result<Self, Error> {
        let mut count = 0;
        let mut bytes = 0;
        let mut bag = |bag: &ChannelMessages| -> Result<Value, Error> {
            count += bag.messages.len() + bag.remote_deletions.len() + bag.pending.len();
            if count > 20_000 {
                return Err(unavailable());
            }
            // No partial/truncated witness. A bounded capture refuses in full.
            let deleted = bag.remote_deletions.iter().collect::<BTreeSet<_>>();
            let value = json!({"messages":bag.messages,"deletions":deleted,"pending":bag.pending,"read":bag.last_read,"revision":bag.read_revision});
            bytes += serde_json::to_vec(&value).map_err(|_| unavailable())?.len();
            if bytes > 16 * 1024 * 1024 {
                return Err(unavailable());
            }
            Ok(value)
        };
        let w = core.workspaces.get(team).ok_or_else(unavailable)?;
        let transcript = w.messages.get(channel).map(&mut bag).transpose()?;
        let mut threads = BTreeMap::new();
        for (key, value) in &core.threads {
            if key.0 == team && key.1 == channel {
                if threads.len() >= 1000 {
                    return Err(unavailable());
                }
                threads.insert(key.clone(), bag(value)?);
            }
        }
        let subscriptions = core
            .thread_subscriptions
            .observe(core.account_epoch, team, channel)
            .ok_or_else(unavailable)?;
        Ok(Self {
            transcript,
            threads,
            subscriptions,
        })
    }
}
