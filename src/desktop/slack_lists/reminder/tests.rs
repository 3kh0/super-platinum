use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell, "slack-list-todo-reminder");
    shell
}
fn serial(shell: &ShellState) -> u64 {
    shell
        .slack_lists
        .reminder
        .draft
        .as_ref()
        .unwrap()
        .origin
        .serial
}
fn read_event(stage: &str, origin: &Origin, token: Option<u64>) -> Value {
    json!({"stage":stage,"key":origin.key(),"row":origin.row,
        "serial":origin.serial,"visit":origin.visit,"token":token})
}
fn attempt(shell: &mut ShellState) -> Attempt {
    let serial = serial(shell);
    begin_save(shell, serial, ListReminderChange::Set(1_900_000_000)).unwrap()
}
#[test]
fn both_named_reminder_fixtures_open_actual_editable_detail_without_requests() {
    for name in [
        "slack-list-todo-reminder",
        "slack-list-todo-reminder-narrow",
    ] {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut shell, name);
        let p = presentation(&shell).unwrap();
        assert!(p.editable && !p.can_clear && !p.pending);
        assert_eq!(fixture::attempts(&shell), 0);
        assert_eq!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records
                .records
                .len(),
            73
        );
        assert!(super::super::fixture::enabled(&shell));
    }
}
#[test]
fn cancel_preflight_zero_writes_and_old_callback_cannot_dispatch_or_retire_new_draft() {
    let mut shell = shell();
    let old = attempt(&mut shell);
    close_visit(&mut shell);
    assert!(shell.slack_lists.reminder.receipts.is_empty());
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    let new = begin(&mut shell, detail.generation).unwrap();
    fixture::load(&mut shell, new.clone());
    fixture::dispatch(&mut shell, old.clone());
    rejected(&mut shell, &old, "old callback", None);
    assert!(shell.slack_lists.reminder.preflights.is_empty());
    assert_eq!(serial(&shell), new.serial);
    assert_eq!(fixture::attempts(&shell), 0);
    assert!(presentation(&shell).unwrap().editable);
}
#[test]
fn real_reminder_form_lost_reply_and_observation_preserve_all_rows_views_and_cursor() {
    let mut shell = shell();
    let origin = shell
        .slack_lists
        .reminder
        .draft
        .as_ref()
        .unwrap()
        .origin
        .clone();
    assert_eq!(
        fixture::read_events(&shell),
        &[read_event("load", &origin, None)]
    );
    assert!(shell.slack_lists.reminder.loads.is_empty());
    let before = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let request = attempt(&mut shell);
    assert_eq!(request.origin.serial, origin.serial);
    assert_eq!(
        shell.slack_lists.reminder.preflights.get(&origin.serial),
        Some(&origin.key())
    );
    fixture::dispatch(&mut shell, request.clone());
    assert!(shell.slack_lists.reminder.preflights.is_empty());
    assert_eq!(
        fixture::read_events(&shell),
        &[
            read_event("load", &origin, None),
            read_event("preflight", &origin, None)
        ]
    );
    assert_eq!(fixture::attempts(&shell), 1);
    assert_eq!(
        fixture::forms(&shell)[0],
        json!({"method":"saved.add","fields":[
        ["item_type","list_record"],["item_id","F_LIST_FIXTURE"],["ts","R_LIST_000"],
        ["date_due","1900000000"],["_x_reason","saved-api/saveListRecord"]]})
    );
    assert!(!fixture::complete(&mut shell, true, true));
    assert!(
        begin_save(
            &mut shell,
            request.origin.serial,
            ListReminderChange::Set(1_900_000_000)
        )
        .is_none()
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    let read = begin_check(&mut shell, request.origin.serial).unwrap();
    assert!(read.current(&shell));
    assert_eq!(read.key, origin.key());
    assert_eq!(read.serial, origin.serial);
    assert_eq!(
        shell.slack_lists.reminder.receipts[&read.key].checking,
        Some(read.token)
    );
    let token = read.token;
    fixture::check(&mut shell, read);
    assert_eq!(
        fixture::read_events(&shell),
        &[
            read_event("load", &origin, None),
            read_event("preflight", &origin, None),
            read_event("check", &origin, Some(token))
        ]
    );
    assert!(shell.slack_lists.reminder.receipts.is_empty());
    let after = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let mut expected = before.records.raw.clone();
    expected["records"][0]["saved"] = json!({"date_due":1900000000,"date_completed":0,
        "is_archived":false,"state":"in_progress","todo_state":"saved"});
    assert_eq!(after.records.raw, expected);
    assert_eq!(after.document.raw, before.document.raw);
    assert_eq!(fixture::attempts(&shell), 1);
    assert_eq!(fixture::reads(&shell), fixture::read_events(&shell).len());
}
#[test]
fn account_epoch_and_permission_changes_refuse_before_single_attempt() {
    for change in 0..3 {
        let mut shell = shell();
        let request = attempt(&mut shell);
        let mut current = super::super::fixture::deletion_loaded(&shell).unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => current.list_experiments.as_mut().unwrap().lists_in_later = false,
            _ => current.records.records[0].raw["date_created"] = json!(1),
        }
        assert!(!validate(&mut shell, &request, &current, None));
        assert_eq!(fixture::attempts(&shell), 0);
    }
}
#[test]
fn unconfirmed_receipt_survives_close_and_cooldown_without_replay() {
    let mut shell = shell();
    let request = attempt(&mut shell);
    fixture::dispatch(&mut shell, request.clone());
    assert!(!finish(
        &mut shell,
        &request,
        Err(ListReminderFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(30)
        }))
    ));
    close_visit(&mut shell);
    assert!(blocks_mutation(&shell));
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    assert!(begin(&mut shell, detail.generation).is_none());
    assert!(!presentation(&shell).unwrap().can_check);
    assert!(begin_check(&mut shell, request.origin.serial).is_none());
    assert_eq!(fixture::attempts(&shell), 1);
    shell
        .slack_lists
        .reminder
        .receipts
        .get_mut(&request.origin.key())
        .unwrap()
        .retry_at = 0;
    shell.slack_lists.reminder.cooldowns.clear();
    let read = begin_check(&mut shell, request.origin.serial).unwrap();
    fixture::check(&mut shell, read);
    assert!(blocks_mutation(&shell)); // old canonical value is not confirmation
    assert_eq!(fixture::attempts(&shell), 1);
}

