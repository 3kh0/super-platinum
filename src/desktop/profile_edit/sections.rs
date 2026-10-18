//! One complete custom section per explicit save, sharing the profile owner's
//! captured credentials and visit. No background writes or readback replay.
use super::*;
pub(crate) use protocol::{SectionField, SectionValue};
use super_platinum_core::slack::api::custom as protocol;
mod directory;
pub(crate) mod fixture;
mod network;
pub(crate) use directory::{people, person_name, query_people, query_tags, tags};
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(crate) struct State {
    draft: Option<SectionPane>,
    entries: BTreeMap<Owner, SectionEntry>,
}
#[derive(Clone)]
struct SectionPane {
    token: Token,
    visit: u64,
    serial: u64,
    snapshot: Option<api::OwnProfileSections>,
    form: Option<protocol::SectionDraft>,
    loading: bool,
    message: Option<String>,
    directory: directory::Directory,
}
#[derive(Clone)]
struct SectionAttempt {
    id: u64,
    token: Token,
    visit: u64,
    serial: u64,
    intent: protocol::SectionIntent,
    phase: Phase,
    read: Option<u64>,
}
#[derive(Clone, Default)]
struct SectionEntry {
    pending: Option<SectionAttempt>,
    retry_at: i64,
    checked: Option<api::OwnProfileSections>,
    message: Option<String>,
}
pub(crate) struct View {
    pub visit: u64,
    pub serial: u64,
    pub sections: Vec<(String, String)>,
    pub form: Option<protocol::SectionDraft>,
    pub busy: bool,
    pub uncertain: bool,
    pub can_save: bool,
    pub can_check: bool,
    pub can_keep: bool,
    pub message: Option<String>,
}
fn current(shell: &ShellState, token: &Token, visit: u64, serial: u64) -> bool {
    draft_current(shell, token, visit)
        && shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .is_some_and(|d| d.token == *token && d.serial == serial && d.visit == visit)
}
pub(super) fn active(shell: &ShellState) -> bool {
    shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .is_some_and(|d| current(shell, &d.token, d.visit, d.serial))
}
pub(crate) fn pending(shell: &ShellState) -> bool {
    Owner::capture(shell)
        .and_then(|o| shell.profile_edit.sections.entries.get(&o))
        .is_some_and(|e| e.pending.is_some())
}
fn competing(shell: &ShellState, owner: &Owner) -> bool {
    recording::pending(shell)
        || photo::pending(shell)
        || crate::profile_title::blocks_profile_writes(shell)
        || crate::self_status::blocks_profile_edit(shell)
        || shell
            .profile_edit
            .entries
            .get(owner)
            .is_some_and(|e| e.pending.is_some())
}
pub(crate) fn presentation(shell: &ShellState) -> Option<View> {
    let d = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .filter(|d| current(shell, &d.token, d.visit, d.serial))?;
    let entry = shell.profile_edit.sections.entries.get(&d.token.owner);
    let pending = entry.and_then(|e| e.pending.as_ref());
    let busy = d.loading || pending.is_some_and(|a| a.phase != Phase::Uncertain);
    let cooling = entry.is_some_and(|e| e.retry_at > now());
    let sections = d
        .snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .sections()
                .iter()
                .filter_map(|s| {
                    let label = if s["type"] == "HEADER" {
                        if !protocol::SectionDraft::pronunciation_available(snapshot) {
                            return None;
                        }
                        "Name pronunciation"
                    } else {
                        s["label"].as_str().unwrap_or("Profile section")
                    };
                    Some((s["sectionId"].as_str()?.into(), label.into()))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(View {
        visit: d.visit,
        serial: d.serial,
        sections,
        form: d.form.clone(),
        busy,
        uncertain: pending.is_some(),
        can_save: !busy
            && !cooling
            && pending.is_none()
            && !competing(shell, &d.token.owner)
            && d.form.as_ref().is_some_and(|f| f.capture().is_ok()),
        can_check: !cooling && pending.is_some_and(|a| a.phase == Phase::Uncertain),
        can_keep: !busy && entry.is_some_and(|e| e.checked.is_some()),
        message: entry
            .and_then(|e| e.message.clone())
            .or_else(|| d.message.clone()),
    })
}
fn open_form(
    snapshot: &api::OwnProfileSections,
    id: &str,
) -> Result<protocol::SectionDraft, Error> {
    if snapshot
        .sections()
        .iter()
        .any(|s| s["sectionId"] == id && s["type"] == "HEADER")
    {
        protocol::SectionDraft::open_pronunciation(snapshot)
    } else {
        protocol::SectionDraft::open(snapshot, id)
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64) {
    let started = { open_shell(&mut state.write(), visit) };
    if let Some((token, serial)) = started {
        dioxus::core::spawn_forever(network::load(state, token, visit, serial));
    }
}
fn open_shell(shell: &mut ShellState, visit: u64) -> Option<(Token, u64)> {
    let d = shell
        .profile_edit
        .draft
        .as_ref()
        .filter(|d| d.visit == visit && draft_current(shell, &d.token, visit))?;
    let token = d.token.clone();
    if recording::active(shell)
        || recording::pending(shell)
        || photo::presentation(shell).is_some()
        || competing(shell, &token.owner)
        || (shell.profile_edit.sections.entries.len() >= 16
            && !shell
                .profile_edit
                .sections
                .entries
                .contains_key(&token.owner))
    {
        return None;
    }
    let entry = shell.profile_edit.sections.entries.get(&token.owner);
    let retained = entry.and_then(|e| e.pending.as_ref());
    let cooling = entry.is_some_and(|e| e.retry_at > now());
    let serial = next_id();
    shell.profile_edit.sections.draft = Some(SectionPane {
        token: token.clone(),
        visit,
        serial,
        snapshot: entry.and_then(|e| e.checked.clone()),
        form: retained.map(|a| a.intent.draft().clone()),
        loading: !cooling && retained.is_none(),
        message: cooling.then(|| "Slack asked us to wait before loading again.".into()),
        directory: Default::default(),
    });
    (!cooling && retained.is_none()).then_some((token, serial))
}
pub(crate) fn select(mut state: Signal<ShellState>, visit: u64, serial: u64, id: String) {
    let mut shell = state.write();
    if !presentation(&shell)
        .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
    {
        return;
    }
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    let Some(snapshot) = d.snapshot.as_ref() else {
        return;
    };
    match open_form(snapshot, &id) {
        Ok(form) => {
            d.form = Some(form);
            d.serial = next_id();
            d.message = None;
        }
        Err(_) => d.message = Some("This profile section cannot be opened safely.".into()),
    }
}
pub(crate) fn change(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
    value: SectionValue,
) {
    change_shell(&mut state.write(), visit, serial, &id, value);
}
fn change_shell(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    value: SectionValue,
) -> bool {
    if !presentation(shell)
        .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
    {
        return false;
    }
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    let changed = d.form.as_mut().is_some_and(|f| f.change(id, value).is_ok());
    if changed {
        d.message = None;
    }
    changed
}
pub(crate) fn use_description(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: &str,
    blocks: Vec<Value>,
) -> bool {
    change_shell(
        &mut state.write(),
        visit,
        serial,
        id,
        SectionValue::LongText(blocks),
    )
}
pub(crate) fn save(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let started = { begin(&mut state.write(), visit, serial) };
    if let Some(a) = started {
        dioxus::core::spawn_forever(network::save(state, a));
    }
}
fn begin(shell: &mut ShellState, visit: u64, serial: u64) -> Option<SectionAttempt> {
    if !presentation(shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_save) {
        return None;
    }
    let d = shell.profile_edit.sections.draft.as_ref()?;
    let a = SectionAttempt {
        id: next_id(),
        token: d.token.clone(),
        visit,
        serial,
        intent: d.form.as_ref()?.capture().ok()?,
        phase: Phase::Preparing,
        read: None,
    };
    let e = shell
        .profile_edit
        .sections
        .entries
        .entry(a.token.owner.clone())
        .or_default();
    if e.pending.is_some() || e.retry_at > now() {
        return None;
    }
    e.checked = None;
    e.message = None;
    e.pending = Some(a.clone());
    Some(a)
}
fn entry_mut<'a>(shell: &'a mut ShellState, a: &SectionAttempt) -> Option<&'a mut SectionEntry> {
    shell
        .profile_edit
        .sections
        .entries
        .get_mut(&a.token.owner)
        .filter(|e| {
            e.pending
                .as_ref()
                .is_some_and(|p| p.id == a.id && p.token == a.token)
        })
}
pub(crate) fn check(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let started = { begin_check(&mut state.write(), visit, serial) };
    if let Some(a) = started {
        dioxus::core::spawn_forever(network::check(state, a));
    }
}
fn begin_check(shell: &mut ShellState, visit: u64, serial: u64) -> Option<SectionAttempt> {
    if !presentation(shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_check) {
        return None;
    }
    let token = shell.profile_edit.sections.draft.as_ref()?.token.clone();
    let entry = shell.profile_edit.sections.entries.get_mut(&token.owner)?;
    let a = entry.pending.as_mut()?;
    // New read authority may reattach only under the same owner; the pending
    // write's frozen intent is never rewritten or dispatched again.
    a.token = token;
    a.visit = visit;
    a.serial = serial;
    a.phase = Phase::Checking;
    a.read = Some(next_id());
    entry.checked = None;
    Some(a.clone())
}
pub(crate) fn keep(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_keep) {
        return;
    }
    let owner = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    shell
        .profile_edit
        .sections
        .entries
        .get_mut(&owner)
        .unwrap()
        .pending = None;
    shell.profile_edit.sections.draft = None;
}
pub(crate) fn back(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy) {
        return;
    }
    shell.profile_edit.sections.draft = None;
}
pub(super) fn intercept_close(shell: &mut ShellState, visit: Option<u64>) -> bool {
    let Some(d) = shell.profile_edit.sections.draft.as_ref() else {
        return false;
    };
    if visit.is_some_and(|v| v != d.visit) {
        return true;
    }
    if let Some(e) = shell.profile_edit.sections.entries.get_mut(&d.token.owner) {
        if e.pending
            .as_ref()
            .is_some_and(|a| matches!(a.phase, Phase::Saving | Phase::Checking))
        {
            return true;
        }
        if e.pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            e.pending = None;
        }
    }
    shell.profile_edit.sections.draft = None;
    false
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    poll_at(shell, now())
}
fn poll_at(shell: &mut ShellState, timestamp: i64) -> bool {
    let stale = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .is_some_and(|d| !current(shell, &d.token, d.visit, d.serial));
    if stale {
        shell.profile_edit.sections.draft = None;
    }
    let retired: Vec<_> = shell
        .profile_edit
        .sections
        .entries
        .iter()
        .filter(|(_, e)| {
            e.pending.as_ref().is_some_and(|a| {
                !a.token.current(shell)
                    || (matches!(a.phase, Phase::Preparing | Phase::Checking)
                        && !current(shell, &a.token, a.visit, a.serial))
            })
        })
        .map(|(o, _)| o.clone())
        .collect();
    for owner in retired {
        let e = shell.profile_edit.sections.entries.get_mut(&owner).unwrap();
        e.checked = None;
        if e.pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            e.pending = None;
        } else if let Some(a) = e.pending.as_mut().filter(|a| a.phase == Phase::Checking) {
            a.phase = Phase::Uncertain;
            a.read = None;
        }
    }
    let visible = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .map(|d| d.token.owner.clone());
    let before = shell.profile_edit.sections.entries.len();
    let mut expired = false;
    for e in shell.profile_edit.sections.entries.values_mut() {
        if e.retry_at != 0 && e.retry_at <= timestamp {
            e.retry_at = 0;
            expired = true;
        }
    }
    shell.profile_edit.sections.entries.retain(|o, e| {
        visible.as_ref() == Some(o) || e.pending.is_some() || e.retry_at > timestamp
    });
    stale || expired || before != shell.profile_edit.sections.entries.len()
}
