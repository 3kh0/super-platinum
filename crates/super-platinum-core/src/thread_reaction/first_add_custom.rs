//! Isolated direct-custom picker DATA, never a native receipt or write grant.
//!
//! Retained Root custom02/Ou8q constructs a new definition from a direct
//! primitive String without predecessor alias/unknown fields. Both standard
//! maps miss these exact names, so picker normalization leaves the wire name
//! unchanged. This module does not inspect or repair a warm workspace map.
use super::*;
use crate::{
    composer_emoji::{
        preferences::Preferences,
        standard::{Catalog, Version},
    },
    slack::api::emoji_catalog::Complete,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

// Native allocation/domain bounds, not Slack schema or Source limits.
const MAX_ENTRIES: usize = 50_000;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_URL_BYTES: usize = 4096;

struct Definitions {
    direct: BTreeMap<String, String>,
}
/// Pure immutable data. The host must retain the actual original Complete
/// response, exact client/session/team, response owner and currentness guards.
#[derive(Clone)]
pub struct CustomDomain {
    definitions: Arc<Definitions>,
}
impl PartialEq for CustomDomain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.definitions, &other.definitions)
    }
}
impl Eq for CustomDomain {}

#[derive(PartialEq, Eq)]
struct Observation {
    original: OriginalRoot,
    preferences: BTreeMap<String, Value>,
}
/// No Debug/Serialize: the URL is data for a separately guarded native image
/// presenter, never a DOM src, clipboard value or native display instruction.
#[derive(Clone, PartialEq, Eq)]
pub struct CustomChoice {
    domain: CustomDomain,
    observation: Arc<Observation>,
    name: String,
    url: String,
}
impl CustomChoice {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn url(&self) -> &str {
        &self.url
    }
}

fn direct_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
        })
        // Conservative refusal of prototype-sensitive spellings; never infer
        // JS ordinary bracket lookup was an own-property-only operation.
        && !matches!(
            name,
            "__proto__"
                | "prototype"
                | "constructor"
                | "tostring"
                | "tolocalestring"
                | "valueof"
                | "hasownproperty"
                | "isprototypeof"
                | "propertyisenumerable"
                | "__definegetter__"
                | "__definesetter__"
                | "__lookupgetter__"
                | "__lookupsetter__"
        )
}

