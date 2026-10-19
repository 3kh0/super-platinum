use super::*;
use serde_json::json;

pub(super) fn shell() -> ShellState {
    let mut shell = super::super::tests::make_shell();
    let mut loaded = super::super::tests::loaded(false);
    let mut raw = loaded.document.raw.clone();
    raw["id"] = json!("FCOMMENT01");
    loaded.document = Document::parse(raw.clone()).unwrap();
    let mut records = loaded.records.raw.clone();
    records["records"][0]["thread_ts"] = json!("1700000000.000001");
    loaded.records = RecordsPage::parse(records).unwrap();
    assert!(super::super::open_shell(
        &mut shell,
        serde_json::from_value(raw).unwrap()
    ));
    super::super::tests::read(&mut shell, loaded);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (instance, visit, layout) = (
        view.rows[0].instance_id.clone(),
        view.generation,
        view.layout.generation,
    );
    assert!(record_detail::open_shell(
        &mut shell, &instance, visit, layout
    ));
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    assert!(!controller::open_shell(&mut shell, generation));
    shell
}
fn loaded(target: &Target, cursor: Option<&str>) -> network::Loaded {
    let page=Page::parse(target,json!({"ok":true,"messages":[
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","text":"A real comment","user":"U1"}
    ],"has_more":cursor.is_some(),"response_metadata":{"next_cursor":cursor.unwrap_or("")}})).unwrap();
    network::Loaded {
        page,
        projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
    }
}
fn complete(shell: &mut ShellState, cursor: Option<&str>) {
    let (owner, serial, target, page) = controller::begin(shell, false).unwrap();
    controller::finish(shell, &owner, serial, page, Ok(loaded(&target, cursor)));
}

#[test]
fn current_read_projects_without_selecting_a_conversation_or_thread() {
    let mut shell = shell();
    let active = shell.active_channel;
    let generation = shell.channel_generation;
    complete(&mut shell, Some("next"));
    let p = presentation(&shell).unwrap();
    assert_eq!(p.rows.len(), 1);
    assert!(p.has_more);
    assert!(!p.loading);
    assert_eq!(p.channel, "CCOMMENT01");
    assert_eq!(shell.active_channel, active);
    assert_eq!(shell.channel_generation, generation);
}
#[test]
fn failed_refresh_retains_rows_and_cursor_and_observes_retry_after() {
    let mut shell = shell();
    complete(&mut shell, Some("next"));
    let rows = presentation(&shell).unwrap().rows;
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    let p = presentation(&shell).unwrap();
    assert!(Arc::ptr_eq(&p.rows, &rows));
    assert!(p.has_more && p.error.is_some() && p.retry_after > 0);
    assert!(controller::begin(&mut shell, true).is_none());
    controller::close_shell(&mut shell);
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    assert!(controller::begin(&mut shell, false).is_none());
}
#[test]
fn closed_and_same_record_reopened_visit_refuses_old_completion() {
    let mut shell = shell();
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::close_shell(&mut shell);
    assert!(!owner.reading(&shell, serial));
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    assert!(!controller::callback_current(&shell, owner.serial));
    assert!(!controller::open_shell(&mut shell, generation));
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(loaded(&target, None)),
    );
    assert!(presentation(&shell).unwrap().rows.is_empty());
    assert!(controller::begin(&mut shell, false).is_some());
}
#[test]
fn account_cookie_transport_and_canonical_record_drift_refuse_publication() {
    for mutation in 0..6 {
        let mut shell = shell();
        let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
        match mutation {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.session.as_mut().unwrap().d_cookie.push('x'),
            2 => {
                shell.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::new("synthetic2").unwrap(),
                ))
            }
            3 => shell.overlay = None,
            4 => shell.slack_lists.view.as_mut().unwrap().revision += 1,
            _ => {
                let loaded = shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = loaded.records.raw.clone();
                raw["records"][0]["updated_timestamp"] = json!("101.0");
                loaded.records = RecordsPage::parse(raw).unwrap();
            }
        }
        assert!(!owner.reading(&shell, serial), "mutation {mutation}");
        controller::finish(
            &mut shell,
            &owner,
            serial,
            cursor,
            Ok(loaded(&target, None)),
        );
        assert!(
            shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .rows
                .is_empty()
        );
    }
}
#[test]
fn next_page_has_distinct_serial_and_repeated_cursor_is_not_replayed() {
    let mut shell = shell();
    complete(&mut shell, Some("next"));
    let (owner, serial, target, cursor) = controller::begin(&mut shell, true).unwrap();
    assert_eq!(cursor.as_deref(), Some("next"));
    assert!(controller::begin(&mut shell, true).is_none());
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(loaded(&target, Some("next"))),
    );
    assert!(!presentation(&shell).unwrap().has_more);
    assert!(controller::begin(&mut shell, true).is_none());
    assert!(controller::begin(&mut shell, false).is_some());
}

#[test]
fn retry_expiry_is_bound_to_original_read_and_panel() {
    let mut shell = shell();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    let future = shell.slack_lists.comments.entry.as_ref().unwrap().retry_at;
    assert!(!controller::retry_current(&shell, &owner, serial, future));
    let expired = now().saturating_sub(1);
    shell.slack_lists.comments.entry.as_mut().unwrap().retry_at = expired;
    assert!(controller::retry_current(&shell, &owner, serial, expired));
    assert!(!controller::retry_current(
        &shell,
        &owner,
        serial + 1,
        expired
    ));
    controller::close_shell(&mut shell);
    assert!(!controller::retry_current(&shell, &owner, serial, expired));
}

#[test]
fn switching_record_cannot_bypass_account_rate_limit() {
    let mut shell = shell();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    controller::close_shell(&mut shell);
    record_detail::dismiss_shell(&mut shell);
    let mut loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let mut raw = loaded.records.raw.clone();
    raw["records"][0]["id"] = json!("different-record");
    loaded.records = RecordsPage::parse(raw).unwrap();
    super::super::tests::read(&mut shell, loaded);
    assert!(record_detail::open_created(&mut shell, "different-record"));
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(controller::open_shell(&mut shell, generation));
    assert!(presentation(&shell).unwrap().retry_after > 0);
    assert!(controller::begin(&mut shell, false).is_none());
}
#[test]
fn fresh_root_change_refuses_continuation_but_refresh_replaces_it() {
    let mut shell = shell();
    complete(&mut shell, Some("next"));
    let old = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .target
        .clone();
    let file = json!({"ok":true,"file":old.document_raw()});
    let mut record = old.record_raw().clone();
    record["thread_ts"] = json!("1800000000.000001");
    let record = json!({"ok":true,"record":record});
    assert!(old.refresh(&file, &record, true).is_err());
    let target = old.refresh(&file, &record, false).unwrap();
    let page = Page::parse(&target, json!({"ok":true,"messages":[]})).unwrap();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page,
            projection: Projection::observed(json!({"ok":true,"assignments":{}})).unwrap(),
        }),
    );
    assert_eq!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .target
            .root_ts(),
        Some("1800000000.000001")
    );
    assert!(presentation(&shell).unwrap().rows.is_empty());
}