#[test]
fn load_and_preflight_cooldowns_survive_close_and_retired_callbacks_and_expire_visibly() {
    let mut shell = shell();
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    close_visit(&mut shell);
    let loading = begin(&mut shell, detail.generation).unwrap();
    close_visit(&mut shell);
    loaded(
        &mut shell,
        &loading,
        Err(Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert!(shell.slack_lists.reminder.loads.is_empty());
    assert!(begin(&mut shell, detail.generation).is_none());
    assert!(reason(&shell).unwrap().contains("wait"));
    assert!(poll(&mut shell));
    let deadline = cooldown(&shell, &loading.key());
    assert!(poll_at(&mut shell, deadline));
    let opened = begin(&mut shell, detail.generation).unwrap();
    fixture::load(&mut shell, opened);
    let pending = attempt(&mut shell);
    close_visit(&mut shell);
    // No receipt owns this cancelled attempt, but its actual awaited 429 must
    // still restrict the same stable owner/List before another load or write.
    rejected(
        &mut shell,
        &pending,
        "late preflight rate limit",
        Some(&Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(shell.slack_lists.reminder.preflights.is_empty());
    assert!(begin(&mut shell, detail.generation).is_none());
    assert_eq!(fixture::attempts(&shell), 0);
}

#[test]
fn current_loaded_draft_repaints_on_cooldown_expiry_and_retired_reads_remain_bounded() {
    let mut shell = shell();
    let request = attempt(&mut shell);
    rejected(
        &mut shell,
        &request,
        "wait",
        Some(&Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert!(!presentation(&shell).unwrap().editable);
    assert!(poll(&mut shell));
    let deadline = cooldown(&shell, &request.origin.key());
    assert!(poll_at(&mut shell, deadline));
    shell.slack_lists.reminder.draft.as_mut().unwrap().retry_at = 0;
    assert!(presentation(&shell).unwrap().editable);
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    close_visit(&mut shell);
    let mut pending = Vec::new();
    for _ in 0..32 {
        let old = begin(&mut shell, detail.generation).unwrap();
        pending.push(old);
        close_visit(&mut shell);
    }
    assert!(begin(&mut shell, detail.generation).is_none());
    for old in pending {
        loaded(
            &mut shell,
            &old,
            Err(Error::RateLimited {
                retry_after_secs: Some(30),
            }),
        );
    }
    assert!(shell.slack_lists.reminder.loads.is_empty());
    assert_eq!(shell.slack_lists.reminder.cooldowns.len(), 1);
    assert_eq!(fixture::attempts(&shell), 0);
}
#[test]
fn old_read_releases_only_its_owned_token_and_cannot_project_after_retirement() {
    let mut shell = shell();
    let request = attempt(&mut shell);
    fixture::dispatch(&mut shell, request.clone());
    fixture::complete(&mut shell, true, true);
    let old = begin_check(&mut shell, request.origin.serial).unwrap();
    let before = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    shell.core.account_epoch += 1;
    let result = super::super::fixture::deletion_loaded(&shell).unwrap();
    assert!(!finish_check(&mut shell, &old, Ok(result)));
    assert!(
        shell
            .slack_lists
            .reminder
            .receipts
            .get(&old.key)
            .unwrap()
            .checking
            .is_none()
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    assert_eq!(fixture::attempts(&shell), 1);
}

#[test]
fn admitted_reads_recheck_late_retired_cooldown_before_start_and_release_only_owned_work() {
    for readback in [false, true] {
        let mut shell = shell();
        let initial = shell
            .slack_lists
            .reminder
            .draft
            .as_ref()
            .unwrap()
            .origin
            .clone();
        let mut expected_reads = vec![read_event("load", &initial, None)];
        assert_eq!(fixture::read_events(&shell), expected_reads);
        let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
        close_visit(&mut shell);
        let retired = begin(&mut shell, detail.generation).unwrap();
        close_visit(&mut shell);
        let queued = begin(&mut shell, detail.generation).unwrap();
        assert!(queued.read_current(&shell));
        assert_eq!(
            shell.slack_lists.reminder.loads.get(&retired.serial),
            Some(&retired.key())
        );
        assert_eq!(
            shell.slack_lists.reminder.loads.get(&queued.serial),
            Some(&queued.key())
        );
        let read = if readback {
            fixture::load(&mut shell, queued.clone());
            expected_reads.push(read_event("load", &queued, None));
            assert_eq!(fixture::read_events(&shell), expected_reads);
            assert!(
                !shell
                    .slack_lists
                    .reminder
                    .loads
                    .contains_key(&queued.serial)
            );
            assert!(
                shell
                    .slack_lists
                    .reminder
                    .loads
                    .contains_key(&retired.serial)
            );
            let request = attempt(&mut shell);
            assert_eq!(request.origin.serial, queued.serial);
            assert_eq!(
                shell.slack_lists.reminder.preflights.get(&queued.serial),
                Some(&queued.key())
            );
            fixture::dispatch(&mut shell, request.clone());
            expected_reads.push(read_event("preflight", &queued, None));
            assert_eq!(fixture::read_events(&shell), expected_reads);
            assert!(shell.slack_lists.reminder.preflights.is_empty());
            fixture::complete(&mut shell, true, true);
            let read = begin_check(&mut shell, request.origin.serial).unwrap();
            assert!(read.current(&shell));
            assert_eq!(read.key, queued.key());
            assert_eq!(read.serial, queued.serial);
            assert_eq!(
                shell.slack_lists.reminder.receipts[&read.key].checking,
                Some(read.token)
            );
            Some(read)
        } else {
            None
        };
        let before = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        loaded(
            &mut shell,
            &retired,
            Err(Error::RateLimited {
                retry_after_secs: Some(30),
            }),
        );
        assert!(queued.current(&shell) && queued.owns_draft(&shell));
        assert!(!queued.read_current(&shell));
        if let Some(read) = read {
            assert!(!read.current(&shell));
            fixture::check(&mut shell, read.clone());
            assert_eq!(fixture::read_events(&shell), expected_reads);
            let receipt = shell.slack_lists.reminder.receipts.get(&read.key).unwrap();
            assert!(receipt.checking.is_none());
            assert!(receipt.phase == Phase::Uncertain);
            assert!(begin_check(&mut shell, queued.serial).is_none());
        } else {
            fixture::load(&mut shell, queued.clone());
            let draft = shell.slack_lists.reminder.draft.as_ref().unwrap();
            assert_eq!(draft.origin.serial, queued.serial);
            assert!(!draft.loading && draft.loaded.is_none());
            assert_eq!(fixture::attempts(&shell), 0);
        }
        assert!(shell.slack_lists.reminder.loads.is_empty());
        assert_eq!(fixture::read_events(&shell), expected_reads);
        assert_eq!(fixture::reads(&shell), expected_reads.len());
        assert_eq!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            before.records
        );
        assert!(cooldown(&shell, &queued.key()) > now());
    }
}

#[test]
fn completion_target_lookup_retains_reminder_draft_and_receipt_off_view() {
    let mut s = shell();
    let scope = s.slack_lists.scope.clone().unwrap();
    assert!(blocks_target(&s, &scope, "F_LIST_FIXTURE"));
    let a = attempt(&mut s);
    fixture::dispatch(&mut s, a);
    close_visit(&mut s);
    s.slack_lists.view.as_mut().unwrap().id = "F_OTHER".into();
    assert!(blocks_target(&s, &scope, "F_LIST_FIXTURE"));
    assert!(!blocks_target(&s, &scope, "F_OTHER"));
    s.core.active_account = Some("foreign".into());
    let foreign = Scope::capture(&s).unwrap();
    assert!(!blocks_target(&s, &foreign, "F_LIST_FIXTURE"));
}
#[test]
fn replacement_carries_reminder_late_outcome_and_cooldown_without_draft_or_credentials() {
    let mut old = shell();
    let a = attempt(&mut old);
    fixture::dispatch(&mut old, a.clone());
    let key = a.origin.key();
    let mut new = ShellState::fixture(crate::media::MediaRegistry::default());
    new.core.active_account = Some("foreign".into());
    carry_pending(&mut old, &mut new);
    assert!(old.slack_lists.reminder.receipts.is_empty());
    assert!(new.slack_lists.reminder.draft.is_none());
    assert!(a.owned(&new));
    assert!(!a.sending(&new));
    assert!(!finish(
        &mut new,
        &a,
        Err(ListReminderFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(30)
        }))
    ));
    let retained = &new.slack_lists.reminder.receipts[&key];
    assert!(retained.phase == Phase::Uncertain && retained.response_seen);
    assert_eq!(retained.attempt.intent.fields(), a.intent.fields());
    assert!(retained.retry_at > now());
    assert!(cooldown(&new, &key) > now());
    assert!(begin_check(&mut new, a.origin.serial).is_none());
}

#[test]
fn retained_reminder_prevents_assignment_editor_admission() {
    let mut s = shell();
    let a = attempt(&mut s);
    fixture::dispatch(&mut s, a.clone());
    close_visit(&mut s);
    record_detail::dismiss_shell(&mut s);
    assert!(a.owned(&s));
    assert!(blocks_mutation(&s));
    assert!(!editor::assignment::fixture::seed(&mut s));
    assert!(!editor::assignment::blocks_mutation(&s));
    assert!(a.owned(&s));
    assert!(s.slack_lists.operations.is_empty());
    // Same layout and owner admit assignment when only the reminder receipt is
    // removed. This distinguishes the guard from a lingering detail surface.
    let retained = std::mem::take(&mut s.slack_lists.reminder.receipts);
    assert!(editor::assignment::fixture::seed(&mut s));
    s.slack_lists.reminder.receipts = retained;
}
#[test]
fn retained_assignment_prevents_new_reminder_but_keeps_its_own_recovery() {
    let mut s = shell();
    close_visit(&mut s);
    let original = s.slack_lists.view.clone();
    record_detail::dismiss_shell(&mut s);
    assert!(editor::assignment::fixture::seed(&mut s));
    s.slack_lists.view = original;
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    assert!(available(&s));
    assert!(editor::assignment::blocks_mutation(&s));
    assert!(begin(&mut s, detail.generation).is_none());
    assert!(s.slack_lists.reminder.draft.is_none());
    assert!(s.slack_lists.reminder.receipts.is_empty());
    assert!(editor::assignment::blocks_mutation(&s));
}
