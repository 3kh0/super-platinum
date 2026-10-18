use super::*;
use crate::slack::{Error, client::RequestBody};
use std::collections::BTreeMap;

struct Fixture {
    journal: store::Journal,
    path: std::path::PathBuf,
    account: String,
    _cleanup: Cleanup,
}
struct Cleanup(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("scheduled-forward-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("private.sqlite");
        let account = uuid::Uuid::new_v4().to_string();
        let journal = store::Journal::open(&path, &account).unwrap();
        Self {
            _cleanup: Cleanup(dir),
            path,
            account,
            journal,
        }
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn source(private: bool) -> Source {
    Source {
        channel: "C1".into(),
        timestamp: "100.000001".into(),
        thread: Some("99.000001".into()),
        author: Some("U1".into()),
        permalink: "https://fixture.slack.com/archives/C1/p100000001?thread_ts=99.000001&cid=C1"
            .into(),
        private,
    }
}
fn note() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","extra":"retained","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Note 🚀","style":{"bold":true,"future":"retain"}},{"type":"future_atom","payload":{"a":[1,2]}}]}]}),
    ]
}
fn intent() -> Intent {
    Intent::new(
        Owner {
            team: "T1".into(),
            user: "U1".into(),
        },
        source(true),
        Destination::Conversation("C2".into()),
        "Destination".into(),
        note(),
        2000,
        true,
        1000,
    )
    .unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn form(attempt: &mut Attempt) -> BTreeMap<String, String> {
    let request = attempt
        .take_request(&SlackClient::default(), &workspace())
        .unwrap();
    assert!(request.url.contains("/api/drafts."));
    match request.body {
        RequestBody::Form(fields) => fields.into_iter().filter(|(k, _)| k != "token").collect(),
        _ => panic!("expected form"),
    }
}
fn raw(record: &Record) -> Value {
    json!({"ok":true,"draft":{"id":"Dr1","client_msg_id":record.intent.client_msg_id,"blocks":record.intent.blocks,"destinations":record.intent.destination.wire(),"file_ids":[],"date_scheduled":record.intent.post_at,"last_updated_ts":"1234","is_deleted":false,"is_sent":false,"is_from_composer":true,"future":{"kept":true}}})
}
fn confirmed(f: &mut Fixture) -> Record {
    let record = f.journal.create(intent(), 1000).unwrap();
    let mut a = f.journal.reserve(&record, false, None, 1001).unwrap();
    form(&mut a);
    let response = raw(a.record());
    f.journal.finish(&a.complete(Ok(response), 1002)).unwrap()
}

#[test]
fn source_atom_and_rich_note_are_lossless_and_private_confirmation_is_explicit() {
    let n = note();
    let original = n.clone();
    let out = append_reference(&n, &source(true)).unwrap();
    assert_eq!(n, original);
    assert_eq!(out[0]["extra"], "retained");
    assert_eq!(
        &out[0]["elements"][0]["elements"].as_array().unwrap()[..2],
        n[0]["elements"][0]["elements"].as_array().unwrap()
    );
    assert_eq!(
        out[0]["elements"][0]["elements"][2],
        json!({"type":"message_mention","message_ts":"100.000001","channel_id":"C1","text":source(true).permalink,"url":source(true).permalink,"author_id":"U1","thread_ts":"99.000001"})
    );
    let list = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_list","style":"bullet","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"item"}]}]}]}),
    ];
    let result = append_reference(&list, &source(false)).unwrap();
    assert_eq!(result[0]["elements"][0], list[0]["elements"][0]);
    assert_eq!(result[0]["elements"][1]["type"], "rich_text_section");
    assert!(
        Intent::new(
            Owner {
                team: "T1".into(),
                user: "U1".into()
            },
            source(true),
            Destination::Conversation("C2".into()),
            "Name".into(),
            vec![],
            2000,
            false,
            1000
        )
        .is_err()
    );
}

