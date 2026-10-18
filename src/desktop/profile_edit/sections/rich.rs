//! Channel/user/group labels are admitted once from checked source records.
//! The mounted draft retains that label custody even when the directory changes.
use super::*;
use crate::app_home::channel_fetches::canonical;
use serde::Deserialize;
use serde_json::Value;
mod catalog;
pub(crate) mod files;
#[cfg(test)]
mod group_tests;
pub(crate) mod opening;
pub(crate) mod query;
pub(crate) use query::fixture_choices;
mod recovery;
pub(crate) mod search;
pub(super) use recovery::lease::Lease as RecoveryLease;
pub(super) fn retire_recovery(d: &mut SectionPane) {
    if let Some(lease) = d.rich_recovery.take() {
        lease.retire();
    }
    d.rich_reading = false;
}
#[cfg(test)]
mod user_tests;
pub(crate) use opening::{begin, finish};

#[derive(Clone)]
pub(crate) struct Session {
    id: String,
    key: u64,
    before: Vec<Value>,
    catalog: catalog::Catalog,
    labels: String,
    notice: Option<String>,
    query: Option<query::Choices>,
    preferences: Option<Arc<search::ranking::Preferences>>,
}
/// Shared recovery calls this only for an attempt whose original publication,
/// account/session and socket witnesses are still current, before retiring it.
pub(crate) fn refresh_failed(
    shell: &mut ShellState,
    account: &str,
    team: &str,
    user: &str,
    ids: &[String],
) -> bool {
    let Some(d) = shell.profile_edit.sections.draft.as_ref() else {
        return false;
    };
    if d.token.owner.account != account
        || d.token.owner.team != team
        || d.token.owner.user != user
        || !editable(shell, d.visit, d.serial)
    {
        return false;
    }
    let Some(session) = d.rich.as_ref() else {
        return false;
    };
    if !session.catalog.contains_any(ids)
        || d.form.as_ref().and_then(|f| f.value(&session.id))
            != Some(&SectionValue::LongText(session.before.clone()))
    {
        return false;
    }
    shell
        .profile_edit
        .sections
        .draft
        .as_mut()
        .unwrap()
        .rich
        .as_mut()
        .unwrap()
        .notice = Some("Channel metadata could not refresh.".into());
    true
}
pub(super) fn notice(session: &Session) -> Option<String> {
    session.notice.clone()
}
pub(crate) fn has_files(blocks: &[Value]) -> bool {
    collect(blocks).is_ok_and(|tokens| {
        tokens
            .iter()
            .any(|raw| matches!(raw["type"].as_str(), Some("file" | "canvas")))
    })
}
pub(crate) fn open(state: Signal<ShellState>, visit: u64, serial: u64, id: &str, key: u64) -> bool {
    open_with_recovery(state, visit, serial, id, key, false)
}
/// Explicit Retry reads unavailable known members and group details.
/// Ordinary opening schedules shared automatic member work without forcing it.
pub(crate) fn retry(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
) -> bool {
    open_with_recovery(state, visit, serial, id, key, true)
}
pub(crate) fn recovering(shell: &ShellState, visit: u64, serial: u64) -> bool {
    shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .is_some_and(|d| current(shell, &d.token, visit, serial) && d.rich_reading)
}
fn open_with_recovery(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
    known: bool,
) -> bool {
    if recovering(&state.peek(), visit, serial) {
        return false;
    }
    let (accepted, hits, recovery) = {
        let mut shell = state.write();
        let (opening, hits) =
            opening::tracked_with_members(&mut shell, visit, serial, id, key, !known);
        let accepted = match opening {
            Ok(opening) => finish(&mut shell, opening),
            Err(_) => false,
        };
        let recovery = (!accepted)
            .then(|| recovery::MissingOpening::capture(&shell, visit, serial, id, key))
            .flatten();
        (accepted, hits, recovery)
    };
    let automatic_members = hits.iter().any(opening::Hit::is_member);
    for hit in hits {
        hit.schedule(state);
    }
    if let Some(recovery) = recovery {
        recovery.start(state, known, automatic_members);
    }
    accepted
}
pub(crate) fn labels(
    shell: &ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
) -> Option<String> {
    let d = shell.profile_edit.sections.draft.as_ref()?;
    let session = d.rich.as_ref()?;
    (current(shell, &d.token, visit, serial) && session.id == id && session.key == key)
        .then(|| session.labels.clone())
}
pub(crate) fn mount_current(shell: &ShellState, visit: u64, serial: u64, key: u64) -> bool {
    editable(shell, visit, serial)
        && shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .and_then(|d| d.rich.as_ref())
            .is_some_and(|s| s.key == key)
}
pub(crate) fn cancel(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if editable(&shell, visit, serial) {
        let d = shell.profile_edit.sections.draft.as_mut().unwrap();
        retire_recovery(d);
        d.rich = None;
        d.rich_opening = next_id();
        d.rich_reading = false;
    }
}
pub(crate) fn accept(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
    capture: Capture,
) -> bool {
    accept_shell(&mut state.write(), visit, serial, id, key, capture)
}
pub(crate) fn accept_shell(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
    capture: Capture,
) -> bool {
    if !editable(shell, visit, serial) || capture.validate(serial).is_err() {
        return false;
    }
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    let Some(session) = d.rich.as_ref().filter(|s| s.id == id && s.key == key) else {
        return false;
    };
    let Some(form) = d.form.as_mut() else {
        return false;
    };
    if form.value(id) != Some(&SectionValue::LongText(session.before.clone())) {
        return false;
    }
    let Ok(context) = session.catalog.context(&capture.tokens, &d.token.owner) else {
        return false;
    };
    if form
        .change_rich(id, capture.snapshot.blocks, context)
        .is_err()
    {
        return false;
    }
    d.rich = None;
    d.message = None;
    true
}
fn editable(shell: &ShellState, visit: u64, serial: u64) -> bool {
    shell.profile_edit.sections.draft.as_ref().is_some_and(|d| {
        current(shell, &d.token, visit, serial)
            && !d.loading
            && shell
                .profile_edit
                .sections
                .entries
                .get(&d.token.owner)
                .is_none_or(|e| e.pending.is_none())
    })
}
fn unavailable() -> Error {
    Error::Api("profile_channel_label_unavailable".into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capture {
    snapshot: NativeSnapshot,
    tokens: Vec<catalog::Witness>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeSnapshot {
    blocks: Vec<Value>,
    revision: u64,
    composing: bool,
    generation: String,
}
impl Capture {
    fn validate(&self, serial: u64) -> Result<(), Error> {
        if self.snapshot.composing
            || self.snapshot.revision >= 9_007_199_254_740_991
            || self.snapshot.generation != serial.to_string()
        {
            return Err(unavailable());
        }
        let tokens = collect(&self.snapshot.blocks)?;
        if tokens.len() != self.tokens.len()
            || tokens.iter().zip(&self.tokens).any(|(a, b)| *a != &b.value)
            || serde_json::to_vec(&self.snapshot.blocks)
                .map_err(|_| unavailable())?
                .len()
                > 1_048_576
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
fn collect(values: &[Value]) -> Result<Vec<&Value>, Error> {
    fn walk<'a>(
        values: &'a [Value],
        depth: usize,
        budget: &mut usize,
        out: &mut Vec<&'a Value>,
    ) -> Result<(), Error> {
        if depth > 32 {
            return Err(unavailable());
        }
        for v in values {
            *budget = budget.checked_sub(1).ok_or_else(unavailable)?;
            match v["type"].as_str() {
                Some(
                    "rich_text"
                    | "rich_text_section"
                    | "rich_text_quote"
                    | "rich_text_preformatted"
                    | "rich_text_list",
                ) => walk(
                    v["elements"].as_array().ok_or_else(unavailable)?,
                    depth + 1,
                    budget,
                    out,
                )?,
                Some("text" | "link") => {}
                Some(_) => out.push(v),
                None => return Err(unavailable()),
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(values, 0, &mut 10_000, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn plain() -> (ShellState, u64, u64) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install_interaction(&mut shell);
        let d = shell.profile_edit.sections.draft.as_ref().unwrap();
        let (visit, serial) = (d.visit, d.serial);
        (shell, visit, serial)
    }
    fn mounted_notice() -> (ShellState, u64, u64, Owner) {
        let (mut shell, visit, serial) = plain();
        let token = json!({"type":"channel","channel_id":"C12345678","unknown":1});
        let blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[token]}]}),
        ];
        assert!(change_shell(
            &mut shell,
            visit,
            serial,
            "long",
            SectionValue::LongText(blocks.clone())
        ));
        let mut catalog = catalog::Catalog::default();
        catalog.insert(&token, "#retained-name".into()).unwrap();
        let pane = shell.profile_edit.sections.draft.as_mut().unwrap();
        pane.rich = Some(Session {
            id: "long".into(),
            key: 1,
            before: blocks,
            catalog,
            labels: "retained-labels".into(),
            notice: None,
            query: None,
            preferences: None,
        });
        let owner = pane.token.owner.clone();
        (shell, visit, serial, owner)
    }
    #[test]
    fn metadata_failure_notice_only_reaches_current_relevant_editor() {
        let (mut shell, visit, serial, owner) = mounted_notice();
        let ids = vec!["C12345678".into()];
        let before = shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value("long")
            .cloned();
        assert!(!refresh_failed(
            &mut shell,
            "foreign",
            &owner.team,
            &owner.user,
            &ids
        ));
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            "foreign",
            &owner.user,
            &ids
        ));
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            "foreign",
            &ids
        ));
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &["COTHER000".into()]
        ));
        assert!(refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &ids
        ));
        assert_eq!(
            presentation(&shell).unwrap().message.as_deref(),
            Some("Channel metadata could not refresh.")
        );
        assert!(
            before
                == shell
                    .profile_edit
                    .sections
                    .draft
                    .as_ref()
                    .unwrap()
                    .form
                    .as_ref()
                    .unwrap()
                    .value("long")
                    .cloned()
        );
        assert_eq!(
            labels(&shell, visit, serial, "long", 1).as_deref(),
            Some("retained-labels")
        );
        shell.core.account_epoch += 1;
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &ids
        ));
        assert!(presentation(&shell).is_none());
        let (mut shell, _, _, owner) = mounted_notice();
        shell.profile_generation += 1;
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &ids
        ));
    }
    #[test]
    fn metadata_notice_cannot_replace_pending_write_recovery() {
        let (mut shell, visit, serial, owner) = mounted_notice();
        assert!(refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &["C12345678".into()]
        ));
        let _attempt = crate::profile_edit::sections::begin(&mut shell, visit, serial).unwrap();
        shell
            .profile_edit
            .sections
            .entries
            .get_mut(&owner)
            .unwrap()
            .message = Some("Keep the pending write recovery.".into());
        assert!(!refresh_failed(
            &mut shell,
            &owner.account,
            &owner.team,
            &owner.user,
            &["C12345678".into()]
        ));
        assert_eq!(
            presentation(&shell).unwrap().message.as_deref(),
            Some("Keep the pending write recovery.")
        );
    }
    #[test]
    fn plain_opening_needs_no_directory_and_new_opening_retires_predecessor() {
        let (mut shell, visit, serial) = plain();
        assert!(shell.core.transport.is_none());
        let older = begin(&mut shell, visit, serial, "long", 1).unwrap();
        let newer = begin(&mut shell, visit, serial, "long", 2).unwrap();
        assert!(!finish(&mut shell, older));
        assert!(finish(&mut shell, newer));
        assert!(labels(&shell, visit, serial, "long", 2).is_some());
        assert!(labels(&shell, visit, serial, "long", 1).is_none());
    }
    #[test]
    fn plain_and_missing_source_openings_do_not_schedule_channel_recovery() {
        let (mut shell, visit, serial) = plain();
        let (plain, hits) = opening::tracked(&mut shell, visit, serial, "long", 1);
        assert!(plain.is_ok());
        assert!(hits.is_empty());
        let blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"channel","channel_id":"C12345678"}]}]}),
        ];
        assert!(change_shell(
            &mut shell,
            visit,
            serial,
            "long",
            SectionValue::LongText(blocks)
        ));
        let (missing, hits) = opening::tracked(&mut shell, visit, serial, "long", 2);
        assert!(missing.is_err());
        assert!(hits.is_empty());
    }
    #[test]
    fn opening_refuses_changed_field_and_account() {
        let (mut shell, visit, serial) = plain();
        let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
        assert!(change_shell(
            &mut shell,
            visit,
            serial,
            "long",
            SectionValue::LongText(vec![])
        ));
        assert!(!finish(&mut shell, opening));
        let opening = begin(&mut shell, visit, serial, "long", 2).unwrap();
        shell.core.account_epoch += 1;
        assert!(!finish(&mut shell, opening));
    }
    #[test]
    fn unavailable_channel_opening_retains_original_description() {
        let (mut shell, visit, serial) = plain();
        let blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"channel","channel_id":"C12345678","unknown":7}]}]}),
        ];
        assert!(change_shell(
            &mut shell,
            visit,
            serial,
            "long",
            SectionValue::LongText(blocks.clone())
        ));
        assert!(begin(&mut shell, visit, serial, "long", 1).is_err());
        assert!(
            shell
                .profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .form
                .as_ref()
                .unwrap()
                .value("long")
                == Some(&SectionValue::LongText(blocks))
        );
    }
    #[test]
    fn capture_requires_complete_ordered_raw_witnesses_and_generation() {
        let first = json!({"type":"channel","channel_id":"C12345678","unknown":1});
        let second = json!({"type":"channel","channel_id":"C12345678","unknown":2});
        let raw = json!({"snapshot":{"blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[first,second]}]}],"revision":4,"composing":false,"generation":"7"},
            "tokens":[{"value":first,"label":"#original"},{"value":second,"label":"#original"}]});
        let good: Capture = serde_json::from_value(raw.clone()).unwrap();
        assert!(good.validate(7).is_ok());
        assert!(good.validate(8).is_err());
        let mut reordered = raw.clone();
        reordered["tokens"].as_array_mut().unwrap().swap(0, 1);
        assert!(
            serde_json::from_value::<Capture>(reordered)
                .unwrap()
                .validate(7)
                .is_err()
        );
        let mut omitted = raw;
        omitted["tokens"].as_array_mut().unwrap().pop();
        assert!(
            serde_json::from_value::<Capture>(omitted)
                .unwrap()
                .validate(7)
                .is_err()
        );
    }
}
