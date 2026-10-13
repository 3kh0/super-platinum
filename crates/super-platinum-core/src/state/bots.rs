//! Per-workspace bot metadata lookup bookkeeping. Account/team/transport guards
//! belong to the caller; tickets guard request replacement and local invalidation.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::slack::{api, models::BotProfile};

// Local resource/backoff policy, not claimed Slack protocol limits.
pub const MAX_IN_FLIGHT: usize = 8;
pub const MAX_NEGATIVE: usize = 64;
pub const MAX_PROFILES: usize = 256;
pub const REQUEST_LIFETIME: Duration = Duration::from_secs(60);
pub const NEGATIVE_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    bot_id: String,
    app_id_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotLookupTicket {
    key: Key,
    generation: u64,
    request: u64,
    explicit: bool,
}

impl BotLookupTicket {
    pub fn bot_id(&self) -> &str {
        &self.key.bot_id
    }
    pub fn app_id_hint(&self) -> Option<&str> {
        self.key.app_id_hint.as_deref()
    }
}

#[derive(Clone)]
struct Profile {
    value: BotProfile,
    order: u64,
}

#[derive(Debug, Clone)]
struct Pending {
    request: u64,
    expires: Duration,
}

#[derive(Clone, Default)]
pub struct BotLookupState {
    generation: u64,
    sequence: u64,
    profiles: BTreeMap<String, Profile>,
    pending: BTreeMap<Key, Pending>,
    negative: BTreeMap<Key, Duration>,
    overflow_until: Duration,
}

impl std::fmt::Debug for BotLookupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BotLookupState")
            .field("profiles", &self.profiles.len())
            .field("pending", &self.pending.len())
            .field("negative", &self.negative.len())
            .finish()
    }
}

impl BotLookupState {
    pub fn is_pending(&self, bot_id: &str, app_id_hint: Option<&str>, now: Duration) -> bool {
        self.pending
            .get(&Key {
                bot_id: bot_id.into(),
                app_id_hint: app_id_hint.map(str::to_owned),
            })
            .is_some_and(|pending| now < pending.expires)
    }