#[test]
fn person_destination_uses_captured_user_ids_and_review_cooldown_survives_reopen() {
    let mut f = Fixture::new();
    let mut input = intent();
    input.destination = Destination::Person("U2".into());
    let r = f.journal.create(input, 1000).unwrap();
    let mut a = f.journal.reserve(&r, false, None, 1001).unwrap();
    assert_eq!(form(&mut a)["destinations"], "[{\"user_ids\":[\"U2\"]}]");
    let raw = raw(a.record());
    let unknown = f
        .journal
        .finish(&a.complete(Err(Error::Transport("lost".into())), 1002))
        .unwrap();
    let delayed = f.journal.defer_review(&unknown, 45, 1003).unwrap();
    let mut reopened = store::Journal::open(&f.path, &f.account).unwrap();
    assert_eq!(reopened.get(delayed.id()).unwrap().unwrap(), delayed);
    assert!(reopened.review(&delayed, &raw, 1047).is_err());
    assert_eq!(
        reopened.review(&delayed, &raw, 1048).unwrap().phase(),
        Phase::Confirmed
    );
}

#[test]
fn create_uses_actual_drafts_form_and_capability_is_once_and_owner_bound() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    let mut a = f.journal.reserve(&r, false, None, 1001).unwrap();
    let mut wrong = workspace();
    wrong.user_id = "OTHER".into();
    assert!(a.take_request(&SlackClient::default(), &wrong).is_err());
    let fields = form(&mut a);
    assert_eq!(fields.len(), 9);
    assert_eq!(
        fields["blocks"],
        serde_json::to_string(r.intent.blocks()).unwrap()
    );
    assert_eq!(fields["destinations"], "[{\"channel_id\":\"C2\"}]");
    assert_eq!(fields["date_scheduled"], "2000");
    assert_eq!(fields["file_ids"], "[]");
    assert_eq!(fields["attachments"], "");
    assert_eq!(fields["is_from_composer"], "true");
    assert_eq!(fields["client_msg_id"], r.intent.client_msg_id);
    assert_eq!(fields["_x_reason"], "schedule-draft");
    assert_eq!(
        fields["unfurl"],
        serde_json::to_string(
            &json!([{"url":source(true).permalink,"acknowledged_private_share":true}])
        )
        .unwrap()
    );
    assert!(
        a.take_request(&SlackClient::default(), &workspace())
            .is_err()
    );
    assert!(f.journal.abandon(a, 1002).is_err());
}

#[test]
fn restart_and_second_connection_never_replay_reserved_or_lost_create() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    assert_eq!(f.journal.create(r.intent.clone(), 1000).unwrap(), r);
    let mut other = store::Journal::open(&f.path, &f.account).unwrap();
    let mut a = f.journal.reserve(&r, false, None, 1001).unwrap();
    assert!(other.reserve(&r, false, None, 1001).is_err());
    let parked = other.get(r.id()).unwrap().unwrap();
    assert_eq!(parked.phase(), Phase::Unknown);
    assert!(other.reserve(&parked, false, None, 1002).is_err());
    form(&mut a);
    let unknown = f
        .journal
        .finish(&a.complete(Err(Error::Transport("lost reply".into())), 1003))
        .unwrap();
    assert_eq!(unknown.phase(), Phase::Unknown);
    let reopened = store::Journal::open(&f.path, &f.account).unwrap();
    assert_eq!(reopened.get(r.id()).unwrap().unwrap(), unknown);
    assert_eq!(unknown.intent.note(), note());
    assert!(store::Journal::open(&f.path, &uuid::Uuid::new_v4().to_string()).is_err());
}

