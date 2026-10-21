//! Existing own-family removal (retained jGtm reaction_toggle_button branch).
//! Raw base identity is deliberately not canonical alias identity. The host
//! schedules child requests and owns their outcomes, account/visit/socket gates,
//! and any reconciliation of its own already-issued removals. No atomic remote
//! operation, optimistic inverse, new reaction, or preferred tone is implied.
use super::*;
mod names;

/// A fixed ordered set of single-use child intents. There is no reconstruction
/// from the inert record and no Clone implementation on sendable work.
pub struct ReactionFamilyIntent {
    record: ReactionFamilyRecord,
    pending: std::collections::VecDeque<ReactionIntent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionFamilyRecord {
    selected: String,
    base: String,
    children: Vec<ReactionRecord>,
}

/// Exact retained V17 standard base/variant names; V16 agreement is tested.
/// This is name metadata, independent of glyph resolution. No lowercasing,
/// outer-colon removal, alias canonicalization, or permissive ue25 regex repair
/// is accepted as authority for a name to send.
pub fn reaction_family_base(name: &str) -> Option<&str> {
    if let Some((base, suffix)) = name.split_once("::skin-tone-") {
        let index = names::TONES
            .binary_search_by_key(&base, |(base, _)| *base)
            .ok()?;
        names::TONES[index]
            .1
            .split_ascii_whitespace()
            .any(|s| s == suffix)
            .then_some(base)
    } else {
        names::BASES.binary_search(&name).ok().map(|_| name)
    }
}

/// Ordered explicitly observed self memberships for an existing selected pill.
/// Partial lists of OTHER users do not invalidate positive self membership.
/// An empty result means no observed own child, never permission to add one.
/// This helper does not establish target, policy, owner or visible-row authority.
pub fn own_reaction_family_names(
    message: &Message,
    user: &str,
    selected: &str,
) -> Result<Vec<String>, ListError> {
    let base =
        reaction_family_base(selected).ok_or(ListError("This reaction family is unavailable."))?;
    if !identity(user) {
        return Err(ListError("List comment reaction membership is ambiguous."));
    }
    let prefix = format!("{base}::");
    let mut seen = HashSet::new();
    let mut selected_present = false;
    let mut own = Vec::new();
    for reaction in &message.reactions {
        let users: HashSet<_> = reaction.users.iter().map(String::as_str).collect();
        if !reaction_name(&reaction.name)
            || !seen.insert(&reaction.name)
            || users.len() != reaction.users.len()
            || reaction.users.iter().any(|u| !identity(u))
            || reaction.count < users.len() as u32
        {
            return Err(ListError("List comment reaction membership is ambiguous."));
        }
        let candidate = reaction_family_base(&reaction.name);
        if candidate.is_none() && reaction.name.starts_with(&prefix) {
            return Err(ListError(
                "This reaction family contains an unsupported name.",
            ));
        }
        selected_present |= reaction.name == selected;
        if candidate == Some(base) && users.contains(user) {
            own.push(reaction.name.clone());
        }
    }
    if !selected_present {
        return Err(ListError("The original reaction pill is unavailable."));
    }
    Ok(own)
}

impl ReactionFamilyIntent {
    pub fn capture(
        target: &Target,
        root: &Message,
        message: &Message,
        channel: Value,
        user: &str,
        selected: &str,
    ) -> Result<Self, ListError> {
        let names = own_reaction_family_names(message, user, selected)?;
        if names.is_empty() {
            return Err(ListError("There is no observed own reaction to remove."));
        }
        let mut children = Vec::with_capacity(names.len());
        let mut pending = std::collections::VecDeque::with_capacity(names.len());
        for name in names {
            let child = ReactionIntent::capture_inner(
                target,
                root,
                message,
                channel.clone(),
                user,
                &name,
                Some(selected),
                None,
            )?;
            children.push(child.record.clone());
            pending.push_back(child);
        }
        Ok(Self {
            record: ReactionFamilyRecord {
                selected: selected.into(),
                base: reaction_family_base(selected).unwrap().into(),
                children,
            },
            pending,
        })
    }
    pub fn record(&self) -> &ReactionFamilyRecord {
        &self.record
    }
    /// Taking a slot does not assert dispatch or success. The host must retain
    /// issued/unsent/settled outcomes and never replay a consumed slot.
    pub fn take_next(&mut self) -> Option<ReactionIntent> {
        self.pending.pop_front()
    }
}

impl ReactionFamilyRecord {
    pub fn selected(&self) -> &str {
        &self.selected
    }
    pub fn base(&self) -> &str {
        &self.base
    }
    pub fn children(&self) -> &[ReactionRecord] {
        &self.children
    }
    /// Exact ORIGINAL observation comparison, not account/visit authority or a
    /// permit to abandon remaining siblings after our own socket/ACK removals.
    /// The host separately tracks those admitted effects while retaining these
    /// fixed original child names; this method never recaptures a successor set.
    pub fn current(
        &self,
        target: &Target,
        root: &Message,
        message: &Message,
        channel: &Value,
    ) -> bool {
        ReactionFamilyIntent::capture(
            target,
            root,
            message,
            channel.clone(),
            self.children[0].user(),
            &self.selected,
        )
        .is_ok_and(|intent| intent.record == *self)
    }
}

#[cfg(test)]
mod tests;
