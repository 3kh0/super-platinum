//! Exact finite alias-query DATA; never a native purpose or SourceReady grant.
//! Source alias-query01/plan01 prove55 direct common aliases (46+9). Old
//! tone-free/preferred APIs remain unchanged; wire-branch changes are refused.
use super::*;
use crate::composer_emoji::{
    catalog::{Catalog, ReactionChoice},
    preferences::Preferences,
    standard::{self, Version},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, PartialEq, Eq)]
pub struct AliasQuery {
    literal: String,
    name: String,
}
impl AliasQuery {
    /// Source removes ALL colons before ECMAScript trim. This finite profile
    /// accepts exact lowercase ASCII aliases, not fuzzy/localized/ranked input.
    pub fn parse(literal: &str) -> Option<Self> {
        if literal.len() > 4096 {
            return None;
        }
        let removed = literal.replace(':', "");
        let name = removed.trim_matches(js_space);
        if name.is_empty()
            || name.len() > 256
            || !name.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'+')
            })
        {
            return None;
        }
        Some(Self {
            literal: literal.into(),
            name: name.into(),
        })
    }
    pub fn literal(&self) -> &str {
        &self.literal
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn matches(&self, choice: &AliasChoice) -> bool {
        self.name == choice.matched_alias
    }
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}

