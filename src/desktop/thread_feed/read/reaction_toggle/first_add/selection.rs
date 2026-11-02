//! Private tagged presentation/operation. No query string is a wire grant.
use super::*;
use super_platinum_core::thread_reaction::{
    first_add_alias::{AliasChoice, AliasToggle},
    first_add_preferred::PreferredToggle,
};
#[derive(Clone)]
pub(super) enum Selected {
    Canonical(PreferredChoice),
    Alias(AliasChoice),
    Custom(custom::Selection),
}
impl Selected {
    pub(super) fn native_current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Custom(c) => c.current(shell),
            _ => true,
        }
    }
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Canonical(c) => c.name(),
            Self::Alias(c) => c.selected_name(),
            Self::Custom(c) => c.name(),
        }
    }
    pub(super) fn base(&self) -> &str {
        match self {
            Self::Canonical(c) => c.base(),
            Self::Alias(c) => c.canonical_group(),
            Self::Custom(c) => c.name(),
        }
    }
    pub(super) fn glyph(&self) -> &str {
        match self {
            Self::Canonical(c) => c.glyph(),
            Self::Alias(c) => c.glyph(),
            // Literal content fallback; never a fabricated standard glyph.
            Self::Custom(c) => c.fallback(),
        }
    }
    pub(super) fn wire_name(&self) -> &str {
        match self {
            Self::Canonical(c) => c.name(),
            Self::Alias(c) => c.wire_name(),
            Self::Custom(c) => c.name(),
        }
    }
    pub(super) fn is_canonical(&self) -> bool {
        matches!(self, Self::Canonical(_))
    }
    pub(super) fn alias_matches(&self, query: &AliasQuery) -> bool {
        matches!(self,Self::Alias(c)if query.matches(c))
    }
    pub(super) fn select(
        &self,
        policy: &RootPolicy,
        prepared: &Prepared,
        membership: &RootMembership,
        user: &str,
    ) -> std::result::Result<Checked, &'static str> {
        match self {
            Self::Custom(c) => c.select(policy, membership, user).map(Checked::Custom),
            Self::Canonical(c) => membership
                .first_add_preferred_selection(
                    policy,
                    &prepared.domain,
                    &prepared.loaded.catalog,
                    policy.preferences(),
                    c,
                    user,
                )
                .map(Checked::Canonical),
            Self::Alias(c) => membership
                .first_add_alias_selection(
                    policy,
                    &prepared.alias_domain,
                    &prepared.loaded.catalog,
                    policy.preferences(),
                    c,
                    user,
                )
                .map(Checked::Alias),
        }
    }
}
pub(super) enum Checked {
    Canonical(PreferredToggle),
    Alias(AliasToggle),
    Custom(custom::Toggle),
}
impl Checked {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Canonical(c) => c.name(),
            Self::Alias(c) => c.name(),
            Self::Custom(c) => c.name(),
        }
    }
    pub(super) fn adding(&self) -> bool {
        match self {
            Self::Canonical(c) => c.adding(),
            Self::Alias(c) => c.adding(),
            Self::Custom(c) => c.adding(),
        }
    }
    pub(super) fn current(
        &self,
        policy: &RootPolicy,
        prepared: &Prepared,
        membership: &RootMembership,
        user: &str,
    ) -> bool {
        match self {
            Self::Custom(c) => c.current(policy, membership, user),
            Self::Canonical(c) => c.current(
                policy,
                &prepared.domain,
                &prepared.loaded.catalog,
                policy.preferences(),
                membership,
                user,
            ),
            Self::Alias(c) => c.current(
                policy,
                &prepared.alias_domain,
                &prepared.loaded.catalog,
                policy.preferences(),
                membership,
                user,
            ),
        }
    }
}