fn direct_url(literal: &str) -> bool {
    // Validate without replacing the literal by Url::to_string. In particular,
    // reject whitespace/backslash repair before WHATWG URL parsing can do it.
    if literal.is_empty()
        || literal.len() > MAX_URL_BYTES
        || !literal.starts_with("https://")
        || literal
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return false;
    }
    let authority = literal["https://".len()..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let Ok(url) = url::Url::parse(literal) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

impl CustomDomain {
    /// Build off render from an actual complete-map DATA value. This public
    /// constructor cannot prove where that value came from. Unsupported rows
    /// are omitted; a response exceeding the native allocation bounds refuses
    /// the entire domain. Aliases and both-version standard collisions stay out.
    pub fn build(complete: &Complete) -> std::result::Result<Self, String> {
        let map = complete.raw()["emoji"]
            .as_object()
            .ok_or_else(|| "Complete emoji data is unavailable.".to_owned())?;
        if map.len() > MAX_ENTRIES {
            return Err("Complete emoji data exceeds the native bound.".into());
        }
        let mut bytes = 0usize;
        for (name, value) in map {
            let value = value
                .as_str()
                .ok_or_else(|| "Complete emoji data is unavailable.".to_owned())?;
            bytes = bytes
                .checked_add(name.len())
                .and_then(|n| n.checked_add(value.len()))
                .filter(|n| *n <= MAX_TOTAL_BYTES)
                .ok_or_else(|| "Complete emoji data exceeds the native bound.".to_owned())?;
        }
        // These are the COMPLETE base+alias key tables, not the current
        // first-Add glyph/preferred/alias admitted subsets and not V16 alone.
        let a = Catalog::build(Version::V16)?;
        let b = Catalog::build(Version::V17)?;
        let names: BTreeSet<_> = a
            .entries
            .iter()
            .chain(&b.entries)
            .map(|entry| entry.name.as_str())
            .collect();
        let direct = map
            .iter()
            .filter_map(|(name, value)| {
                let value = value.as_str()?;
                (direct_name(name) && !names.contains(name.as_str()) && direct_url(value))
                    .then(|| (name.clone(), value.to_owned()))
            })
            .collect();
        Ok(Self {
            definitions: Arc::new(Definitions { direct }),
        })
    }

    /// Stable native name order, not Source locale/frecency/worker ordering.
    /// Full membership is confirmed before projection, but does not become a
    /// retained write roster: selection re-evaluates the fresh exact-name rows.
    pub fn choices(
        &self,
        policy: &RootPolicy,
        prefs: &Preferences,
        membership: &RootMembership,
    ) -> Vec<CustomChoice> {
        if !prefs.loaded()
            || prefs.raw() != policy.preferences().raw()
            || policy.original != membership.original
        {
            return vec![];
        }
        let observation = Arc::new(Observation {
            original: policy.original.clone(),
            preferences: prefs.raw().clone(),
        });
        self.definitions
            .direct
            .iter()
            .map(|(name, url)| CustomChoice {
                domain: self.clone(),
                observation: observation.clone(),
                name: name.clone(),
                url: url.clone(),
            })
            .collect()
    }

    fn accepts(&self, prefs: &Preferences, choice: &CustomChoice) -> bool {
        choice.domain == *self
            && prefs.loaded()
            && &choice.observation.preferences == prefs.raw()
            && self.definitions.direct.get(choice.name()) == Some(&choice.url)
    }
}

/// Immutable observation comparison only. No request, serialization, current
/// actor/provider/namespace, native permission, resource or physical slot.
#[derive(PartialEq)]
pub struct CustomToggle {
    domain: CustomDomain,
    policy: RootPolicy,
    membership: Value,
    choice: CustomChoice,
    self_user: String,
    adding: bool,
}
impl RootMembership {
    pub fn first_add_custom_selection(
        &self,
        policy: &RootPolicy,
        domain: &CustomDomain,
        prefs: &Preferences,
        choice: &CustomChoice,
        self_user: &str,
    ) -> Result<CustomToggle> {
        if policy.original != self.original
            || policy.original != choice.observation.original
            || !user_id(self_user)
            || prefs.raw() != policy.preferences().raw()
            || !domain.accepts(prefs, choice)
        {
            return Err(UNAVAILABLE);
        }
        // Source ZpXj keeps an existing exact name; otherwise this fresh direct
        // definition canonicalizes to the SAME name. Both complete standard
        // maps miss it, so obsolete/tone/alias branches cannot retarget it.
        let existing = self.reactions.iter().find(|r| r.name == choice.name);
        let adding = existing.is_none_or(|r| !r.users.iter().any(|u| u == self_user));
        if adding
            && (self
                .reactions
                .iter()
                .filter(|r| r.users.iter().any(|u| u == self_user))
                .count()
                >= 23
                || existing.is_none() && self.reactions.len() >= 50)
        {
            return Err(UNAVAILABLE);
        }
        Ok(CustomToggle {
            domain: domain.clone(),
            policy: policy.clone(),
            membership: self.message.clone(),
            choice: choice.clone(),
            self_user: self_user.into(),
            adding,
        })
    }
}
impl CustomToggle {
    pub fn name(&self) -> &str {
        self.choice.name()
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// PURE snapshot equivalence, not current native scope or a wire fence.
    pub fn current(
        &self,
        policy: &RootPolicy,
        domain: &CustomDomain,
        prefs: &Preferences,
        membership: &RootMembership,
        self_user: &str,
    ) -> bool {
        membership
            .first_add_custom_selection(policy, domain, prefs, &self.choice, self_user)
            .is_ok_and(|fresh| fresh == *self)
    }
}

#[cfg(test)]
mod tests;
