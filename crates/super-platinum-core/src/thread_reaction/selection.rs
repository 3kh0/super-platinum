use super::*;
use crate::composer_emoji::{
    catalog::{Catalog, ReactionChoice},
    preferences::Preferences,
};

/// A fixed single-wire DATA selection. Native custody/dispatch is separate.
#[derive(PartialEq)]
pub struct RootToggle {
    policy: RootPolicy,
    membership: Value,
    choice: ReactionChoice,
    requested: String,
    name: String,
    self_user: String,
    adding: bool,
}
impl RootMembership {
    pub fn selection(
        &self,
        policy: &RootPolicy,
        catalog: &Catalog,
        prefs: &Preferences,
        requested: &str,
        self_user: &str,
    ) -> Result<RootToggle> {
        if policy.original != self.original
            || !user_id(self_user)
            || !prefs.loaded()
            || prefs.raw() != policy.preferences().raw()
            || !standard_spelling(catalog, requested)
            || !self.reactions.iter().any(|r| r.name == requested)
        {
            return Err(UNAVAILABLE);
        }
        let base = family(requested).ok_or(UNAVAILABLE)?;
        let mut owned = Vec::new();
        let mut owned_rows = 0usize;
        for row in &self.reactions {
            if row.users.iter().any(|id| id == self_user) {
                owned_rows += 1;
                // Unassignable own spellings cannot prove a family empty.
                if family(&row.name).ok_or(UNAVAILABLE)? == base {
                    owned.push(row.name.as_str());
                }
            }
        }
        let (adding, name, choice) = match owned.as_slice() {
            [] => {
                let choice = ReactionChoice::resolve_pill_addition(
                    catalog,
                    prefs,
                    &self.reactions,
                    requested,
                    self_user,
                )
                .map_err(|_| UNAVAILABLE)?;
                if !standard_spelling(catalog, choice.name())
                    || family(choice.name()) != Some(base)
                    || owned_rows >= 23
                    || (self.reactions.len() >= 50
                        && !self.reactions.iter().any(|r| r.name == choice.name()))
                {
                    return Err(UNAVAILABLE);
                }
                (true, choice.name().to_owned(), choice)
            }
            [name] => {
                if !standard_spelling(catalog, name) {
                    return Err(UNAVAILABLE);
                }
                // This choice binds catalog identity/prefs/rows only. Its
                // potentially tone-resolved name NEVER replaces the actual
                // owned wire spelling on Remove.
                let choice = ReactionChoice::resolve(catalog, prefs, &self.reactions, name)
                    .map_err(|_| UNAVAILABLE)?;
                (false, (*name).to_owned(), choice)
            }
            _ => return Err(UNAVAILABLE), // Source multi-remove is outside one wire.
        };
        Ok(RootToggle {
            policy: policy.clone(),
            membership: self.message.clone(),
            choice,
            requested: requested.into(),
            name,
            self_user: self_user.into(),
            adding,
        })
    }
}
impl RootToggle {
    pub fn requested(&self) -> &str {
        &self.requested
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// Pure input equivalence, including catalog identity and exact raw rows.
    /// The host still validates actual producer owner/revision, original scope,
    /// authenticated principal, read watch, current node and physical one-shot.
    pub fn current(
        &self,
        policy: &RootPolicy,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &RootMembership,
        self_user: &str,
    ) -> bool {
        membership
            .selection(policy, catalog, prefs, &self.requested, self_user)
            .is_ok_and(|fresh| fresh == *self)
    }
}

// Finite standard-wire subset of the retained raw-base family operation.
// No colon wrapping/repair/case folding or alias canonicalization is performed.
fn family(name: &str) -> Option<&str> {
    let (base, tone) = name
        .split_once("::skin-tone-")
        .map_or((name, None), |(base, tone)| (base, Some(tone)));
    if base.is_empty()
        || name.len() > 256
        || !base.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'+')
        })
        || tone.is_some_and(|t| {
            !matches!(
                t.as_bytes(),
                [b'2'..=b'6'] | [b'2'..=b'6', b'-', b'2'..=b'6']
            )
        })
    {
        return None;
    }
    Some(base)
}
fn standard_spelling(catalog: &Catalog, name: &str) -> bool {
    let Some(base) = family(name) else {
        return false;
    };
    let Some(entry) = catalog.standard.get(base) else {
        return false;
    };
    if name
        .split_once("::skin-tone-")
        .is_some_and(|(_, tone)| !entry.skin_variations.contains_key(tone))
    {
        return false;
    }
    // A custom override is outside this glyph-only standard-name profile.
    // Composer lookup accepts a base and separate tone, never Slack's colon
    // wire suffix. Validate its glyph without changing the selected wire name.
    let tone = name.split_once("::skin-tone-").map(|(_, tone)| tone);
    catalog
        .resolve(base, tone)
        .is_some_and(|v| !v.custom && v.image_url.is_none() && v.glyph.is_some())
}
