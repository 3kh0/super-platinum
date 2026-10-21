//! Reaction selection, separate from composer shortcode insertion. Retained
//! 5tUG/UHpe/NTFR/dSSJ/jGtm, executed selector contract (56 cases). Picker caller
//! uses defaultMulti=false; an existing no-own pill addition uses true.
use super::Catalog;
use crate::{composer_emoji::preferences::Preferences, slack::models::Reaction};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ReactionChoice {
    identity: Arc<()>,
    requested: String,
    name: String,
    preferences: Value,
    reactions: Value,
    addition_allowed: bool,
    kind: ChoiceKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum ChoiceKind {
    Picker,
    PillAddition { user: String, base: String },
}
impl PartialEq for ReactionChoice {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.requested == other.requested
            && self.name == other.name
            && self.preferences == other.preferences
            && self.reactions == other.reactions
            && self.addition_allowed == other.addition_allowed
            && self.kind == other.kind
    }
}
impl Eq for ReactionChoice {}

impl ReactionChoice {
    pub fn resolve(
        catalog: &Catalog,
        preferences: &Preferences,
        reactions: &[Reaction],
        requested: &str,
    ) -> Result<Self, String> {
        Self::resolve_inner(catalog, preferences, reactions, requested, None)
    }

    /// An actual existing pill, with no observed own raw-base family. The
    /// retained pill caller fixes defaultMulti=true. This is addition-only:
    /// later self membership must retire this choice, never flip its operation.
    pub fn resolve_pill_addition(
        catalog: &Catalog,
        preferences: &Preferences,
        reactions: &[Reaction],
        requested: &str,
        user: &str,
    ) -> Result<Self, String> {
        if !actor(user) || !reactions.iter().any(|r| r.name == requested) {
            return Err("The original reaction pill is unavailable.".into());
        }
        Self::resolve_inner(catalog, preferences, reactions, requested, Some(user))
    }

