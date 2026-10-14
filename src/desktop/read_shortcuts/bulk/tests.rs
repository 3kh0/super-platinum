use super::*;
use serde_json::json;
use super_platinum_core::state::CONFIRM_CLEAR_UNREADS;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn pref(s: &mut ShellState, value: Option<serde_json::Value>) {
    let raw = value
        .map(|v| BTreeMap::from([(CONFIRM_CLEAR_UNREADS.into(), v)]))
        .unwrap_or_default();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .clear_unreads_preference = ClearUnreadsPreference::from_prefs(&raw);
}
fn clear(s: &mut ShellState) -> Work {
    pref(s, Some(json!(false)));
    match begin(s).unwrap() {
        Next::Bulk(w) => w,
        _ => panic!("expected clear"),
    }
}
fn ack(s: &mut ShellState, w: &Work, undo: bool) {
    let raw = if undo {
        json!({"ok":true,"undo_ts":"1719800700.000001"})
    } else {
        json!({"ok":true})
    };
    let id = finish_write(s, w, Ok(raw)).unwrap();
    finish_reconcile(s, &w.context, id, true);
}
#[test]
fn true_requires_confirmation_and_cancel_dispatches_nothing() {
    let mut s = shell();
    assert!(begin(&mut s).is_none());
    assert!(blocks_reads(&s));
    assert!(begin_individual(&mut s).is_none());
    assert!(
        s.bulk_read
            .dialog
            .as_ref()
            .is_some_and(|d| d.phase == DialogPhase::Confirm)
    );
    assert!(s.bulk_read.entries.values().all(|e| e.operation.is_none()));
    s.bulk_read.dialog = None;
    assert!(begin(&mut s).is_none());
}
#[test]
fn missing_authoritative_pref_bypasses_but_unhydrated_and_malformed_load() {
    let mut s = shell();
    pref(&mut s, None);
    assert!(matches!(begin(&mut s), Some(Next::Bulk(_))));
    let mut s = shell();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .clear_unreads_preference = ClearUnreadsPreference::default();
    assert!(matches!(begin(&mut s), Some(Next::Preference(_))));
    let mut s = shell();
    pref(&mut s, Some(json!("false")));
    assert!(matches!(begin(&mut s), Some(Next::Preference(_))));
}
#[test]
fn confirmation_revalidates_visit_and_exact_pref() {
    let mut s = shell();
    begin(&mut s);
    let id = s.bulk_read.dialog.as_ref().unwrap().id;
    pref(&mut s, Some(json!(false)));
    assert!(confirm_shell(&mut s, id).0.is_none());
    assert!(s.bulk_read.dialog.as_ref().unwrap().phase == DialogPhase::Unavailable);
    let mut s = shell();
    begin(&mut s);
    let id = s.bulk_read.dialog.as_ref().unwrap().id;
    s.channel_generation += 1;
    assert!(confirm_shell(&mut s, id).0.is_none());
    assert!(poll(&mut s));
    assert!(s.bulk_read.dialog.is_none());
}
#[test]
fn one_clear_reservation_blocks_duplicate_and_individual_actions() {
    let mut s = shell();
    let w = clear(&mut s);
    assert!(sending(&s, &w));
    assert!(blocks_reads(&s));
    assert!(begin(&mut s).is_none());
    assert!(begin_individual(&mut s).is_none());
    cancel_unsent(&mut s, &w);
    let id = begin_individual(&mut s).unwrap();
    assert!(begin(&mut s).is_none());
    end_individual(&mut s, id);
    assert!(matches!(begin(&mut s), Some(Next::Bulk(_))));
}
#[test]
fn unknown_and_contradictory_ack_never_replay_even_after_refresh() {
    for raw in [
        None,
        Some(json!({"ok":false,"error":"invalid_auth","undo_ts":"1719800700.000001"})),
    ] {
        let mut s = shell();
        let w = clear(&mut s);
        finish_write(&mut s, &w, raw.ok_or(Error::Transport("lost".into())));
        assert!(
            s.bulk_read.entries[&w.context.owner]
                .operation
                .as_ref()
                .unwrap()
                .phase
                == Phase::Unknown
        );
        let id = start_reconcile(&mut s, &w.context).unwrap();
        finish_reconcile(&mut s, &w.context, id, true);
        assert!(begin(&mut s).is_none());
        assert!(begin_undo(&mut s, Instant::now()).is_none());
    }
}
#[test]
fn clean_rejection_releases_reservation_but_429_keeps_it() {
    let mut s = shell();
    let w = clear(&mut s);
    finish_write(&mut s, &w, Ok(json!({"ok":false,"error":"invalid_auth"})));
    assert!(s.bulk_read.entries[&w.context.owner].operation.is_none());
    assert!(begin(&mut s).is_some());
    let mut s = shell();
    let w = clear(&mut s);
    finish_write(
        &mut s,
        &w,
        Err(Error::RateLimited {
            retry_after_secs: Some(2),
        }),
    );
    assert!(begin(&mut s).is_none());
}
#[test]
fn undo_is_one_use_six_seconds_and_never_chains() {
    let mut s = shell();
    let w = clear(&mut s);
    ack(&mut s, &w, true);
    let started = s.bulk_read.entries[&w.context.owner]
        .notice
        .as_ref()
        .unwrap()
        .started;
    let undo = begin_undo(&mut s, started + Duration::from_secs(5)).unwrap();
    assert!(undo.kind == Kind::Undo);
    assert!(begin_undo(&mut s, started).is_none());
    ack(&mut s, &undo, true);
    assert!(
        s.bulk_read.entries[&w.context.owner]
            .notice
            .as_ref()
            .unwrap()
            .receipt
            .is_none()
    );
    let mut s = shell();
    let w = clear(&mut s);
    ack(&mut s, &w, true);
    let started = s.bulk_read.entries[&w.context.owner]
        .notice
        .as_ref()
        .unwrap()
        .started;
    assert!(begin_undo(&mut s, started + Duration::from_secs(6)).is_none());
}
#[test]
fn owner_rotation_discards_undo_and_stale_ack_never_refreshes_new_owner() {
    let mut s = shell();
    let w = clear(&mut s);
    ack(&mut s, &w, true);
    s.core.account_epoch += 1;
    assert!(begin_undo(&mut s, Instant::now()).is_none());
    assert!(poll(&mut s));
    assert!(s.bulk_read.entries[&w.context.owner].notice.is_none());
    let mut s = shell();
    let w = clear(&mut s);
    s.core.account_epoch += 1;
    assert!(
        finish_write(
            &mut s,
            &w,
            Ok(json!({"ok":true,"undo_ts":"1719800700.000001"}))
        )
        .is_none()
    );
    assert!(s.bulk_read.entries[&w.context.owner].notice.is_none());
}
#[test]
fn shell_replacement_retains_unknown_without_copying_snapshot_or_dialog() {
    let mut s = shell();
    let w = clear(&mut s);
    finish_write(&mut s, &w, Err(Error::Transport("lost".into())));
    let mut replacement = shell();
    replacement.core.active_account = Some("other".into());
    replacement.core.account_epoch += 1;
    crate::read_preferences::settings::carry_pending(&mut s, &mut replacement);
    assert!(replacement.bulk_read.dialog.is_none());
    assert!(
        replacement.bulk_read.entries[&w.context.owner]
            .operation
            .is_some()
    );
    replacement.core.active_account = Some(w.context.owner.account.clone());
    pref(&mut replacement, Some(json!(false)));
    assert!(begin(&mut replacement).is_none());
}
#[test]
fn checkbox_is_independent_and_raw_baseline_drift_never_sets() {
    let mut s = shell();
    begin(&mut s);
    let d = s.bulk_read.dialog.as_mut().unwrap();
    d.dont_ask = true;
    let id = d.id;
    let (bulk, prefwork) = confirm_shell(&mut s, id);
    let w = prefwork.unwrap();
    assert!(bulk.is_some());
    let fresh = ClearUnreadsPreference::from_prefs(&BTreeMap::new());
    assert!(preference::preflight(&mut s, &w, &fresh).is_none());
    assert!(s.bulk_read.entries[&w.context.owner].preference.is_none());
    assert!(s.bulk_read.entries[&w.context.owner].operation.is_some());
}
#[test]
fn uncertain_preference_requires_explicit_false_not_equivalent_missing() {
    let mut s = shell();
    let c = Arc::new(Context::capture(&s).unwrap());
    let baseline = observed(&s, &c);
    let w = preference::begin(&mut s, c, baseline.clone()).unwrap();
    assert!(preference::preflight(&mut s, &w, &baseline).is_some());
    assert!(preference::finish_write(
        &mut s,
        &w,
        Err(Error::Transport("lost".into()))
    ));
    preference::finish_check(
        &mut s,
        &w,
        Ok(ClearUnreadsPreference::from_prefs(&BTreeMap::new())),
    );
    assert!(preference_pending(&s, &w.context.owner));
    // This is a new explicit status read, with its own current snapshot.
    s.bulk_read.dialog = None;
    let Next::Preference(r) = begin(&mut s).unwrap() else {
        panic!("expected read")
    };
    let p = ClearUnreadsPreference::from_prefs(&BTreeMap::from([(
        CONFIRM_CLEAR_UNREADS.into(),
        json!(false),
    )]));
    assert!(matches!(
        preference::finish_read(&mut s, &r, Ok(p)),
        Some(Next::Bulk(_))
    ));
    assert!(!preference_pending(&s, &w.context.owner));
}
#[test]
fn malformed_complete_pref_revokes_permission_without_clearing() {
    let mut s = shell();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .clear_unreads_preference = ClearUnreadsPreference::default();
    let Next::Preference(r) = begin(&mut s).unwrap() else {
        panic!("expected read")
    };
    let p = ClearUnreadsPreference::from_prefs(&BTreeMap::from([(
        CONFIRM_CLEAR_UNREADS.into(),
        json!(null),
    )]));
    assert!(preference::finish_read(&mut s, &r, Ok(p)).is_none());
    assert!(
        s.core.workspaces["T1"]
            .clear_unreads_preference
            .requires_confirmation()
            .is_err()
    );
    assert!(s.bulk_read.entries.values().all(|e| e.operation.is_none()));
}
#[test]
fn independent_pref_failure_cannot_hide_bulk_uncertainty() {
    let mut s = shell();
    begin(&mut s);
    let d = s.bulk_read.dialog.as_mut().unwrap();
    d.dont_ask = true;
    let id = d.id;
    let (bulk, pref) = confirm_shell(&mut s, id);
    let bulk = bulk.unwrap();
    let pref = pref.unwrap();
    finish_write(&mut s, &bulk, Err(Error::Transport("lost".into())));
    let message = s.bulk_read.entries[&bulk.context.owner].message.clone();
    preference::preflight_failed(&mut s, &pref, None);
    let entry = &s.bulk_read.entries[&bulk.context.owner];
    assert_eq!(entry.message, message);
    assert!(entry.preference_message.is_some());
}
#[test]
fn completed_pref_receipt_rejects_old_followup_before_projection() {
    let mut s = shell();
    let c = Arc::new(Context::capture(&s).unwrap());
    let baseline = observed(&s, &c);
    let w = preference::begin(&mut s, c, baseline.clone()).unwrap();
    preference::preflight(&mut s, &w, &baseline).unwrap();
    preference::finish_write(&mut s, &w, Ok(()));
    let fresh = ClearUnreadsPreference::from_prefs(&BTreeMap::from([(
        CONFIRM_CLEAR_UNREADS.into(),
        json!(false),
    )]));
    preference::finish_check(&mut s, &w, Ok(fresh));
    assert!(!preference_pending(&s, &w.context.owner));
    pref(&mut s, Some(json!(true)));
    let canonical = observed(&s, &w.context);
    preference::finish_check(
        &mut s,
        &w,
        Ok(ClearUnreadsPreference::from_prefs(&BTreeMap::new())),
    );
    assert_eq!(observed(&s, &w.context), canonical);
}
#[test]
fn undo_retires_refresh_generation_without_extending_deadline() {
    let mut s = shell();
    let w = clear(&mut s);
    let id = finish_write(
        &mut s,
        &w,
        Ok(json!({"ok":true,"undo_ts":"1719800700.000001"})),
    )
    .unwrap();
    let started = s.bulk_read.entries[&w.context.owner]
        .notice
        .as_ref()
        .unwrap()
        .started;
    let undo = begin_undo(&mut s, started + Duration::from_secs(5)).unwrap();
    finish_reconcile(&mut s, &w.context, id, true);
    assert!(sending(&s, &undo));
    assert!(s.bulk_read.entries[&w.context.owner].reconciling.is_none());
}
#[test]
fn in_flight_bulk_and_pref_settle_into_replacement_without_adopting_new_owner() {
    for positive in [false, true] {
        let mut s = shell();
        begin(&mut s);
        let d = s.bulk_read.dialog.as_mut().unwrap();
        d.dont_ask = true;
        let id = d.id;
        let (bulk, prefwork) = confirm_shell(&mut s, id);
        let bulk = bulk.unwrap();
        let p = prefwork.unwrap();
        assert!(preference::preflight(&mut s, &p, &p.baseline).is_some());
        let mut next = shell();
        next.core.active_account = Some("other-account".into());
        next.core.account_epoch += 1;
        let canonical = next.core.workspaces["T1"].clear_unreads_preference.clone();
        crate::read_preferences::settings::carry_pending(&mut s, &mut next);
        let outcome = if positive {
            Ok(json!({"ok":true,"undo_ts":"1719800700.000001"}))
        } else {
            Err(Error::Transport("lost".into()))
        };
        assert!(finish_write(&mut next, &bulk, outcome).is_none());
        assert!(!preference::finish_write(&mut next, &p, Ok(())));
        assert_eq!(
            next.core.workspaces["T1"].clear_unreads_preference,
            canonical
        );
        let e = &next.bulk_read.entries[&bulk.context.owner];
        assert!(e.notice.is_none());
        assert!(e.reconciling.is_none());
        assert!(e.preference.as_ref().unwrap().phase == PrefPhase::Unknown);
        assert_eq!(e.operation.is_some(), !positive);
        next.core.active_account = Some(bulk.context.owner.account.clone());
        if !positive {
            assert!(begin(&mut next).is_none());
        }
    }
}
#[test]
fn confirmation_blocks_realtime_channel_and_thread_read_authority() {
    let mut s = shell();
    s.core.pending_scroll_to = None;
    s.stick_to_bottom = true;
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    let cursor = s.core.workspaces[&team].messages[&channel]
        .last_read
        .clone()
        .unwrap();
    s.unread_anchor = Some((channel.clone(), cursor));
    crate::read_preferences::renderer::record_fixture_observation(&mut s, true, true);
    s.thread_root = Some("root".into());
    s.thread_loading = false;
    s.thread_target = None;
    s.thread_at_bottom = true;
    assert!(s.reading_latest(true));
    assert!(s.reading_thread(true, &channel, "root"));
    assert!(begin(&mut s).is_none());
    assert!(dialog_open(&s));
    assert!(!s.reading_latest(true));
    assert!(!s.reading_thread(true, &channel, "root"));
}
#[test]
fn in_flight_individual_mark_survives_replacement_until_exact_completion() {
    let mut s = shell();
    let account = s.core.active_account.clone();
    let id = begin_individual(&mut s).unwrap();
    let mut replacement = shell();
    replacement.core.active_account = Some("other".into());
    replacement.core.account_epoch += 1;
    carry_pending(&mut s, &mut replacement);
    assert!(!cursor_busy(&replacement, "T1"));
    replacement.core.active_account = account;
    pref(&mut replacement, Some(json!(false)));
    assert!(begin(&mut replacement).is_none());
    end_individual(&mut replacement, id);
    assert!(matches!(begin(&mut replacement), Some(Next::Bulk(_))));
}
