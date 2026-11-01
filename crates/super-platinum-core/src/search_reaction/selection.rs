//! Exact catalog/preference/membership selection data, never a native permit.
use super::{Membership, Result, UNAVAILABLE, reaction_name, user_id};
use crate::composer_emoji::{
    catalog::{Catalog, ReactionChoice},
    preferences::Preferences,
};

#[derive(Debug)]
pub struct SelectedReaction {
    choice: ReactionChoice,
    self_user: String,
    adding: bool,
}
impl Membership {
    /// The caller separately owns the genuine current catalog/preferences,
    /// original complete reaction read and authenticated actor. Resolve the
    /// actual wire name before consulting that actor's exact membership.
    pub fn selection(
        &self,
        catalog: &Catalog,
        prefs: &Preferences,
        requested: &str,
        self_user: &str,
    ) -> Result<SelectedReaction> {
        if !reaction_name(requested) || !user_id(self_user) {
            return Err(UNAVAILABLE);
        }
        let choice = ReactionChoice::resolve(catalog, prefs, self.reactions(), requested)
            .map_err(|_| UNAVAILABLE)?;
        let adding = self.adding(choice.name(), self_user);
        if !reaction_name(choice.name())
            || (adding
                && (!choice.addition_allowed()
                    || !self.addition_within_limits(requested, choice.name(), self_user)))
        {
            return Err(UNAVAILABLE);
        }
        Ok(SelectedReaction {
            choice,
            self_user: self_user.into(),
            adding,
        })
    }
    fn addition_within_limits(&self, requested: &str, resolved: &str, self_user: &str) -> bool {
        // Z1kg P09 counts raw rows containing self, not users, normalized
        // families or the requested name's count. Remove bypasses both limits.
        self.reactions()
            .iter()
            .filter(|r| r.users.iter().any(|user| user == self_user))
            .count()
            < 23
            && (self.reactions().len() < 50
                // Source checks its selected entry. Until requested/resolved
                // entry equivalence is proved here, require both to be present.
                || [requested, resolved]
                    .iter()
                    .all(|name| self.reactions().iter().any(|r| r.name == *name)))
    }
}
impl SelectedReaction {
    pub fn name(&self) -> &str {
        self.choice.name()
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    /// Retains the resolver's exact catalog identity, full preference snapshot
    /// and reaction observations, plus the original actor and operation. It
    /// grants no ownership/currentness beyond those inert data comparisons.
    pub fn current(
        &self,
        catalog: &Catalog,
        prefs: &Preferences,
        membership: &Membership,
        self_user: &str,
    ) -> bool {
        self.self_user == self_user
            && user_id(self_user)
            && self.choice.current(catalog, prefs, membership.reactions())
            && membership.adding(self.name(), self_user) == self.adding
            && (!self.adding
                || (self.choice.addition_allowed()
                    && membership.addition_within_limits(
                        self.choice.requested(),
                        self.name(),
                        self_user,
                    )))
    }
}