#[test]
fn lost_create_needs_unique_complete_identity_and_payload_not_absence_or_ack() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    let a = f.journal.reserve(&r, false, None, 1001).unwrap();
    let parked = a.record().clone();
    drop(a);
    for missing in [
        json!({"ok":true}),
        json!({"ok":true,"drafts":[]}),
        json!({"ok":true,"drafts":[],"has_more":false}),
    ] {
        assert!(list_match(&parked, &missing).is_none());
    }
    let response = raw(&parked);
    let rows = json!({"ok":true,"drafts":[response["draft"]],"has_more":true});
    assert_eq!(list_match(&parked, &rows), Some(response.clone()));
    assert!(
        list_match(
            &parked,
            &json!({"ok":true,"drafts":[response["draft"],response["draft"]]})
        )
        .is_none()
    );
    for field in [
        "blocks",
        "destinations",
        "file_ids",
        "is_sent",
        "is_deleted",
        "date_scheduled",
    ] {
        let mut bad = response.clone();
        bad["draft"].as_object_mut().unwrap().remove(field);
        assert!(f.journal.review(&parked, &bad, 1002).is_err());
    }
    let mut wrong = response.clone();
    wrong["draft"]["client_msg_id"] = "other".into();
    assert!(f.journal.review(&parked, &wrong, 1002).is_err());
    let done = f.journal.review(&parked, &response, 1002).unwrap();
    assert_eq!(done.phase(), Phase::Confirmed);
    assert_eq!(done.server_id(), Some("Dr1"));
}

#[test]
fn edit_cancel_and_delete_require_current_full_readback_and_keep_source_note() {
    let mut f = Fixture::new();
    let r = confirmed(&mut f);
    let base = r.reviewed().unwrap().clone();
    let mut drift = base.clone();
    drift["draft"]["future"]["kept"] = false.into();
    assert!(
        f.journal
            .revise(&r, note(), 2100, Some(&drift), 1003)
            .is_err()
    );
    let edit = f
        .journal
        .revise(&r, note(), 2100, Some(&base), 1003)
        .unwrap();
    assert_eq!(edit.intent.source(), r.intent.source());
    let mut a = f.journal.reserve(&edit, false, Some(&base), 1004).unwrap();
    let fields = form(&mut a);
    assert_eq!(fields["draft_id"], "Dr1");
    assert_eq!(fields["client_last_updated_ts"], "1004000");
    assert!(!fields.contains_key("client_msg_id"));
    assert_eq!(fields["date_scheduled"], "2100");
    let response = raw(a.record());
    let edited = f.journal.finish(&a.complete(Ok(response), 1005)).unwrap();
    let cancel = f
        .journal
        .revise(
            &edited,
            edited.intent.note.clone(),
            0,
            edited.reviewed(),
            1006,
        )
        .unwrap();
    let mut a = f
        .journal
        .reserve(&cancel, false, edited.reviewed(), 1007)
        .unwrap();
    assert_eq!(form(&mut a)["date_scheduled"], "0");
    let response = raw(a.record());
    let saved = f.journal.finish(&a.complete(Ok(response), 1008)).unwrap();
    assert_eq!(saved.phase(), Phase::Confirmed);
    assert_eq!(saved.action(), Action::SaveAsDraft);
    assert_eq!(saved.intent.note(), note());
    let mut a = f
        .journal
        .reserve(&saved, true, saved.reviewed(), 1009)
        .unwrap();
    let fields = form(&mut a);
    assert_eq!(fields["skip_file_deletion"], "false");
    assert_eq!(fields.len(), 4);
    let retained = f
        .journal
        .finish(&a.complete(Ok(json!({"ok":true})), 1010))
        .unwrap();
    assert_eq!(retained.phase(), Phase::Unknown);
    let mut deleted = raw(&retained);
    deleted["draft"]["is_deleted"] = true.into();
    assert_eq!(
        f.journal.review(&retained, &deleted, 1011).unwrap().phase(),
        Phase::Deleted
    );
}

