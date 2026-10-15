//! Synthetic protocol/controller tests. Never load a session or send HTTP.
use super::{api::*, catalog::Catalog, controller::*, *};
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport, client::RequestBody},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn owner() -> Owner {
    Owner {
        account: "c765fef0-1f85-49f0-ab9c-fdb779292337".into(),
        team: "T1".into(),
        user: "U1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com".into(),
        token: "synthetic-token".into(),
    }
}
fn row() -> Value {
    json!({"name":"/example","canonical_name":"/canonical","type":"app","app":"A1",
        "teams":["T1"],"is_runnable_from_thread":true,"future_metadata":{"retained":true}})
}
fn raw(rows: Vec<Value>) -> Value {
    json!({"ok":true,"commands":rows,"app_actions":[],"cache_ts":"opaque"})
}
fn catalog() -> Catalog {
    Catalog::decode(owner(), raw(vec![row()])).unwrap()
}
fn blocks() -> Value {
    json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
    {"type":"text","text":"  argument 🌱 "},{"type":"user","user_id":"U2"}]}]}])
}
fn frozen(catalog: &Catalog) -> Frozen {
    Frozen::capture(
        catalog.owner().clone(),
        7,
        json!({"ops":[{"insert":"/EXAMPLE  argument 🌱 "}]}),
        "/EXAMPLE".into(),
        Some(blocks()),
        &catalog.choice(0).unwrap(),
        catalog,
    )
    .unwrap()
}
fn admission(frozen: &Frozen, catalog: &Catalog) -> Admission {
    Admission {
        intent: String::new(),
        frozen: frozen.clone(),
        catalog: catalog.clone(),
        transport: Arc::new(Transport::new("synthetic-cookie").unwrap()),
        session: session(),
        client: SlackClient::default(),
        alive: Arc::new(AtomicBool::new(true)),
    }
}
// Synthetic fixture only. Production Admission has no Clone or constructor;
// each arm consumes the opaque protected execution claim, even on failure.
fn fresh(a: &Admission, record: &Record) -> Admission {
    Admission {
        intent: record.id().to_owned(),
        frozen: a.frozen.clone(),
        catalog: a.catalog.clone(),
        transport: a.transport.clone(),
        session: a.session.clone(),
        client: a.client.clone(),
        alive: a.alive.clone(),
    }
}
#[derive(Default)]
struct Memory {
    stored: Option<Record>,
    fail: bool,
    cooldown: bool,
    revoke: Option<Arc<AtomicBool>>,
    calls: usize,
}
impl Journal for Memory {
    async fn compare_exchange(
        &mut self,
        old: Option<&Record>,
        next: &Record,
    ) -> Result<(), Failure> {
        self.calls += 1;
        if self.fail {
            return Err(Failure::Persistence);
        }
        // Models the required adapter gate, not a production cooldown clock.
        if self.cooldown && matches!(next.phase(), Phase::DispatchPossible) {
            return Err(Failure::RateLimited);
        }
        if self.stored.as_ref() == Some(next) {
            return Ok(());
        }
        if self.stored.as_ref() != old {
            return Err(Failure::Stale);
        }
        self.stored = Some(next.clone());
        if let Some(alive) = self.revoke.take() {
            alive.store(false, Ordering::Release);
        }
        Ok(())
    }
}

