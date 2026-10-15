use super::*;
#[path = "ordinary_flag_tests.rs"]
mod ordinary_flag;
use serde_json::json;
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

struct Fixture {
    account: Arc<Account>,
    owner: Owner,
    path: std::path::PathBuf,
    local: LocalDraft,
    remote: RemoteDraft,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_remote(true).await
    }
    async fn with_remote(adopt: bool) -> Self {
        let scope = Scope {
            account_id: "00000000-0000-4000-8000-000000000071".into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: Some("100.000001".into()),
            },
        };
        let path = std::env::temp_dir().join(format!(
            "draft-observation-{}.sqlite",
            DraftSync::new(scope.clone()).client_msg_id()
        ));
        let local = LocalDraft {
            composer: ComposerState::with_text("Original"),
            attachments: vec![],
        };
        let remote = RemoteDraft {
            scope: scope.clone(),
            id: "Dr1".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000072".into(),
            last_updated_ts: "1000".into(),
            snapshot: codec::encode(&scope.key, "Original").unwrap(),
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(scope.key.clone(), local.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: scope.account_id.clone(),
            path: path.clone(),
        });
        let owner = account.owner(scope.key);
        owner
            .enroll(local.clone(), adopt.then(|| remote.clone()))
            .await
            .unwrap();
        Self {
            account,
            owner,
            path,
            local,
            remote,
        }
    }
    fn raw(&self, text: &str, stamp: &str) -> RawEvent {
        let snapshot = codec::encode(&self.owner.scope.key, text).unwrap();
        serde_json::from_value(
            json!({"type":"draft_update", "draft_id":"Dr1", "event_ts":stamp,
            "client_msg_id":self.remote.client_msg_id, "draft":{
                "id":"Dr1", "client_msg_id":self.remote.client_msg_id,
                "team_id":"T1", "user_id":"U1", "last_updated_ts":"2000",
                "is_deleted":false, "is_sent":false, "date_scheduled":0,
                "blocks":snapshot.blocks, "file_ids":[], "destinations":snapshot.destinations
            }}),
        )
        .unwrap()
    }
    fn prepared(&self, raw: &RawEvent) -> Prepared {
        decode(&self.owner.scope, "U1", raw).unwrap()
    }
    fn restart(&self) -> Owner {
        Account::new(self.account.backend.clone()).owner(self.owner.scope.key.clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn clean_observation_persists_pair_and_ordering_before_return_and_restart() {
    let f = Fixture::new().await;
    let raw = f.raw("Remote 日本語 🚀", "10.000002");
    let result = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
    assert!(result.changed);
    assert!(same_local(&result.before.local, &f.local));
    assert_eq!(result.after.local.composer.text, "Remote 日本語 🚀");
    let restored = f.restart().load().await.unwrap();
    assert!(same_local(&restored.local, &result.after.local));
    assert_eq!(
        restored.stored.unwrap().state,
        result.after.stored.as_ref().unwrap().state
    );
    let duplicate = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
    assert!(!duplicate.changed);
    assert_eq!(
        duplicate.after.stored.as_ref().unwrap().revision,
        result.after.stored.as_ref().unwrap().revision
    );
    let stale = f
        .owner
        .observe_event(f.prepared(&f.raw("Old", "10.000001")))
        .await
        .unwrap();
    assert!(!stale.changed);
    assert!(same_local(&stale.after.local, &result.after.local));
}

#[tokio::test]
async fn dirty_input_is_preserved_when_observation_and_checkpoint_compete() {
    let f = Fixture::new().await;
    let initial = f.owner.load().await.unwrap().stored.unwrap();
    let typed = LocalDraft {
        composer: ComposerState::with_text("Local change"),
        attachments: vec![],
    };
    f.owner
        .checkpoint(initial.revision, typed.clone())
        .await
        .unwrap();
    let observed = f
        .owner
        .observe_event(f.prepared(&f.raw("Remote change", "10.000001")))
        .await
        .unwrap();
    assert!(same_local(&observed.after.local, &typed));
    assert!(matches!(
        observed.after.stored.as_ref().unwrap().state.workflow(),
        super_platinum_core::drafts::sync::Workflow::Conflict {
            remote: Some(_),
            ..
        }
    ));
    assert!(matches!(
        f.owner.checkpoint(initial.revision, f.local.clone()).await,
        Err(Fault::Stale)
    ));
    assert!(same_local(&f.restart().load().await.unwrap().local, &typed));
}

#[tokio::test]
async fn unknown_matched_lifecycle_or_partial_content_blocks_without_erasing_local() {
    for field in [
        "is_sent",
        "date_scheduled",
        "attachments",
        "missing_blocks",
        "unknown_field",
        "destination",
    ] {
        let f = Fixture::new().await;
        let mut raw = f.raw("Remote", "10.000001");
        let draft = raw.rest.get_mut("draft").unwrap();
        match field {
            "is_sent" => draft["is_sent"] = true.into(),
            "date_scheduled" => draft["date_scheduled"] = 123.into(),
            "attachments" => draft["attachments"] = json!([{"text":"Keep opaque evidence"}]),
            "missing_blocks" => {
                draft.as_object_mut().unwrap().remove("blocks");
            }
            "unknown_field" => draft["future_semantics"] = json!({"keep":true}),
            "destination" => draft["destinations"][0]["channel_id"] = "C2".into(),
            _ => unreachable!(),
        }
        let evidence = draft.clone();
        let observed = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
        assert!(same_local(&observed.after.local, &f.local));
        let stored = observed.after.stored.unwrap();
        assert_eq!(
            stored.state.blocked_observation().unwrap().kind,
            ObservationKind::Unsupported(evidence)
        );
        assert!(matches!(
            f.owner.release(stored.revision).await,
            Err(Fault::Pending)
        ));
        assert!(matches!(
            f.owner
                .dispatch(stored.revision, |_| async { panic!("must not dispatch") })
                .await,
            Err(Fault::Pending)
        ));
        assert_eq!(
            f.restart().load().await.unwrap().stored.unwrap().state,
            stored.state
        );
    }
}

#[tokio::test]
async fn source_identity_mismatches_and_unowned_server_identity_do_not_write() {
    let f = Fixture::new().await;
    let before = f.owner.load().await.unwrap().stored.unwrap();
    for field in ["user_id", "team_id", "client_msg_id", "id"] {
        let mut raw = f.raw("Foreign", "10.000001");
        raw.rest.get_mut("draft").unwrap()[field] = "wrong".into();
        assert!(decode(&f.owner.scope, "U1", &raw).is_err());
    }
    let mut foreign = f.raw("Foreign", "10.000001");
    foreign.rest.insert("draft_id".into(), "DrOther".into());
    foreign.rest.get_mut("draft").unwrap()["id"] = "DrOther".into();
    assert!(matches!(
        f.owner.observe_event(f.prepared(&foreign)).await,
        Err(Fault::InvalidReceipt)
    ));
    // A missing client UUID is admitted but remains unresolved before storage.
    let deletion: RawEvent = serde_json::from_value(
        json!({"type":"draft_delete", "draft_id":"Dr1", "event_ts":"10.000001"}),
    )
    .unwrap();
    assert!(decode(&f.owner.scope, "U1", &deletion).is_ok());
    assert_eq!(
        f.owner.load().await.unwrap().stored.unwrap().revision,
        before.revision
    );
}

#[tokio::test]
async fn admitted_observation_cannot_be_redirected_to_another_account_owner() {
    let f = Fixture::new().await;
    let mut key = f.owner.scope.key.clone();
    key.channel = "C2".into();
    let wrong_owner = f.account.owner(key);
    assert!(matches!(
        wrong_owner
            .observe_event(f.prepared(&f.raw("New", "10.000001")))
            .await,
        Err(Fault::InvalidReceipt)
    ));
    let other_path = f.path.with_extension("other.sqlite");
    let other_account = Account::new(Backend::Isolated {
        account: "00000000-0000-4000-8000-000000000099".into(),
        path: other_path.clone(),
    });
    let other_owner = other_account.owner(f.owner.scope.key.clone());
    assert!(matches!(
        other_owner
            .observe_event(f.prepared(&f.raw("New", "10.000001")))
            .await,
        Err(Fault::InvalidReceipt)
    ));
    assert!(
        !other_path.exists(),
        "scope rejection precedes storage access"
    );
    assert!(same_local(&f.owner.load().await.unwrap().local, &f.local));
}

#[test]
fn captured_socket_source_rejects_account_transport_credential_and_generation_changes() {
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
    };
    for change in [
        "account",
        "epoch",
        "transport",
        "token",
        "user",
        "generation",
    ] {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let team = shell.core.active_team.clone().unwrap();
        let user = shell.core.workspaces[&team].self_user_id.clone();
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = 0;
        shell.core.active_account = Some("00000000-0000-4000-8000-000000000071".into());
        shell.core.account_epoch = 7;
        shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
        shell.core.session = Some(Session {
            d_cookie: "fixture-only-cookie".into(),
            workspaces: [(
                team.clone(),
                WorkspaceSession {
                    team_id: team.clone(),
                    enterprise_id: None,
                    user_id: user,
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "fixture-only-token".into(),
                },
            )]
            .into(),
        });
        let params = super_platinum_core::slack::realtime::ConnectParams {
            team: team.clone(),
            ws_url: super_platinum_core::slack::realtime::flannel_url("fixture-only-token", &team),
            d_cookie: "fixture-only-cookie".into(),
            user_agent: "fixture-only-agent".into(),
        };
        let mut worker =
            crate::drafts::observation::WorkerSource::capture(&mut shell, params).unwrap();
        worker.take_connect_params(&shell).unwrap();
        assert!(worker.can_accept(&shell, &team, 9));
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = 9;
        let source = worker.accept(&mut shell, &team, 9).unwrap();
        let key = DraftKey {
            team: team.clone(),
            channel: "C1".into(),
            thread: None,
        };
        let raw: RawEvent = serde_json::from_value(json!({"type":"draft_delete", "draft_id":"Dr1",
            "client_msg_id":"00000000-0000-4000-8000-000000000072", "event_ts":"10.000001"}))
        .unwrap();
        assert!(source.prepare(&shell, 9, &key, &raw).is_ok());
        match change {
            "account" => {
                shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into())
            }
            "epoch" => shell.core.account_epoch += 1,
            "transport" => {
                shell.core.transport =
                    Some(Arc::new(Transport::new("other-fixture-cookie").unwrap()))
            }
            "token" => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "rotated-fixture-token".into()
            }
            "user" => shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "UOTHER".into(),
            "generation" => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            _ => unreachable!(),
        }
        assert!(matches!(
            source.prepare(&shell, 9, &key, &raw),
            Err(Invalid::Source)
        ));
    }
}

