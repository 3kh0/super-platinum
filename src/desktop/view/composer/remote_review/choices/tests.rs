use super::*;
use serde_json::json;
use super_platinum_core::{
    ComposerState,
    drafts::{
        DraftKey,
        store::sync::{BlockedLocal, LocalAlternate, Stored},
        sync::{DraftSync, RemoteDraft, Scope},
    },
};

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn review(mut change: impl FnMut(&mut LocalReview)) -> RemoteReview {
    let scope = Scope {
        account_id: "11111111-1111-4111-8111-111111111111".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1.000001".into()),
        },
    };
    let value = local("Local\n  exact spacing");
    let mut snapshot = codec::encode_composer(&scope.key, &value.composer).unwrap();
    snapshot.destinations[0]["broadcast"] = json!(false);
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: "22222222-2222-4222-8222-222222222222".into(),
        last_updated_ts: "10.000001".into(),
        snapshot,
    };
    let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    let mut inventory = LocalReview {
        scope: scope.clone(),
        local: value,
        stored: Some(Stored { revision: 1, state }),
        blocked: None,
        alternates: vec![],
    };
    change(&mut inventory);
    let mut remote =
        codec::encode_composer(&scope.key, &local("Slack\n version").composer).unwrap();
    remote.destinations[0]["broadcast"] = json!(false);
    RemoteReview::prepare(inventory,json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"12.000001","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":remote.blocks,"file_ids":remote.file_ids,"destinations":remote.destinations}}),"U1").unwrap()
}

#[test]
fn identical_visible_text_does_not_coalesce_blocked_and_distinct_writer_choices() {
    let review = review(|inventory| {
        inventory.blocked = Some(BlockedLocal {
            token: "blocked-token".into(),
            draft: local("same"),
        });
        inventory.alternates = vec![
            LocalAlternate {
                writer_id: "writer-a".into(),
                revision: 4,
                draft: local("same"),
            },
            LocalAlternate {
                writer_id: "writer-b".into(),
                revision: 7,
                draft: local("same"),
            },
        ];
    });
    let rows = build(&review);
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows[1].choice,
        Some(RemoteChoice::Local(LocalChoice::Blocked {
            token: "blocked-token".into()
        }))
    );
    assert_eq!(
        rows[2].choice,
        Some(RemoteChoice::Local(LocalChoice::Alternate {
            writer_id: "writer-a".into(),
            revision: 4
        }))
    );
    assert_eq!(
        rows[3].choice,
        Some(RemoteChoice::Local(LocalChoice::Alternate {
            writer_id: "writer-b".into(),
            revision: 7
        }))
    );
    assert_eq!(rows[4].choice, Some(RemoteChoice::Remote));
    assert!(!allowed(
        &review,
        &RemoteChoice::Local(LocalChoice::Alternate {
            writer_id: "writer-a".into(),
            revision: 5
        })
    ));
    assert!(!allowed(
        &review,
        &RemoteChoice::Local(LocalChoice::Blocked {
            token: "old-token".into()
        })
    ));
}

#[test]
fn previews_preserve_raw_destination_policy_and_full_local_selection() {
    let review = review(|inventory| inventory.local.composer.set_selection(1, 4));
    let rows = build(&review);
    let Content::Local(local) = &rows[0].content else {
        panic!("local")
    };
    assert_eq!(local.composer, review.inventory().local.composer);
    assert_eq!(
        rows[0].retained.as_ref().unwrap().destinations[0]["broadcast"],
        false
    );
    assert_eq!(
        rows[0].retained.as_ref(),
        review.inventory().stored.as_ref().unwrap().state.desired()
    );
    let Content::Snapshot(remote) = &rows.last().unwrap().content else {
        panic!("remote")
    };
    assert_eq!(remote, &review.remote().snapshot);
    assert_eq!(remote.destinations[0]["broadcast"], false);
    assert!(allowed(&review, &RemoteChoice::Local(LocalChoice::Current)));
}

#[test]
fn attached_version_remains_visible_but_cannot_be_selected() {
    let review = review(|inventory| {
        let mut draft = local("Text with file");
        draft.attachments.push(
            serde_json::from_value(
                json!({"id":1,"path":"fixture-only.txt","name":"Keep this file.txt","bytes":8}),
            )
            .unwrap(),
        );
        inventory.alternates.push(LocalAlternate {
            writer_id: "files".into(),
            revision: 2,
            draft,
        });
    });
    let rows = build(&review);
    assert_eq!(rows.len(), 3);
    assert!(rows[1].unavailable.is_some());
    let Content::Local(local) = &rows[1].content else {
        panic!("local")
    };
    assert_eq!(local.attachments[0].name, "Keep this file.txt");
    assert!(!allowed(&review, rows[1].choice.as_ref().unwrap()));
    assert!(allowed(&review, &RemoteChoice::Remote));
}

#[test]
fn older_retained_slack_content_is_shown_without_fabricating_a_choice() {
    let review = review(|inventory| {
        let previous = RemoteDraft {
            scope: inventory.scope.clone(),
            id: "D1".into(),
            client_msg_id: "22222222-2222-4222-8222-222222222222".into(),
            last_updated_ts: "11.000001".into(),
            snapshot: codec::encode_composer(
                &inventory.scope.key,
                &local("Previously preserved").composer,
            )
            .unwrap(),
        };
        inventory
            .stored
            .as_mut()
            .unwrap()
            .state
            .observe_conflict(previous);
    });
    let rows = build(&review);
    assert_eq!(rows.len(), 3);
    assert!(rows[1].choice.is_none());
    assert!(rows[1].unavailable.is_some());
    let Content::Snapshot(raw) = &rows[1].content else {
        panic!("raw")
    };
    assert_eq!(
        codec::decode(&review.inventory().scope.key, raw)
            .unwrap()
            .composer()
            .text,
        "Previously preserved"
    );
}