#[test]
fn catalog_preserves_unknown_rows_and_raw_metadata() {
    let mut unsupported = row();
    unsupported["type"] = json!("custom");
    let wire = raw(vec![row(), unsupported]);
    let catalog = Catalog::decode(owner(), wire.clone()).unwrap();
    assert!(catalog.raw() == &wire && catalog.commands().len() == 1 && catalog.unsupported() == 1);
    assert!(catalog.commands()[0].raw["future_metadata"]["retained"] == true);
}
#[test]
fn same_name_different_apps_remain_distinct_duplicate_identity_rejected() {
    let mut other = row();
    other["app"] = json!("A2");
    assert_eq!(
        Catalog::decode(owner(), raw(vec![row(), other]))
            .unwrap()
            .commands()
            .len(),
        2
    );
    assert!(matches!(
        Catalog::decode(owner(), raw(vec![row(), row()])),
        Err(Failure::Invalid)
    ));
}
#[test]
fn invalid_or_partial_catalog_cannot_publish_empty_success() {
    for wire in [
        json!({"ok":true,"commands":[]}),
        json!({"ok":true,"commands":[],"app_actions":[],"error":null}),
        raw(vec![Value::Null]),
        json!({"ok":"true","commands":[],"app_actions":[]}),
    ] {
        assert!(Catalog::decode(owner(), wire).is_err());
    }
}
#[test]
fn old_choice_rejected_after_equal_catalog_replacement_and_account_return() {
    let first = catalog();
    let choice = first.choice(0).unwrap();
    let replacement = catalog();
    assert!(matches!(
        choice.admit(&replacement, &owner()),
        Err(Failure::Stale)
    ));
    let mut other = owner();
    other.account = "b120f5ab-6928-414a-a7eb-cb372aa3e6a8".into();
    assert!(matches!(choice.admit(&first, &other), Err(Failure::Stale)));
    // Returning to the same logical owner still uses a fresh allocation.
    assert!(matches!(
        choice.admit(&catalog(), &owner()),
        Err(Failure::Stale)
    ));
}
#[test]
fn thread_and_team_ambiguity_require_positive_support() {
    let mut thread = owner();
    thread.thread = Some("123.000004".into());
    let mut no_thread = row();
    no_thread
        .as_object_mut()
        .unwrap()
        .remove("is_runnable_from_thread");
    let c = Catalog::decode(thread.clone(), raw(vec![no_thread])).unwrap();
    assert!(matches!(
        c.choice(0).unwrap().admit(&c, &thread),
        Err(Failure::Unsupported)
    ));
    let mut many = row();
    many["teams"] = json!(["T1", "T2"]);
    let c = Catalog::decode(owner(), raw(vec![many])).unwrap();
    assert!(matches!(
        c.choice(0).unwrap().admit(&c, &owner()),
        Err(Failure::Unsupported)
    ));
}
#[test]
fn aliases_and_boolean_coercion_stay_unsupported() {
    let mut alias = row();
    alias["alias_of"] = json!("/canonical");
    let mut string_bool = row();
    string_bool["is_runnable_from_thread"] = json!("true");
    let c = Catalog::decode(owner(), raw(vec![alias, string_bool])).unwrap();
    assert!(c.commands().is_empty() && c.unsupported() == 2);
}
#[test]
fn source_form_uses_metadata_name_original_display_rich_blocks_and_exact_fields() {
    let mut own = owner();
    own.thread = Some("123.000004".into());
    let c = Catalog::decode(own, raw(vec![row()])).unwrap();
    let f = frozen(&c);
    let req = prepare(
        &SlackClient::default(),
        &session(),
        &f,
        "opaque-correlation",
    )
    .unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!("form required")
    };
    let actual: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    let expected = std::collections::BTreeMap::from([
        ("token".to_owned(), "synthetic-token".to_owned()),
        ("command".into(), "/example".into()),
        ("disp".into(), "/EXAMPLE".into()),
        ("channel".into(), "C1".into()),
        ("client_token".into(), "opaque-correlation".into()),
        ("app_id".into(), "A1".into()),
        ("team_id".into(), "T1".into()),
        ("_x_reason".into(), "executeCommand".into()),
        ("blocks".into(), blocks().to_string()),
        ("thread_ts".into(), "123.000004".into()),
    ]);
    assert_eq!(actual, expected);
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/chat.command"
    );
}
#[test]
fn absent_arguments_and_discovery_cache_timestamp_are_omitted() {
    let c = catalog();
    let mut f = frozen(&c);
    f.argument_blocks = None;
    let req = prepare(&SlackClient::default(), &session(), &f, "token").unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!("form required")
    };
    assert!(!fields.iter().any(|(k, _)| matches!(
        k.as_str(),
        "text" | "blocks" | "thread_ts" | "client_msg_id" | "draft_id"
    )));
    let req = discovery(&SlackClient::default(), &session());
    let RequestBody::Form(fields) = req.body else {
        panic!("form required")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic-token".into()),
            (
                "_x_reason".into(),
                "app-commands-conditional-fetching".into()
            )
        ]
    );
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/client.appCommands"
    );
}
#[test]
fn prepare_rejects_session_and_rich_argument_mismatch() {
    let c = catalog();
    let f = frozen(&c);
    let mut wrong = session();
    wrong.user_id = "U2".into();
    assert!(matches!(
        prepare(&SlackClient::default(), &wrong, &f, "token"),
        Err(Failure::Stale)
    ));
    let mut lossy = f.clone();
    lossy.argument_blocks = Some(json!("plain replacement"));
    assert!(prepare(&SlackClient::default(), &session(), &lossy, "token").is_err());
    let mut wrong = f;
    wrong.display = "/another".into();
    assert!(prepare(&SlackClient::default(), &session(), &wrong, "token").is_err());
}
#[test]
fn strict_json_rejects_duplicates_nested_duplicates_size_and_trailing_content() {
    for bytes in [
        br#"{"ok":true,"ok":false}"#.as_slice(),
        br#"{"nested":{"x":1,"x":2}}"#,
        br#"{} {}"#,
        br#"{"unclosed":[]"#,
    ] {
        assert!(codec::decode(bytes, 256).is_err());
    }
    assert!(codec::decode(b"{}", 1).is_err());
    assert_eq!(
        codec::decode(br#"{"ok":true,"n":[null,1,-2,1.5]}"#, 256).unwrap(),
        json!({"ok":true,"n":[null,1,-2,1.5]})
    );
}
#[test]
fn unknown_reply_preserves_bounded_evidence_without_coercion() {
    for raw in [
        json!({"ok":true,"keep_input":"false"}),
        json!({"ok":true,"response":{}}),
        json!({"ok":true,"error":"contradiction"}),
        json!({"ok":false,"error":"dispatch_failed"}),
        json!({"ok":false,"error":"unknown_command","keep_input":true}),
        json!({"ok":null}),
    ] {
        assert!(matches!(decode(raw.clone()),Reply::Unknown {raw:Some(v)} if v==raw));
    }
}
#[test]
fn typed_ack_and_rejection_retain_full_raw_evidence() {
    let raw =
        json!({"ok":true,"keep_input":true,"response":"Synthetic feedback","data":{"unknown":1}});
    assert!(
        matches!(decode(raw.clone()),Reply::Acknowledged {keep_input:true,feedback:Some(_),raw:v} if v==raw)
    );
    assert!(matches!(
        decode(json!({"ok":false,"error":"restricted_in_threads"})),
        Reply::Rejected { .. }
    ));
}
#[tokio::test]
async fn durable_prepare_and_arm_failures_yield_no_dispatch_permit() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory {
        fail: true,
        ..Default::default()
    };
    assert!(matches!(
        Controller::reserve(f.clone(), c.clone(), &mut disk).await,
        Err(Failure::Persistence)
    ));
    disk.fail = false;
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    disk.fail = true;
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::Persistence)
    ));
    assert!(matches!(ctl.record().phase(), Phase::Prepared));
}
#[tokio::test]
async fn replacement_catalog_and_changed_snapshot_cannot_arm() {
    let c = catalog();
    let f = frozen(&c);
    let mut a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c.clone(), &mut disk).await.unwrap();
    a.catalog = catalog();
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::Stale)
    ));
    a.catalog = c;
    a.frozen.original = json!({"ops":[{"insert":"newer input"}]});
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::Stale)
    ));
    assert_eq!(disk.calls, 1);
}
#[tokio::test]
async fn owner_invalidation_during_persistence_leaves_unknown_without_permit() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    disk.revoke = Some(a.alive.clone());
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::Stale)
    ));
    assert!(ctl.recovery_required());
    assert!(matches!(ctl.record().phase(), Phase::DispatchPossible));
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::NoReplay)
    ));
}
#[tokio::test]
async fn one_permit_binds_exact_transport_and_irrevocable_liveness() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    let permit = ctl
        .arm(fresh(&a, ctl.record()), "token", &mut disk)
        .await
        .unwrap();
    assert!(permit.current(&a.transport));
    assert!(!permit.current(&a.transport.as_ref().clone()));
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::NoReplay)
    ));
    a.alive.store(false, Ordering::Release);
    assert!(!permit.current(&a.transport));
    ctl.finish(permit.report(Delivery::NotSent, None)).unwrap();
    ctl.flush(&mut disk).await.unwrap();
    assert!(matches!(ctl.record().phase(), Phase::NotSent));
}
#[tokio::test]
async fn every_restart_phase_never_rearms_even_prepared() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    let mut restored = Controller::restore(ctl.record().clone()).unwrap();
    assert!(matches!(
        restored
            .arm(fresh(&a, restored.record()), "token", &mut disk)
            .await,
        Err(Failure::NoReplay)
    ));
    let permit = ctl
        .arm(fresh(&a, ctl.record()), "token", &mut disk)
        .await
        .unwrap();
    let mut restored = Controller::restore(ctl.record().clone()).unwrap();
    assert!(matches!(
        restored
            .arm(fresh(&a, restored.record()), "token", &mut disk)
            .await,
        Err(Failure::NoReplay)
    ));
    ctl.finish(permit.report(Delivery::Unknown, None)).unwrap();
    ctl.flush(&mut disk).await.unwrap();
    let mut restored = Controller::restore(ctl.record().clone()).unwrap();
    assert!(matches!(
        restored
            .arm(fresh(&a, restored.record()), "token", &mut disk)
            .await,
        Err(Failure::NoReplay)
    ));
    assert!(matches!(
        restored.retire(&mut disk).await,
        Err(Failure::NoReplay)
    ));
}
#[tokio::test]
async fn acknowledged_keep_input_persistence_retries_only_flush_preserves_original() {
    let c = catalog();
    let f = frozen(&c);
    let original = f.original.clone();
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    let permit = ctl
        .arm(fresh(&a, ctl.record()), "token", &mut disk)
        .await
        .unwrap();
    ctl.finish(permit.report(Delivery::Reply(json!({"ok":true,"keep_input":true})), None))
        .unwrap();
    disk.fail = true;
    assert_eq!(ctl.flush(&mut disk).await, Err(Failure::Persistence));
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::NoReplay)
    ));
    disk.fail = false;
    ctl.flush(&mut disk).await.unwrap();
    assert!(matches!(
        ctl.record().phase(),
        Phase::Completed(Reply::Acknowledged {
            keep_input: true,
            ..
        })
    ));
    assert!(ctl.record().original() == &original);
    // No composer mutable reference or clear/release operation exists here.
    ctl.retire(&mut disk).await.unwrap();
    assert!(ctl.record().retired());
    assert!(matches!(
        ctl.record().phase(),
        Phase::Completed(Reply::Acknowledged {
            keep_input: true,
            ..
        })
    ));
}
#[tokio::test]
async fn foreign_receipt_cannot_finish_current_operation() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut first = Controller::reserve(f.clone(), c.clone(), &mut disk)
        .await
        .unwrap();
    let permit = first
        .arm(fresh(&a, first.record()), "token", &mut disk)
        .await
        .unwrap();
    let mut other_disk = Memory::default();
    let mut other = Controller::reserve(f, c, &mut other_disk).await.unwrap();
    assert!(matches!(
        other
            .arm(fresh(&a, first.record()), "token", &mut other_disk)
            .await,
        Err(Failure::Stale)
    ));
    assert!(matches!(
        other.finish(permit.report(Delivery::Unknown, None)),
        Err(Failure::Stale)
    ));
}
#[tokio::test]
async fn rate_limit_receipt_retains_status_and_cooldown_admission_is_obeyed() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let mut ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    disk.cooldown = true;
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::RateLimited)
    ));
    disk.cooldown = false;
    let permit = ctl
        .arm(fresh(&a, ctl.record()), "token", &mut disk)
        .await
        .unwrap();
    ctl.finish(permit.report(Delivery::HttpFailure(429), Some(37)))
        .unwrap();
    ctl.flush(&mut disk).await.unwrap();
    assert_eq!(ctl.record().http_status(), Some(429));
    assert_eq!(ctl.record().retry_after_secs(), Some(37));
    assert!(ctl.recovery_required());
    assert!(matches!(
        ctl.arm(fresh(&a, ctl.record()), "token", &mut disk).await,
        Err(Failure::NoReplay)
    ));
}
#[test]
fn managed_admission_is_explicitly_unavailable() {
    assert!(matches!(
        managed_admission_unavailable(),
        Err(Failure::AdmissionUnavailable)
    ));
    // Real managed Review/Writer/Binding types intentionally have no adapter.
}