fn deletion(id: &str, stamp: &str) -> RawEvent {
    serde_json::from_value(json!({"type":"draft_delete", "draft_id":id, "event_ts":stamp})).unwrap()
}

#[tokio::test]
async fn uuidless_bound_delete_resolves_durable_identity_and_preserves_restart_ordering() {
    let f = Fixture::new().await;
    f.owner
        .observe_event(f.prepared(&f.raw("Latest", "10.000002")))
        .await
        .unwrap();
    let stale = f
        .owner
        .observe_event(f.prepared(&deletion("Dr1", "10.000001")))
        .await
        .unwrap();
    assert!(!stale.changed);
    assert_eq!(stale.after.local.composer.text, "Latest");
    let observed = f
        .owner
        .observe_event(f.prepared(&deletion("Dr1", "10.000003")))
        .await
        .unwrap();
    assert!(observed.changed);
    assert!(observed.after.local.empty());
    let stored = observed.after.stored.unwrap();
    assert!(stored.state.acknowledged().is_none());
    assert_eq!(stored.state.observed_event_ts("Dr1"), Some("10.000003"));
    let restarted = f.restart();
    assert_eq!(
        restarted.load().await.unwrap().stored.unwrap().state,
        stored.state
    );
    for stamp in ["10.000003", "10.000001"] {
        let duplicate = restarted
            .observe_event(f.prepared(&deletion("Dr1", stamp)))
            .await
            .unwrap();
        assert!(!duplicate.changed);
        assert_eq!(duplicate.after.stored.unwrap().revision, stored.revision);
        assert!(duplicate.after.local.empty());
    }
    let late = restarted
        .observe_event(f.prepared(&f.raw("Late stale resurrection", "99.000001")))
        .await
        .unwrap();
    assert!(!late.changed);
    assert!(late.after.local.empty());
    // A later unmatched delete is not a replay of the retained retirement. No
    // fresh UUID is inferred from the destination or another pending draft.
    assert!(matches!(
        restarted
            .observe_event(f.prepared(&deletion("Dr1", "99.000002")))
            .await,
        Err(Fault::InvalidReceipt)
    ));
}

