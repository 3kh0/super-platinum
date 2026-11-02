//! Finite first-Add picker DATA, not a native purpose or SourceReady grant.
//!
//! Source standard-data01 and native-plan02 establish the joint1582 canonical,
//! tone-free profile in BOTH retained version maps. The host still owns original
//! raw intake, actual current catalog producer, namespace, UI and physical IO.
use super::*;
use crate::composer_emoji::{
    catalog::{Catalog, ReactionChoice},
    preferences::Preferences,
    standard::{self, Version},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstAddGlyph {
    name: String,
    glyph: String,
}
impl FirstAddGlyph {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn glyph(&self) -> &str {
        &self.glyph
    }
}
struct Definition {
    unicode: String,
    glyph: FirstAddGlyph,
}
/// Immutable joint builtin DATA. Build off render; no disk/network work occurs.
#[derive(Clone)]
pub struct FirstAddDomain {
    definitions: Arc<BTreeMap<String, Definition>>,
}
impl PartialEq for FirstAddDomain {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.definitions, &b.definitions)
    }
}
impl Eq for FirstAddDomain {}
fn canonical(entry: &standard::Entry) -> bool {
    entry.alias_of.is_none()
        && entry.obsoletes.is_none()
        && entry.skin_variations.is_empty()
        && !entry.multi_skin_tone
}
impl FirstAddDomain {
    /// Parses the two existing immutable datasets; callers must use a worker.
    /// A V16-only filter admits six entries outside the proven joint profile.
    pub fn build() -> std::result::Result<Self, String> {
        let v16 = standard::Catalog::build(Version::V16)?;
        let v17 = standard::Catalog::build(Version::V17)?;
        let mut definitions = BTreeMap::new();
        for a in &v16.entries {
            let Some(b) = v17.get(&a.name) else { continue };
            if !canonical(a) || !canonical(b) || a.unicode != b.unicode {
                continue;
            }
            let glyph = v16
                .insertion(&a.name, None)
                .ok_or_else(|| "First-Add builtin glyph is unavailable.".to_owned())?
                .glyph;
            definitions.insert(
                a.name.clone(),
                Definition {
                    unicode: a.unicode.clone(),
                    glyph: FirstAddGlyph {
                        name: a.name.clone(),
                        glyph,
                    },
                },
            );
        }
        Ok(Self {
            definitions: Arc::new(definitions),
        })
    }
    fn resolved(&self, catalog: &Catalog, name: &str) -> Option<FirstAddGlyph> {
        let definition = self.definitions.get(name)?;
        let entry = catalog.standard.get(name)?;
        if !canonical(entry) || entry.name != name || entry.unicode != definition.unicode {
            return None;
        }
        let current = catalog.resolve(name, None)?;
        (!current.custom
            && current.image_url.is_none()
            && current.name == name
            && current.glyph.as_deref() == Some(definition.glyph.glyph()))
        .then(|| definition.glyph.clone())
    }
    /// Filter before projection: this API returns glyphs only, never a URL or
    /// registration instruction. Native callers also fence Loaded ownership.
    pub fn choices(&self, catalog: &Catalog) -> Vec<FirstAddGlyph> {
        self.definitions
            .keys()
            .filter_map(|name| self.resolved(catalog, name))
            .collect()
    }
}

/// Exactly one immutable normalized wire operation; IO/currentness is separate.
#[derive(PartialEq)]
pub struct FirstAddToggle {
    domain: FirstAddDomain,
    policy: RootPolicy,
    membership: Value,
    choice: ReactionChoice,
    requested: String,
    name: String,
    self_user: String,
    adding: bool,
}
impl RootMembership {
    pub fn first_add_selection(
        &self,
        policy: &RootPolicy,
        domain: &FirstAddDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        requested: &str,
        self_user: &str,
    ) -> Result<FirstAddToggle> {
        if policy.original != self.original
            || !user_id(self_user)
            || !prefs.loaded()
            || prefs.raw() != policy.preferences().raw()
            || domain.resolved(catalog, requested).is_none()
        {
            return Err(UNAVAILABLE);
        }
        let choice = ReactionChoice::resolve(catalog, prefs, &self.reactions, requested)
            .map_err(|_| UNAVAILABLE)?;
        // No alias/obsolete/tone substitution can escape the named domain.
        if choice.name() != requested || domain.resolved(catalog, choice.name()).is_none() {
            return Err(UNAVAILABLE);
        }
        let existing = self.reactions.iter().find(|r| r.name == choice.name());
        // Picker actionSource differs from an existing-pill family toggle.
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
        Ok(FirstAddToggle {
            domain: domain.clone(),
            policy: policy.clone(),
            membership: self.message.clone(),
            name: choice.name().into(),
            choice,
            requested: requested.into(),
            self_user: self_user.into(),
            adding,
        })
    }
}
impl FirstAddToggle {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// Pure snapshot equivalence only. This cannot validate a native original
    /// owner, catalog producer, callback occurrence, read watch or wire fence.
    pub fn current(
        &self,
        policy: &RootPolicy,
        domain: &FirstAddDomain,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &RootMembership,
        self_user: &str,
    ) -> bool {
        membership
            .first_add_selection(policy, domain, catalog, prefs, &self.requested, self_user)
            .is_ok_and(|fresh| fresh == *self)
    }
}

#[cfg(test)]
mod tests;