struct Glyph {
    alias: String,
    canonical: String,
    glyph: String,
}
struct Definition {
    alias: standard::Entry,
    canonical: standard::Entry,
    glyphs: BTreeMap<u8, Glyph>,
}
#[derive(Clone)]
pub struct AliasDomain {
    definitions: Arc<BTreeMap<String, Definition>>,
}
impl PartialEq for AliasDomain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.definitions, &other.definitions)
    }
}
impl Eq for AliasDomain {}
#[derive(PartialEq, Eq)]
struct Observation {
    original: OriginalRoot,
    preferences: BTreeMap<String, Value>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct AliasChoice {
    domain: AliasDomain,
    observation: Arc<Observation>,
    matched_alias: String,
    canonical_group: String,
    tone: u8,
    selected_alias: String,
    canonical_wire: String,
    wire: String,
    glyph: String,
}
impl AliasChoice {
    pub fn matched_alias(&self) -> &str {
        &self.matched_alias
    }
    pub fn canonical_group(&self) -> &str {
        &self.canonical_group
    }
    pub fn selected_name(&self) -> &str {
        &self.selected_alias
    }
    pub fn wire_name(&self) -> &str {
        &self.wire
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
        && a.skin_variations.iter().all(|(tone, v)| {
            b.skin_variations
                .get(tone)
                .is_some_and(|other| v.name == other.name && v.unicode == other.unicode)
        })
}
fn canonical(entry: &standard::Entry) -> bool {
    entry.alias_of.is_none()
        && entry.obsoletes.is_none()
        && !entry.multi_skin_tone
        && (entry.skin_variations.is_empty()
            || entry.skin_variations.len() == 5
                && (2..=6).all(|tone| {
                    entry
                        .skin_variations
                        .get(&tone.to_string())
                        .is_some_and(|v| v.name == format!("{}::skin-tone-{tone}", entry.name))
                }))
}
fn direct(alias: &standard::Entry, target: &standard::Entry) -> bool {
    alias.name != target.name
        && alias.alias_of.as_deref() == Some(target.name.as_str())
        && alias.obsoletes.is_none()
        && !alias.multi_skin_tone
        && canonical(target)
        && alias.unicode == target.unicode
        && alias.skin_variations.len() == target.skin_variations.len()
        && alias.skin_variations.iter().all(|(tone, v)| {
            target.skin_variations.get(tone).is_some_and(|other| {
                v.name == format!("{}::skin-tone-{tone}", alias.name) && v.unicode == other.unicode
            })
        })
}
fn tone(entry: &standard::Entry, prefs: &Preferences) -> Option<u8> {
    if entry.skin_variations.is_empty() {
        return Some(1);
    }
    let n = prefs.raw().get("preferred_skin_tone")?.as_u64()?;
    (1..=6).contains(&n).then_some(n as u8)
}
impl AliasDomain {
    /// Immutable data parsing belongs on a worker, never a render/event path.
    pub fn build() -> std::result::Result<Self, String> {
        let a = standard::Catalog::build(Version::V16)?;
        let b = standard::Catalog::build(Version::V17)?;
        let mut definitions = BTreeMap::new();
        for alias in &a.entries {
            let Some(target_name) = alias.alias_of.as_deref() else {
                continue;
            };
            let (Some(target), Some(other_alias), Some(other_target)) =
                (a.get(target_name), b.get(&alias.name), b.get(target_name))
            else {
                continue;
            };
            if !same(alias, other_alias) || !same(target, other_target) || !direct(alias, target) {
                continue;
            }
            let mut glyphs = BTreeMap::new();
            for tone in 1..=if alias.skin_variations.is_empty() {
                1
            } else {
                6
            } {
                let text = tone.to_string();
                let preferred = (tone > 1).then_some(text.as_str());
                let left = a
                    .insertion(&alias.name, preferred)
                    .ok_or_else(|| "Alias glyph is unavailable.".to_owned())?;
                let right = a
                    .insertion(&target.name, preferred)
                    .ok_or_else(|| "Alias target glyph is unavailable.".to_owned())?;
                if left.glyph != right.glyph {
                    return Err("Alias and target glyphs differ.".into());
                }
                glyphs.insert(
                    tone,
                    Glyph {
                        alias: left.name,
                        canonical: right.name,
                        glyph: left.glyph,
                    },
                );
            }
            definitions.insert(
                alias.name.clone(),
                Definition {
                    alias: alias.clone(),
                    canonical: target.clone(),
                    glyphs,
                },
            );
        }
        Ok(Self {
            definitions: Arc::new(definitions),
        })
    }
    fn resolved<'a>(&'a self, catalog: &Catalog, alias: &str, tone: u8) -> Option<&'a Glyph> {
        let d = self.definitions.get(alias)?;
        if !same(catalog.standard.get(alias)?, &d.alias)
            || !same(catalog.standard.get(&d.canonical.name)?, &d.canonical)
        {
            return None;
        }
        let expected = d.glyphs.get(&tone)?;
        let text = tone.to_string();
        let preferred = (tone > 1).then_some(text.as_str());
        let left = catalog.resolve(alias, preferred)?;
        let right = catalog.resolve(&d.canonical.name, preferred)?;
        (!left.custom
            && !right.custom
            && left.image_url.is_none()
            && right.image_url.is_none()
            && left.name == expected.alias
            && right.name == expected.canonical
            && left.glyph.as_deref() == Some(expected.glyph.as_str())
            && right.glyph.as_deref() == Some(expected.glyph.as_str()))
        .then_some(expected)
    }
    /// Full membership is observed before projection. One shared snapshot, not
    /// one potentially large frecency copy per alias. These remain inert DATA.
    pub fn choices(
        &self,
        policy: &RootPolicy,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &RootMembership,
    ) -> Vec<AliasChoice> {
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
        let names = membership
            .reactions
            .iter()
            .map(|r| r.name.as_str())
            .collect::<BTreeSet<_>>();
        self.definitions
            .iter()
            .filter_map(|(alias, d)| {
                let tone = tone(&d.alias, prefs)?;
                let glyph = self.resolved(catalog, alias, tone)?;
                let wire = if names.contains(glyph.alias.as_str()) {
                    &glyph.alias
                } else {
                    &glyph.canonical
                };
                Some(AliasChoice {
                    domain: self.clone(),
                    observation: observation.clone(),
                    matched_alias: alias.clone(),
                    canonical_group: d.canonical.name.clone(),
                    tone,
                    selected_alias: glyph.alias.clone(),
                    canonical_wire: glyph.canonical.clone(),
                    wire: wire.clone(),
                    glyph: glyph.glyph.clone(),
                })
            })
            .collect()
    }
    fn accepts(&self, catalog: &Catalog, prefs: &Preferences, choice: &AliasChoice) -> bool {
        choice.domain == *self
            && prefs.loaded()
            && &choice.observation.preferences == prefs.raw()
            && self
                .definitions
                .get(choice.matched_alias())
                .is_some_and(|d| {
                    d.canonical.name == choice.canonical_group
                        && tone(&d.alias, prefs) == Some(choice.tone)
                })
            && self
                .resolved(catalog, choice.matched_alias(), choice.tone)
                .is_some_and(|g| {
                    g.alias == choice.selected_alias
                        && g.canonical == choice.canonical_wire
                        && g.glyph == choice.glyph
                        && (choice.wire == g.alias || choice.wire == g.canonical)
                })
    }
}
#[derive(PartialEq)]
pub struct AliasToggle {
    domain: AliasDomain,
    policy: RootPolicy,
    membership: Value,
    displayed: AliasChoice,
    choice: ReactionChoice,
    self_user: String,
    adding: bool,
}
impl RootMembership {
    pub fn first_add_alias_selection(
        &self,
        policy: &RootPolicy,
        domain: &AliasDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        displayed: &AliasChoice,
        self_user: &str,
    ) -> Result<AliasToggle> {
        if policy.original != self.original
            || policy.original != displayed.observation.original
            || !user_id(self_user)
            || prefs.raw() != policy.preferences().raw()
            || !domain.accepts(catalog, prefs, displayed)
        {
            return Err(UNAVAILABLE);
        }
        let choice =
            ReactionChoice::resolve(catalog, prefs, &self.reactions, displayed.selected_name())
                .map_err(|_| UNAVAILABLE)?;
        // The source input stays the captured alias. Fresh normalization may
        // not silently switch the prepared alias/canonical wire branch.
        if choice.name() != displayed.wire_name() {
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
        Ok(AliasToggle {
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
impl AliasToggle {
    pub fn name(&self) -> &str {
        self.choice.name()
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// Pure immutable observation equality; native owner/Loaded/watch and wire
    /// fencing are separate and cannot be supplied by these public DATA values.
    pub fn current(
        &self,
        policy: &RootPolicy,
        domain: &AliasDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &RootMembership,
        self_user: &str,
    ) -> bool {
        membership
            .first_add_alias_selection(policy, domain, catalog, prefs, &self.displayed, self_user)
            .is_ok_and(|fresh| fresh == *self)
    }
}
#[cfg(test)]
mod tests;
