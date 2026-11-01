//! Private topic reading needs its own original privacy input and canonical row.
//! A title, a default flag or directory metadata supplies neither observation.
use super::*;

pub(super) fn project(s: &ShellState) -> Option<Topic> {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        || !s.signed_in
        || s.loading
        || !s.detail_open()
        || s.core.screen != super_platinum_core::state::Screen::Main
    {
        return None;
    }
    let team = s.core.active_team.as_deref()?;
    let id = s.core.active_channel.as_deref()?;
    let vm = s.channels.get(s.active_channel)?;
    let native = s.core.workspaces.get(team)?.channels.get(id)?;
    let name = native
        .name
        .as_deref()
        .filter(|v| !v.is_empty() && v.len() <= MAX_BYTES)?;
    if native.id != id
        || vm.id != id
        || vm.name != name
        || vm.is_im
        || vm.is_mpim
        || !vm.is_private
        || vm.is_private != (native.is_private || native.is_group)
        || vm.is_ext_shared != native.is_ext_shared
        || native.is_archived
    {
        return None;
    }
    let privacy = crate::app_home::channel_fetches::header_privacy_inactive(s, team)?;
    let source = canonical::observed(s, team).ok()?;
    if !privacy.current(s) || !source.current_for(s, team) {
        return None;
    }
    let raw = source.row(id)?;
    if !super::super::privacy::qualified(raw, native)
        || raw.get("is_member") != Some(&Value::Bool(true))
        || native.extra.get("is_member") != Some(&Value::Bool(true))
        || raw.get("is_archived") != Some(&Value::Bool(false))
    {
        return None;
    }
    let topic = raw.get("topic")?;
    if native.extra.get("topic") != Some(topic) {
        return None;
    }
    let value = text(topic)?;
    let result = Topic {
        raw: value.to_owned(),
        runs: passive_topic(value),
        provenance: Provenance::Owned,
    };
    (privacy.current(s) && source.current_for(s, team)).then_some(result)
}

pub(super) fn fixture_model(s: &ShellState) -> Option<Topic> {
    // This is the existing disconnected private layout, never either live input.
    super::super::privacy::fixture_model(s)?;
    super::fixture_model(s)
}

pub(crate) fn fixture(s: &mut ShellState) {
    super::super::privacy::fixture(s);
    super::fixture(s);
}

#[cfg(test)]
mod tests;