#[test]
fn late_reply_is_recorded_without_overwriting_newer_explicit_edit() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    let mut old = f.journal.reserve(&r, false, None, 1001).unwrap();
    form(&mut old);
    let original = old.record().clone();
    let confirmed = f.journal.review(&original, &raw(&original), 1002).unwrap();
    let edit = f
        .journal
        .revise(&confirmed, note(), 2200, confirmed.reviewed(), 1003)
        .unwrap();
    let mut newer = f
        .journal
        .reserve(&edit, false, confirmed.reviewed(), 1004)
        .unwrap();
    form(&mut newer);
    let newer_record = newer.record().clone();
    let preserved = f
        .journal
        .finish(&old.complete(Err(Error::Transport("late".into())), 1005))
        .unwrap();
    assert_eq!(preserved, newer_record);
    assert_eq!(preserved.intent.post_at(), 2200);
    let response = raw(newer.record());
    assert_eq!(
        f.journal
            .finish(&newer.complete(Ok(response), 1006))
            .unwrap()
            .phase(),
        Phase::Confirmed
    );
}

#[test]
fn rate_limit_retains_exact_input_until_explicit_retry_and_conflict_keeps_both() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    let mut a = f.journal.reserve(&r, false, None, 1001).unwrap();
    form(&mut a);
    let rejected = f
        .journal
        .finish(&a.complete(
            Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }),
            1002,
        ))
        .unwrap();
    assert_eq!(rejected.phase(), Phase::Rejected);
    assert!(f.journal.reserve(&rejected, false, None, 1061).is_err());
    let mut retry = f.journal.reserve(&rejected, false, None, 1062).unwrap();
    assert_eq!(form(&mut retry)["client_msg_id"], r.intent.client_msg_id);
    let pending = retry.record().clone();
    let mut changed = raw(&pending);
    changed["draft"]["date_scheduled"] = 2500.into();
    let conflict = f.journal.review(&pending, &changed, 1063).unwrap();
    assert_eq!(conflict.phase(), Phase::Conflict);
    assert_eq!(conflict.intent.post_at(), 2000);
    assert_eq!(
        conflict.observed().unwrap()["draft"]["date_scheduled"],
        2500
    );
    assert!(
        f.journal
            .reserve(&conflict, false, Some(&changed), 1064)
            .is_err()
    );
    let returned = f
        .journal
        .finish(&retry.complete(Err(Error::Transport("late".into())), 1065))
        .unwrap();
    assert_eq!(returned.phase(), Phase::Conflict);
    assert_eq!(returned.observed(), Some(&changed));
}