    fn resolve_inner(
        catalog: &Catalog,
        preferences: &Preferences,
        reactions: &[Reaction],
        requested: &str,
        pill_user: Option<&str>,
    ) -> Result<Self, String> {
        if !preferences.loaded() || !catalog.reaction_supported_spelling(requested) {
            return Err("Reaction catalog or preferences are unavailable.".into());
        }
        // NTFR invalid produces a truthy Error object. For the bounded picker
        // this means no default-tone append, not validity or a successful name.
        let multi_or_invalid =
            !catalog.reaction_valid(requested) || catalog.reaction_multi(requested);
        let resolved = if reactions.iter().any(|r| r.name == requested) {
            requested.to_owned()
        } else {
            let canonical = catalog
                .reaction_canonical(requested)
                .ok_or("The selected reaction is unavailable.")?;
            catalog
                .reaction_obsolete(&canonical)
                .filter(|old| reactions.iter().any(|r| &r.name == old))
                .unwrap_or(canonical)
        };
        let name = if (pill_user.is_some() || !multi_or_invalid)
            && catalog.reaction_modifiable(&resolved)
        {
            preferences
                .preferred_tone()
                .map(|tone| format!("{resolved}::skin-tone-{tone}"))
                .unwrap_or(resolved)
        } else {
            resolved
        };
        let existing_url = reactions
            .iter()
            .find(|r| r.name == name)
            .is_some_and(|r| truthy(r.extra.get("url")));
        if !catalog.reaction_supported_spelling(&name) {
            return Err("The selected reaction is unavailable.".into());
        }
        let addition_allowed = catalog.reaction_valid(&name) || existing_url;
        let kind = if let Some(user) = pill_user {
            let (base, _) = parsed(&name).ok_or("The selected reaction is unavailable.")?;
            no_own_family(reactions, user, &base)?;
            if !addition_allowed {
                return Err("The selected reaction is unavailable.".into());
            }
            ChoiceKind::PillAddition {
                user: user.into(),
                base,
            }
        } else {
            ChoiceKind::Picker
        };
        Ok(Self {
            identity: catalog.reaction_identity.clone(),
            requested: requested.into(),
            name,
            addition_allowed,
            kind,
            preferences: serde_json::to_value(preferences)
                .map_err(|_| "Reaction preferences are malformed.")?,
            reactions: serde_json::to_value(reactions)
                .map_err(|_| "Reaction membership is malformed.")?,
        })
    }
    pub fn requested(&self) -> &str {
        &self.requested
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Catalog identity and exact complete prefs/row observations. Owner, visit,
    /// socket and the immutable native Loaded lifetime remain caller-owned.
    pub fn current(
        &self,
        catalog: &Catalog,
        preferences: &Preferences,
        reactions: &[Reaction],
    ) -> bool {
        let current = match &self.kind {
            ChoiceKind::Picker => Self::resolve(catalog, preferences, reactions, &self.requested),
            ChoiceKind::PillAddition { user, .. } => {
                Self::resolve_pill_addition(catalog, preferences, reactions, &self.requested, user)
            }
        };
        current.is_ok_and(|now| now == *self)
    }
    pub(crate) fn matches_reactions(&self, reactions: &[Reaction]) -> bool {
        serde_json::to_value(reactions).is_ok_and(|raw| raw == self.reactions)
    }
    pub(crate) fn addition_allowed(&self) -> bool {
        self.addition_allowed
    }
    pub(crate) fn accepts_actor(&self, user: &str) -> bool {
        match &self.kind {
            ChoiceKind::Picker => true,
            ChoiceKind::PillAddition { user: original, .. } => original == user,
        }
    }
    pub(crate) fn addition_only(&self) -> bool {
        matches!(self.kind, ChoiceKind::PillAddition { .. })
    }
}

fn actor(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 256
        && !user.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn no_own_family(reactions: &[Reaction], user: &str, base: &str) -> Result<(), String> {
    let mut names = std::collections::HashSet::new();
    for reaction in reactions {
        let users: std::collections::HashSet<_> = reaction.users.iter().collect();
        if !names.insert(&reaction.name)
            || users.len() != reaction.users.len()
            || reaction.users.iter().any(|u| !actor(u))
            || reaction.count < users.len() as u32
        {
            return Err("Reaction membership is ambiguous.".into());
        }
        if reaction.users.iter().any(|u| u == user) {
            // The source permissive regex can assign malformed tone spellings
            // surprising bases. Do not treat an unassignable own name as proof
            // that this family is empty; native names stay bounded as before.
            let Some((owned_base, _)) = parsed(&reaction.name) else {
                return Err("The own reaction family is unavailable.".into());
            };
            if owned_base == base {
                return Err("The original reaction family is already owned.".into());
            }
        }
    }
    Ok(())
}

impl Catalog {
    fn reaction_supported_spelling(&self, name: &str) -> bool {
        let Some((base, tone)) = parsed(name) else {
            return false;
        };
        // ce1x/UHpe actually fall back to the base for unsupported tones, which
        // can cause dSSJ to generate a second suffix. Native picker choices are
        // bounded to real catalog variants; this refusal is explicit adaptation.
        self.standard
            .get(&base)
            .is_none_or(|entry| tone.is_none_or(|tone| entry.skin_variations.contains_key(tone)))
    }
    fn reaction_canonical(&self, name: &str) -> Option<String> {
        let (base, tone) = parsed(name)?;
        let canonical = if let Some(entry) = self.standard.get(&base) {
            entry.alias_of.as_deref().unwrap_or(&base)
        } else {
            let entry = self.custom.get(&base)?;
            entry.value.strip_prefix("alias:").unwrap_or(&base)
        };
        Some(with_tone(canonical, tone))
    }
    fn reaction_obsolete(&self, name: &str) -> Option<String> {
        let (base, tone) = parsed(name)?;
        Some(with_tone(
            self.standard.get(&base)?.obsoletes.as_deref()?,
            tone,
        ))
    }
    fn reaction_valid(&self, name: &str) -> bool {
        let Some((base, tone)) = parsed(name) else {
            return false;
        };
        if let Some(entry) = self.standard.get(&base) {
            return tone.is_none_or(|tone| entry.skin_variations.contains_key(tone));
        }
        self.custom
            .get(&base)
            .is_some_and(|entry| !entry.is_nonexistent)
    }
    fn reaction_multi(&self, name: &str) -> bool {
        let Some((base, _)) = parsed(name) else {
            return false;
        };
        self.reaction_canonical(&base)
            .and_then(|name| self.standard.get(&name))
            .is_some_and(|entry| entry.multi_skin_tone)
    }
    fn reaction_modifiable(&self, name: &str) -> bool {
        let Some(canonical) = self.reaction_canonical(name) else {
            return false;
        };
        let Some((base, tone)) = parsed(&canonical) else {
            return false;
        };
        // Actual variant metadata has no skinVariations: an explicit valid
        // tone is not replaced, and preferred tone is never appended twice.
        tone.is_none()
            && self
                .standard
                .get(&base)
                .is_some_and(|e| !e.skin_variations.is_empty())
    }
}

fn with_tone(base: &str, tone: Option<&str>) -> String {
    tone.map(|tone| format!("{base}::skin-tone-{tone}"))
        .unwrap_or_else(|| base.into())
}
// A picker supplies an exact unwrapped name. Restrict malformed permissive
// ue25 regex spellings rather than turn them into newly sendable identities.
fn parsed(name: &str) -> Option<(String, Option<&str>)> {
    let (base, tone) = name
        .split_once("::skin-tone-")
        .map_or((name, None), |(b, t)| (b, Some(t)));
    if base.is_empty()
        || name.len() > 256
        || !base
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
    {
        return None;
    }
    if tone.is_some_and(|t| {
        !matches!(
            t.as_bytes(),
            [b'2'..=b'6'] | [b'2'..=b'6', b'-', b'2'..=b'6']
        )
    }) {
        return None;
    }
    Some((base.to_ascii_lowercase(), tone))
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::Number(v)) => v.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
#[path = "reaction/tests.rs"]
mod tests;
