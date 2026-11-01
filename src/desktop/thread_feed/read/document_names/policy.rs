//! A bounded native DATA adapter, not Source's global store/Init assignment.
use super::*;
use crate::app_home::autocomplete::observations::Store;
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Policy {
    pub(super) store: Store,
}
impl Policy {
    fn bound(source: &Source, s: &ShellState) -> Option<Store> {
        let store = Store::capture(s)?;
        if !store.matches_reader(s, &source.client, &source.transport, &source.session) {
            return None;
        }
        let names = &store.complete().names;
        let raw = &names.user_preferences;
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return None;
        }
        let prefs = raw.get("prefs")?.as_object()?;
        // The sole production producer already decoded this exact complete
        // response. Check its supported mask shape without rebuilding a CSV
        // set for each rendered label; the immutable Snapshot owns that set.
        let hidden_valid = match prefs.get("hidden_users") {
            None | Some(Value::Null) | Some(Value::Bool(false)) | Some(Value::String(_)) => true,
            Some(Value::Number(n)) => n.as_f64() == Some(0.0),
            _ => false,
        };
        hidden_valid.then_some(store)
    }
    pub(super) fn hidden(source: &Source, s: &ShellState, id: &str) -> Option<bool> {
        let store = Self::bound(source, s)?;
        Some(store.complete().names.policy.hidden(id))
    }
    pub(super) fn capture(source: &Source, s: &ShellState) -> Option<Self> {
        let store = Self::bound(source, s)?;
        let names = &store.complete().names;
        let prefs = names.user_preferences.get("prefs")?.as_object()?;
        // Numeric +/-1 dominate the unproved Source client-policy branch.
        // Numeric0 is admitted ONLY when complete original literal Boolean
        // data proves both fresh recipes agree. This never reads/assumes the
        // Source global memoized value or missing-field defaults.
        let value = prefs.get("display_real_names_override")?.as_f64()?;
        let expected = if value == 1.0 {
            true
        } else if value == -1.0 {
            false
        } else if value == 0.0 {
            names
                .policy
                .convergent_fresh_name_preference(&source.team)?
        } else {
            return None;
        };
        if names.policy.should_display_real_names().ok()? != expected {
            return None;
        }
        // Complete server prefs are a positive observation, not a cached/default
        // empty hidden map. Core's strict decoder refuses malformed values;
        // its raw CSV split matches the proved1sjB callback. Enterprise masking
        // inputs/membership are part of this SAME complete guarded policy.
        Some(Self { store })
    }
    pub(super) fn current(&self, source: &Source, s: &ShellState) -> bool {
        self.store
            .matches_reader(s, &source.client, &source.transport, &source.session)
    }
    pub(super) fn label(&self, user: &super_platinum_core::slack::models::User) -> Option<String> {
        let raw: Value = serde_json::to_value(user).ok()?;
        let label = self
            .store
            .complete()
            .names
            .policy
            .member_name(&raw, "Name hidden")
            .ok()?;
        (!label.is_empty()).then_some(label)
    }
}