    /// An incomplete returned profile remains incomplete. The resolver decides
    /// whether it supplies enough canonical identity for an action.
    pub fn get(&self, bot_id: &str, app_id_hint: Option<&str>) -> Option<&BotProfile> {
        let profile = &self.profiles.get(bot_id)?.value;
        if app_id_hint.is_some_and(|hint| {
            profile
                .extra
                .get("app_id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|app| app != hint)
        }) {
            return None;
        }
        Some(profile)
    }

    /// Automatic missing-only lookup. Existing records, including incomplete or
    /// app-hint-conflicting records, require explicit retry. Admission reserves
    /// capacity so completion-driven rendering cannot cycle through evictions.
    /// `now` is elapsed monotonic time with one origin for this state's lifetime.
    pub fn begin(
        &mut self,
        bot_id: &str,
        app_id_hint: Option<&str>,
        now: Duration,
    ) -> Option<BotLookupTicket> {
        self.begin_inner(bot_id, app_id_hint, now, false)
    }

    /// Explicit user-driven retry/refresh. It may replace a cached record or
    /// evict the oldest positive entry, but still respects failure cooldowns.
    pub fn begin_retry(
        &mut self,
        bot_id: &str,
        app_id_hint: Option<&str>,
        now: Duration,
    ) -> Option<BotLookupTicket> {
        self.begin_inner(bot_id, app_id_hint, now, true)
    }

    fn begin_inner(
        &mut self,
        bot_id: &str,
        app_id_hint: Option<&str>,
        now: Duration,
        explicit: bool,
    ) -> Option<BotLookupTicket> {
        if special(bot_id) || !valid_request(bot_id, app_id_hint) {
            return None;
        }
        self.expire(now);
        if now < self.overflow_until
            || (!explicit
                && (self.profiles.contains_key(bot_id)
                    || self.profiles.len() + self.pending.len() >= MAX_PROFILES))
        {
            return None;
        }
        let key = Key {
            bot_id: bot_id.into(),
            app_id_hint: app_id_hint.map(str::to_owned),
        };
        if self.pending.contains_key(&key)
            || self.negative.contains_key(&key)
            || self.pending.len() >= MAX_IN_FLIGHT
        {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        let request = self.sequence;
        self.pending.insert(
            key.clone(),
            Pending {
                request,
                expires: now.saturating_add(REQUEST_LIFETIME),
            },
        );
        Some(BotLookupTicket {
            key,
            generation: self.generation,
            request,
            explicit,
        })
    }

    /// Returns false for stale/expired/malformed responses or an automatic
    /// completion whose reserved capacity was taken by explicit retry. Invalid
    /// responses release the current request into cooldown without caching data.
    pub fn complete(
        &mut self,
        ticket: &BotLookupTicket,
        profile: Option<BotProfile>,
        now: Duration,
    ) -> bool {
        self.expire(now);
        if !self.current(ticket) {
            return false;
        }
        let value = serde_json::json!({"ok":true,"bots":profile.into_iter().collect::<Vec<_>>()});
        let decoded = api::decode_bots_info(&value, ticket.bot_id(), ticket.app_id_hint());
        self.pending.remove(&ticket.key);
        let profile = match decoded {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                self.cooldown(ticket.key.clone(), now, NEGATIVE_COOLDOWN);
                return true;
            }
            Err(_) => {
                self.cooldown(ticket.key.clone(), now, NEGATIVE_COOLDOWN);
                return false;
            }
        };
        self.negative.remove(&ticket.key);
        if let Some(existing) = self.profiles.get(ticket.bot_id()) {
            // Captured Tzza keeps the newer existing record only when both
            // timestamps are present. Non-integral/negative values are unknown.
            if updated(&existing.value)
                .zip(updated(&profile))
                .is_some_and(|(old, new)| old > new)
            {
                return true;
            }
            // Without a comparable timestamp, a request issued before this
            // record's successful request cannot overwrite the newer result.
            if updated(&existing.value).zip(updated(&profile)).is_none()
                && existing.order > ticket.request
            {
                return true;
            }
        }
        if !self.profiles.contains_key(ticket.bot_id()) && self.profiles.len() >= MAX_PROFILES {
            // An explicit retry may have filled a slot reserved by this read.
            // Never let an automatic completion evict and restart the sweep.
            if !ticket.explicit {
                self.cooldown(ticket.key.clone(), now, NEGATIVE_COOLDOWN);
                return false;
            }
            if let Some(key) = self
                .profiles
                .iter()
                .min_by_key(|(_, value)| value.order)
                .map(|(key, _)| key.clone())
            {
                self.profiles.remove(&key);
            }
        }
        self.profiles.insert(
            ticket.bot_id().into(),
            Profile {
                value: profile,
                order: ticket.request,
            },
        );
        true
    }

    /// Back off a failed lookup; honor a longer Retry-After supplied by transport.
    pub fn fail(&mut self, ticket: &BotLookupTicket, now: Duration, retry_after: Duration) -> bool {
        self.expire(now);
        if !self.current(ticket) {
            return false;
        }
        self.pending.remove(&ticket.key);
        self.cooldown(ticket.key.clone(), now, retry_after.max(NEGATIVE_COOLDOWN));
        true
    }

    /// Drop profiles and invalidate every ticket, including same-key requests
    /// begun later. Request sequence numbers are never reused across clears.
    pub fn clear(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.profiles.clear();
        self.pending.clear();
        self.negative.clear();
        self.overflow_until = Duration::ZERO;
    }

    fn current(&self, ticket: &BotLookupTicket) -> bool {
        ticket.generation == self.generation
            && self
                .pending
                .get(&ticket.key)
                .is_some_and(|pending| pending.request == ticket.request)
    }

    fn expire(&mut self, now: Duration) {
        self.negative.retain(|_, expires| now < *expires);
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, pending)| now >= pending.expires)
            .map(|(key, pending)| (key.clone(), pending.expires))
            .collect();
        for (key, deadline) in expired {
            self.pending.remove(&key);
            let until = deadline.saturating_add(NEGATIVE_COOLDOWN);
            if now < until {
                self.cooldown(key, now, until.saturating_sub(now));
            }
        }
    }

    fn cooldown(&mut self, key: Key, now: Duration, delay: Duration) {
        let until = now.saturating_add(delay);
        if !self.negative.contains_key(&key) && self.negative.len() >= MAX_NEGATIVE {
            // No unbounded tombstones and no negative eviction/refetch cycle.
            // One scalar covers results that cannot fit in the bounded map.
            self.overflow_until = self.overflow_until.max(until);
            return;
        }
        self.negative.insert(key, until);
    }
}

fn updated(profile: &BotProfile) -> Option<u64> {
    profile.extra.get("updated")?.as_u64()
}

fn valid_request(bot: &str, app: Option<&str>) -> bool {
    // Reuse the decoder's request-identity validation without credentials or I/O.
    api::decode_bots_info(&serde_json::json!({"ok":true,"bots":[]}), bot, app).is_ok()
}

fn special(bot: &str) -> bool {
    matches!(
        bot,
        "B01" | "BSLACKBOT" | "B03" | "BSLACKSECURITY" | "BCHANNELEXPERT"
    )
}

#[cfg(test)]
mod tests;
