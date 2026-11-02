//! Finite preferred-tone picker DATA; no native purpose or freshness grant.
//! Source tone01 qualifies310 common single-tone bases alongside1582 tone-free
//! rows. Keep the existing tone-free API and its exclusions unchanged.
use super::*;
use crate::composer_emoji::{
    catalog::{Catalog, ReactionChoice},
    preferences::Preferences,
    standard::{self, Version},
};
use std::{collections::BTreeMap, sync::Arc};

struct Definition {
    entry: standard::Entry,
    glyphs: BTreeMap<u8, (String, String)>,
}
#[derive(Clone)]
pub struct PreferredDomain {
    definitions: Arc<BTreeMap<String, Definition>>,
}
impl PartialEq for PreferredDomain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.definitions, &other.definitions)
    }
}
impl Eq for PreferredDomain {}

/// Immutable displayed data, including the complete preference observation.
/// Constructing this value cannot establish an original renderer occurrence.
#[derive(Clone)]
pub struct PreferredChoice {
    domain: PreferredDomain,
    preferences: Arc<BTreeMap<String, Value>>,
    base: String,
    tone: u8,
    name: String,
    glyph: String,
}
impl PartialEq for PreferredChoice {
    fn eq(&self, other: &Self) -> bool {
        self.domain == other.domain
            && self.preferences == other.preferences
            && self.base == other.base
            && self.tone == other.tone
            && self.name == other.name
            && self.glyph == other.glyph
    }
}
impl Eq for PreferredChoice {}
impl PreferredChoice {
    pub fn base(&self) -> &str {
        &self.base
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn glyph(&self) -> &str {
        &self.glyph
    }
}
fn same(a: &standard::Entry, b: &standard::Entry) -> bool {
    a.name == b.name
        && a.unicode == b.unicode
        && a.alias_of == b.alias_of
        && a.obsoletes == b.obsoletes
        && a.multi_skin_tone == b.multi_skin_tone
        && a.skin_variations.len() == b.skin_variations.len()
        && a.skin_variations.iter().all(|(tone, variant)| {
            b.skin_variations
                .get(tone)
                .is_some_and(|other| other.name == variant.name && other.unicode == variant.unicode)
        })
}
fn single(entry: &standard::Entry) -> bool {
    entry.alias_of.is_none()
        && entry.obsoletes.is_none()
        && !entry.multi_skin_tone
        && entry.skin_variations.len() == 5
        && (2..=6).all(|tone| {
            entry
                .skin_variations
                .get(&tone.to_string())
                .is_some_and(|v| v.name == format!("{}::skin-tone-{tone}", entry.name))
        })
}
fn numeric_preference(prefs: &Preferences) -> Option<u8> {
    let n = prefs.raw().get("preferred_skin_tone")?.as_u64()?;
    (1..=6).contains(&n).then_some(n as u8)
}
impl PreferredDomain {
    /// Parse immutable builtins off render. Unknown prefs keep tone-free rows;
    /// they cannot qualify any tone-capable row, including its unmodified base.
    pub fn build() -> std::result::Result<Self, String> {
        let a = standard::Catalog::build(Version::V16)?;
        let b = standard::Catalog::build(Version::V17)?;
        let mut definitions = BTreeMap::new();
        for entry in &a.entries {
            let Some(other) = b.get(&entry.name) else {
                continue;
            };
            let tone_free = entry.alias_of.is_none()
                && entry.obsoletes.is_none()
                && !entry.multi_skin_tone
                && entry.skin_variations.is_empty();
            if !same(entry, other) || !(tone_free || single(entry)) {
                continue;
            }
            let mut glyphs = BTreeMap::new();
            for tone in 1..=if tone_free { 1 } else { 6 } {
                let tone_text = tone.to_string();
                let insertion = a
                    .insertion(&entry.name, (tone > 1).then_some(tone_text.as_str()))
                    .ok_or_else(|| "Preferred builtin glyph is unavailable.".to_owned())?;
                glyphs.insert(tone, (insertion.name, insertion.glyph));
            }
            definitions.insert(
                entry.name.clone(),
                Definition {
                    entry: entry.clone(),
                    glyphs,
                },
            );
        }
        Ok(Self {
            definitions: Arc::new(definitions),
        })
    }
    fn resolved(&self, catalog: &Catalog, base: &str, tone: u8) -> Option<(String, String)> {
        let definition = self.definitions.get(base)?;
        if !same(catalog.standard.get(base)?, &definition.entry) {
            return None;
        }
        let expected = definition.glyphs.get(&tone)?;
        // The composer resolver takes a base and a separate tone, never a
        // colon wire name. Require its exact final glyph/name before projection.
        let tone_text = tone.to_string();
        let current = catalog.resolve(base, (tone > 1).then_some(tone_text.as_str()))?;
        (!current.custom
            && current.image_url.is_none()
            && current.name == expected.0
            && current.glyph.as_deref() == Some(expected.1.as_str()))
        .then(|| expected.clone())
    }
    pub fn choices(&self, catalog: &Catalog, prefs: &Preferences) -> Vec<PreferredChoice> {
        if !prefs.loaded() {
            return vec![];
        }
        let preferences = Arc::new(prefs.raw().clone());
        self.definitions
            .iter()
            .filter_map(|(base, definition)| {
                let tone = if definition.entry.skin_variations.is_empty() {
                    1
                } else {
                    numeric_preference(prefs)?
                };
                let (name, glyph) = self.resolved(catalog, base, tone)?;
                Some(PreferredChoice {
                    domain: self.clone(),
                    preferences: preferences.clone(),
                    base: base.clone(),
                    tone,
                    name,
                    glyph,
                })
            })
            .collect()
    }
    fn accepts(&self, catalog: &Catalog, prefs: &Preferences, selected: &PreferredChoice) -> bool {
        selected.domain == *self
            && prefs.loaded()
            && selected.preferences.as_ref() == prefs.raw()
            && self.definitions.get(selected.base()).is_some_and(|d| {
                if d.entry.skin_variations.is_empty() {
                    selected.tone == 1
                } else {
                    numeric_preference(prefs) == Some(selected.tone)
                }
            })
            && self
                .resolved(catalog, selected.base(), selected.tone)
                .is_some_and(|(name, glyph)| name == selected.name && glyph == selected.glyph)
    }
}

#[derive(PartialEq)]
pub struct PreferredToggle {
    domain: PreferredDomain,
    policy: RootPolicy,
    membership: Value,
    displayed: PreferredChoice,
    choice: ReactionChoice,
    self_user: String,
    adding: bool,
}
impl RootMembership {
    pub fn first_add_preferred_selection(
        &self,
        policy: &RootPolicy,
        domain: &PreferredDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        displayed: &PreferredChoice,
        self_user: &str,
    ) -> Result<PreferredToggle> {
        if policy.original != self.original
            || !user_id(self_user)
            || prefs.raw() != policy.preferences().raw()
            || !domain.accepts(catalog, prefs, displayed)
        {
            return Err(UNAVAILABLE);
        }
        let choice = ReactionChoice::resolve(catalog, prefs, &self.reactions, displayed.name())
            .map_err(|_| UNAVAILABLE)?;
        if choice.name() != displayed.name() {
            return Err(UNAVAILABLE);
        }
        let existing = self.reactions.iter().find(|r| r.name == choice.name());
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
        Ok(PreferredToggle {
            domain: domain.clone(),
            policy: policy.clone(),
            membership: self.message.clone(),
            displayed: displayed.clone(),
            choice,
            self_user: self_user.into(),
            adding,
        })
    }
}
impl PreferredToggle {
    pub fn name(&self) -> &str {
        self.choice.name()
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// Snapshot comparison only; the native owner/watch/wire fences are separate.
    pub fn current(
        &self,
        policy: &RootPolicy,
        domain: &PreferredDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &RootMembership,
        self_user: &str,
    ) -> bool {
        membership
            .first_add_preferred_selection(
                policy,
                domain,
                catalog,
                prefs,
                &self.displayed,
                self_user,
            )
            .is_ok_and(|fresh| fresh == *self)
    }
}

#[cfg(test)]
mod tests;