#[test]
fn local_finish_failure_retains_exact_completion_for_storage_only_retry() {
    let mut f = Fixture::new();
    let r = f.journal.create(intent(), 1000).unwrap();
    let mut attempt = f.journal.reserve(&r, false, None, 1001).unwrap();
    form(&mut attempt);
    let parked = attempt.record().clone();
    let response = raw(&parked);
    let completion = attempt.complete(Ok(response.clone()), 1002);
    let db = rusqlite::Connection::open(&f.path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_completion BEFORE INSERT ON outcomes BEGIN SELECT RAISE(ABORT, 'fixture storage failure'); END;").unwrap();
    assert!(f.journal.finish(&completion).is_err());
    assert_eq!(f.journal.get(r.id()).unwrap().unwrap(), parked);
    assert_eq!(completion.record(), &parked);
    assert!(f.journal.reserve(&parked, false, None, 1003).is_err());
    db.execute_batch("DROP TRIGGER fail_completion;").unwrap();
    let saved = f.journal.finish(&completion).unwrap();
    assert_eq!(saved.phase(), Phase::Confirmed);
    assert_eq!(saved.observed(), Some(&response));
    // Retrying an already committed storage acknowledgement adds no version or outcome.
    assert_eq!(f.journal.finish(&completion).unwrap(), saved);
    let count: i64 = db
        .query_row("SELECT count(*) FROM outcomes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let count: i64 = db
        .query_row("SELECT count(*) FROM versions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 3);
}

#[test]
fn local_edit_survives_preflight_conflict_and_rate_limit_without_new_admission() {
    let mut f = Fixture::new();
    let original = confirmed(&mut f);
    let changed = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Retain this edit"}]}]}),
    ];
    let local = f
        .journal
        .retain_edit(&original, changed.clone(), 2300, 1003)
        .unwrap();
    assert_eq!(local.intent.note(), changed);
    assert_eq!(local.reviewed(), original.reviewed());
    let delayed = f.journal.defer_read(&local, 60, 1004).unwrap();
    let mut reopened = store::Journal::open(&f.path, &f.account).unwrap();
    assert!(
        reopened
            .reserve(&delayed, false, original.reviewed(), 1063)
            .is_err()
    );
    let mut remote = original.reviewed().unwrap().clone();
    remote["draft"]["future"]["kept"] = false.into();
    let conflict = reopened.observe_preflight(&delayed, &remote, 1064).unwrap();
    assert_eq!(conflict.phase(), Phase::Conflict);
    assert_eq!(conflict.intent.note(), changed);
    assert_eq!(conflict.reviewed(), original.reviewed());
    assert_eq!(conflict.observed(), Some(&remote));
    assert!(
        reopened
            .reserve(&conflict, false, Some(&remote), 1065)
            .is_err()
    );
    assert!(reopened.retain_edit(&conflict, note(), 2400, 1065).is_err());
}

#[test]
fn known_unsent_storage_failure_keeps_non_dispatchable_retry_proof() {
    let mut f = Fixture::new();
    let record = f.journal.create(intent(), 1000).unwrap();
    let attempt = f.journal.reserve(&record, false, None, 1001).unwrap();
    let pending = attempt.record().clone();
    let proof = attempt.unsent(1002);
    let db = rusqlite::Connection::open(&f.path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_unsent BEFORE INSERT ON versions BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;").unwrap();
    assert!(f.journal.save_unsent(&proof).is_err());
    assert_eq!(f.journal.get(record.id()).unwrap().unwrap(), pending);
    db.execute_batch("DROP TRIGGER fail_unsent;").unwrap();
    let ready = f.journal.save_unsent(&proof).unwrap();
    assert_eq!(ready.phase(), Phase::Ready);
    assert_eq!(f.journal.save_unsent(&proof).unwrap(), ready);
    let mut attempt = f.journal.reserve(&ready, false, None, 1003).unwrap();
    form(&mut attempt);
    assert!(f.journal.save_unsent(&attempt.unsent(1004)).is_err());
}

#[test]
fn permalink_admission_uses_existing_enterprise_canonicalizer_without_foreign_origin() {
    let mut f = Fixture::new();
    let record = f.journal.create(intent(), 1000).unwrap();
    let mut attempt = f.journal.reserve(&record, false, None, 1001).unwrap();
    let mut enterprise = workspace();
    enterprise.url = "https://fixture.enterprise.slack.com".into();
    let mut foreign = enterprise.clone();
    foreign.url = "https://other.enterprise.slack.com".into();
    assert!(
        attempt
            .take_request(&SlackClient::default(), &foreign)
            .is_err()
    );
    let mut invalid = enterprise.clone();
    invalid.url = "http://fixture.enterprise.slack.com".into();
    assert!(
        attempt
            .take_request(&SlackClient::default(), &invalid)
            .is_err()
    );
    invalid.url = "https://fixture.enterprise.slack.com:443".into();
    assert!(
        attempt
            .take_request(&SlackClient::default(), &invalid)
            .is_err()
    );
    assert!(
        attempt
            .take_request(&SlackClient::default(), &enterprise)
            .is_ok()
    );
}

#[test]
fn explicit_conflict_decision_preserves_requested_input_and_still_needs_preflight() {
    let mut f = Fixture::new();
    let original = confirmed(&mut f);
    let mut remote = raw(&original);
    remote["draft"]["blocks"] = json!([{ "type":"remote_future", "value":[3,1]}]);
    let conflict = f
        .journal
        .observe_preflight(&original, &remote, 1010)
        .unwrap();
    assert_eq!(conflict.phase(), Phase::Conflict);
    let ready = f
        .journal
        .resolve_conflict(&conflict, &remote, 1011)
        .unwrap();
    assert_eq!(ready.phase(), Phase::Ready);
    assert_eq!(ready.intent(), original.intent());
    assert_eq!(ready.reviewed(), Some(&remote));
    assert_eq!(ready.observed(), Some(&remote));
    assert!(
        f.journal
            .resolve_conflict(&conflict, &remote, 1012)
            .is_err()
    );
    assert!(
        f.journal
            .reserve(&ready, false, Some(&raw(&original)), 1012)
            .is_err()
    );
    let mut attempt = f
        .journal
        .reserve(&ready, false, Some(&remote), 1012)
        .unwrap();
    let fields = form(&mut attempt);
    assert_eq!(
        fields["blocks"],
        serde_json::to_string(original.intent().blocks()).unwrap()
    );
    assert_eq!(fields["draft_id"], "Dr1");
    assert!(
        attempt
            .take_request(&SlackClient::default(), &workspace())
            .is_err()
    );
}

#[test]
fn conflict_resolution_refuses_newer_partial_foreign_terminal_and_cooling_reads() {
    let mut f = Fixture::new();
    let original = confirmed(&mut f);
    let mut remote = raw(&original);
    remote["draft"]["date_scheduled"] = 2100.into();
    let conflict = f
        .journal
        .observe_preflight(&original, &remote, 1010)
        .unwrap();
    for changed in [
        json!({"ok":true}),
        {
            let mut v = remote.clone();
            v["draft"]["id"] = "Dr2".into();
            v
        },
        {
            let mut v = remote.clone();
            v["draft"]["last_updated_ts"] = "newer".into();
            v
        },
    ] {
        assert!(
            f.journal
                .resolve_conflict(&conflict, &changed, 1011)
                .is_err()
        );
        assert_eq!(
            f.journal.get(conflict.id()).unwrap(),
            Some(conflict.clone())
        );
    }
    let cooling = f.journal.defer_read(&conflict, 60, 1011).unwrap();
    assert!(f.journal.resolve_conflict(&cooling, &remote, 1070).is_err());
    let mut current = cooling;
    for (index, field) in ["is_deleted", "is_sent"].into_iter().enumerate() {
        let at = 1071 + index as u64 * 2;
        let mut terminal = remote.clone();
        terminal["draft"][field] = true.into();
        let observed = f.journal.review(&current, &terminal, at).unwrap();
        assert!(
            f.journal
                .resolve_conflict(&observed, &terminal, at + 1)
                .is_err()
        );
        current = f.journal.review(&observed, &remote, at + 1).unwrap();
    }
}

#[test]
fn conflict_decision_compares_complete_draft_not_response_metadata() {
    let mut f = Fixture::new();
    let original = confirmed(&mut f);
    let mut remote = raw(&original);
    remote["draft"]["date_scheduled"] = 2100.into();
    remote["response_metadata"] = json!({"request_id":"original-read"});
    let conflict = f
        .journal
        .observe_preflight(&original, &remote, 1010)
        .unwrap();
    let mut fresh = remote.clone();
    fresh["response_metadata"] = json!({"request_id":"fresh-read","warnings":[]});
    for invalid in [
        {
            let mut v = fresh.clone();
            v["ok"] = false.into();
            v
        },
        {
            let mut v = fresh.clone();
            v.as_object_mut().unwrap().remove("ok");
            v
        },
        {
            let mut v = fresh.clone();
            v["draft"]["unknown_future_field"] = json!([1, 2]);
            v
        },
    ] {
        assert!(
            f.journal
                .resolve_conflict(&conflict, &invalid, 1011)
                .is_err()
        );
        assert_eq!(
            f.journal.get(conflict.id()).unwrap(),
            Some(conflict.clone())
        );
    }
    let ready = f.journal.resolve_conflict(&conflict, &fresh, 1012).unwrap();
    assert_eq!(ready.intent(), original.intent());
    assert_eq!(ready.observed(), Some(&remote));
    assert_eq!(ready.reviewed(), Some(&fresh));
    assert_eq!(ready.phase(), Phase::Ready);
}