#[tokio::test]
async fn stored_receipt_roundtrip_keeps_raw_input_and_restores_without_replay() {
    let c = catalog();
    let f = frozen(&c);
    let a = admission(&f, &c);
    let mut disk = Memory::default();
    let ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    let bytes = serde_json::to_vec(ctl.record()).unwrap();
    let record = Record::decode(&bytes).unwrap();
    assert!(record == *ctl.record());
    assert_eq!(format!("{:?}", record), "SlashRecord(..)");
    let mut restored = Controller::restore(record).unwrap();
    assert!(matches!(
        restored
            .arm(fresh(&a, restored.record()), "token", &mut disk)
            .await,
        Err(Failure::NoReplay)
    ));
    let mut wire = serde_json::to_value(ctl.record()).unwrap();
    wire["phase"] = json!({"Completed":{"Acknowledged":{"keep_input":false,"feedback":null,"raw":{"ok":false,"error":"dispatch_failed"}}}});
    let forged = Record::decode(&serde_json::to_vec(&wire).unwrap()).unwrap();
    assert!(matches!(Controller::restore(forged), Err(Failure::Invalid)));
}

#[test]
fn byte_catalog_rejects_duplicates_and_hidden_or_enterprise_choices_are_unavailable() {
    assert!(
        Catalog::decode_bytes(
            owner(),
            br#"{"ok":true,"ok":false,"commands":[],"app_actions":[]}"#
        )
        .is_err()
    );
    let mut hidden = row();
    hidden["autocomplete"] = json!(false);
    let c = Catalog::decode(owner(), raw(vec![hidden])).unwrap();
    assert!(matches!(
        c.choice(0).unwrap().admit(&c, &owner()),
        Err(Failure::Unsupported)
    ));
    let mut enterprise = row();
    enterprise["is_enterprise_install"] = json!(true);
    let c = Catalog::decode(owner(), raw(vec![enterprise])).unwrap();
    assert!(c.commands().is_empty() && c.unsupported() == 1);
}