#[tokio::test]
async fn uuidless_delete_of_dirty_bound_draft_preserves_local_conflict() {
    let f = Fixture::new().await;
    let stored = f.owner.load().await.unwrap().stored.unwrap();
    let dirty = LocalDraft {
        composer: ComposerState::with_text("Keep local input"),
        attachments: vec![],
    };
    f.owner
        .checkpoint(stored.revision, dirty.clone())
        .await
        .unwrap();
    let observed = f
        .owner
        .observe_event(f.prepared(&deletion("Dr1", "10.000001")))
        .await
        .unwrap();
    assert!(same_local(&observed.after.local, &dirty));
    assert!(matches!(
        observed.after.stored.as_ref().unwrap().state.workflow(),
        super_platinum_core::drafts::sync::Workflow::Conflict { remote: None, .. }
    ));
    assert!(same_local(&f.restart().load().await.unwrap().local, &dirty));
}

#[tokio::test]
async fn uuidless_delete_during_bound_send_never_confirms_delivery() {
    use super_platinum_core::drafts::sync::{ConsumeKind, Workflow};
    let f = Fixture::new().await;
    let stored = f.owner.load().await.unwrap().stored.unwrap();
    let consumed = f
        .owner
        .consume(stored.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let dispatch = f
        .owner
        .dispatch(consumed.revision, |_| async {
            super::super::dispatch::Outcome::Uncertain
        })
        .await
        .unwrap();
    let observed = f
        .owner
        .observe_event(f.prepared(&deletion("Dr1", "10.000001")))
        .await
        .unwrap();
    let state = &observed.after.stored.as_ref().unwrap().state;
    assert!(
        matches!(state.workflow(), Workflow::Uncertain(operation) if operation == &dispatch.operation)
    );
    assert!(state.consumption().is_some());
    assert!(observed.after.local.empty());
    assert_eq!(
        f.restart().load().await.unwrap().stored.unwrap().state,
        *state
    );
}

#[tokio::test]
async fn uuidless_delete_refuses_unknown_id_unbound_create_and_conflicting_supplied_uuid() {
    let f = Fixture::new().await;
    let original = f.owner.load().await.unwrap().stored.unwrap();
    assert!(matches!(
        f.owner
            .observe_event(f.prepared(&deletion("DrOther", "10.000001")))
            .await,
        Err(Fault::InvalidReceipt)
    ));
    let mut conflict = deletion("Dr1", "10.000001");
    conflict.rest.insert(
        "client_msg_id".into(),
        "00000000-0000-4000-8000-000000000099".into(),
    );
    assert!(matches!(
        f.owner.observe_event(f.prepared(&conflict)).await,
        Err(Fault::InvalidReceipt)
    ));
    assert_eq!(
        f.owner.load().await.unwrap().stored.unwrap().state,
        original.state
    );
    let pending = Fixture::with_remote(false).await;
    let stored = pending.owner.load().await.unwrap().stored.unwrap();
    let prepared_create = pending
        .owner
        .dispatch(stored.revision, |_| async {
            super::super::dispatch::Outcome::Uncertain
        })
        .await
        .unwrap();
    assert!(matches!(
        pending
            .owner
            .observe_event(pending.prepared(&deletion("Dr1", "10.000001")))
            .await,
        Err(Fault::InvalidReceipt)
    ));
    assert_eq!(
        pending.owner.load().await.unwrap().stored.unwrap().state,
        prepared_create.stored.state
    );
}

#[path = "neutral_metadata_tests.rs"]
mod neutral_metadata;
