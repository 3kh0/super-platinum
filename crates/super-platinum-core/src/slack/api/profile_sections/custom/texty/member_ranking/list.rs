//! Ordinary List aD6p contextual sort observations. Acquisition and freshness
//! belong to the original List query owner, never the selected Home channel.
use super::*;
use std::collections::BTreeMap;

/// These inputs are observed selector results, not optional readiness defaults.
/// `None` means the channel selector was observed to return no row. An empty
/// thread slice means the original root's `reply_users || []`, not reply authors.
/// Membership values are original 1IsG.B selector observations under the same
/// owner, not known-membership claims. That selector returns false for observed
/// absent/pending cache entries as well as known nonmembers. A missing map entry
/// means the selector itself was not observed and refuses a present channel.
pub struct ListContext<'a> {
    pub current_channel: Option<&'a Value>,
    pub current_thread_users: &'a [String],
    pub channel_membership: &'a BTreeMap<String, bool>,
}

impl ListContext<'_> {
    pub(super) fn options(&self, demote: bool) -> Result<shared::Options, Error> {
        let channel = self
            .current_channel
            .map(|raw| {
                if raw["id"].as_str().is_none_or(str::is_empty) {
                    return Err(Error::InvalidInput);
                }
                // Neither channel nor member wire `facts` is authority.
                directory_selectors::item(
                    raw,
                    directory_selectors::Names {
                        comparison: "",
                        mpim: None,
                        custom: None,
                        custom_allowed: false,
                    },
                )
                .ok_or(Error::InvalidInput)
            })
            .transpose()?;
        Ok(shared::Options {
            limit: 25,
            source: "texty-autocomplete".into(),
            demote_passthrough: demote,
            prefer_current_channel_members: channel.is_some(),
            current_channel: channel,
            current_thread_users: Some(self.current_thread_users.to_vec()),
            ..Default::default()
        })
    }

    pub(super) fn apply_membership(&self, item: &mut Value, id: &str) -> Result<(), Error> {
        if self.current_channel.is_some() {
            let member = self.channel_membership.get(id).ok_or_else(|| {
                Error::MissingObservation("List current-channel membership".into())
            })?;
            item["facts"]["currentChannelMember"] = json!(member);
        }
        Ok(())
    }
}

/// The same member matching, selector projection, initial 75 and final 25 as
/// Profile, with the original List channel/thread contextual ranking inputs.
/// This does not acquire memberships, infer eligibility, or authorize insertion.
pub fn rank_list_members(
    query: &str,
    matched: &[local::Scored<'_>],
    list: &ListContext<'_>,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    rank_list_mixed(
        query,
        matched,
        &[],
        list,
        context,
        cache,
        comparison,
        collate,
        demote_passthrough_observed,
    )
}

/// Members then effective group occurrences share the original index space and
/// the single 75/25 ranking limits, with the original List channel/root context.
/// Group matching and eligibility remain independently observed by the caller.
pub fn rank_list_mixed(
    query: &str,
    matched: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    list: &ListContext<'_>,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    rank_list_mixed_files(
        query,
        matched,
        groups,
        &[],
        list,
        context,
        cache,
        comparison,
        collate,
        demote_passthrough_observed,
    )
}

/// Member, effective group, then effective file occurrences share one index
/// space and the single 75/25 limits. Files use the existing observed selector
/// and feature adapter; List channel/root context still applies to members.
/// This does not change category eligibility or grant insertion authority.
pub fn rank_list_mixed_files(
    query: &str,
    matched: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    files: &[super::super::file_matching::FileScored<'_>],
    list: &ListContext<'_>,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    rank_inner(
        query,
        matched,
        groups,
        files,
        context,
        cache,
        comparison,
        collate,
        demote_passthrough_observed,
        Some(list),
    )
}

#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod tests;