#[tokio::test]
async fn directly_constructed_hidden_frozen_input_cannot_prepare_or_reserve() {
    let mut hidden = row();
    hidden["autocomplete"] = json!(false);
    let c = Catalog::decode(owner(), raw(vec![hidden])).unwrap();
    let mut f = frozen(&catalog());
    f.command = c.commands()[0].clone();
    assert!(matches!(
        prepare(&SlackClient::default(), &session(), &f, "token"),
        Err(Failure::Unsupported)
    ));
    let mut disk = Memory::default();
    assert!(matches!(
        Controller::reserve(f, c, &mut disk).await,
        Err(Failure::Unsupported)
    ));
    assert_eq!(disk.calls, 0);
}

#[tokio::test]
async fn stored_receipt_requires_explicit_http_metadata_and_consistent_phase() {
    let c = catalog();
    let f = frozen(&c);
    let mut disk = Memory::default();
    let ctl = Controller::reserve(f, c, &mut disk).await.unwrap();
    let base = serde_json::to_value(ctl.record()).unwrap();
    for key in ["http_status", "retry_after_secs"] {
        let mut raw = base.clone();
        raw.as_object_mut().unwrap().remove(key);
        assert!(Record::decode(&serde_json::to_vec(&raw).unwrap()).is_err());
    }
    for phase in [
        json!("Prepared"),
        json!("NotSent"),
        json!("DispatchPossible"),
        json!({"Completed":{"Acknowledged":{"keep_input":false,"feedback":null,"raw":{"ok":true}}}}),
    ] {
        let mut raw = base.clone();
        raw["phase"] = phase;
        raw["http_status"] = json!(429);
        raw["retry_after_secs"] = json!(37);
        let record = Record::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(matches!(Controller::restore(record), Err(Failure::Invalid)));
    }
    let mut invalid = base.clone();
    invalid["phase"] = json!("Unknown");
    invalid["http_status"] = json!(999);
    assert!(matches!(
        Controller::restore(Record::decode(&serde_json::to_vec(&invalid).unwrap()).unwrap()),
        Err(Failure::Invalid)
    ));
    let mut unknown = base;
    unknown["phase"] = json!("Unknown");
    unknown["http_status"] = json!(429);
    unknown["retry_after_secs"] = json!(37);
    let restored =
        Controller::restore(Record::decode(&serde_json::to_vec(&unknown).unwrap()).unwrap())
            .unwrap();
    assert!(restored.recovery_required());
    assert_eq!(restored.record().http_status(), Some(429));
}
