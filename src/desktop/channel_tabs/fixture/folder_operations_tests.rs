use super::*;
use std::{cell::Cell, rc::Rc};
fn harness() -> (VirtualDom, Signal<ShellState>) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
                install_folder(&mut s);
                s
            });
            slot.set(Some(state));
            rsx! {}
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap())
}
async fn action(dom: &VirtualDom, mut state: Signal<ShellState>, action: Option<Action>) {
    let owner = Owner::capture(&state.peek()).unwrap();
    let intent = action
        .map(|a| Intent::capture(super::super::inventory(&state.peek()).unwrap(), a).unwrap());
    let serial = {
        let mut s = state.write();
        s.channel_tabs.next += 1;
        let serial = s.channel_tabs.next;
        let e = s.channel_tabs.entries.get_mut(&owner.key).unwrap();
        e.serial = serial;
        e.busy = true;
        serial
    };
    let mut work = Box::pin(super::super::run::execute(state, owner, serial, intent));
    std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(work.as_mut(), cx)))
        .await;
}
fn methods(cap: &Capability) -> Vec<String> {
    cap.0
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| {
            r.url
                .split('?')
                .next()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect()
}
#[tokio::test(flavor = "current_thread")]
async fn actual_folder_creation_records_bookmark_wire_and_positive_readback_not_second_write() {
    let (dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let before = cap.0.lock().unwrap().raw["properties"]["tabz"].clone();
    action(
        &dom,
        state,
        Some(Action::AddFolder {
            label: "Launch".into(),
        }),
    )
    .await;
    assert_eq!(
        methods(&cap),
        ["conversations.info", "bookmarks.add", "conversations.info"]
    );
    let r = cap.0.lock().unwrap();
    let RequestBody::Form(fields) = &r.requests[1].body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "fixture-only".into()),
            ("channel".into(), CHANNEL.into()),
            ("title".into(), "Launch".into()),
            ("type".into(), "local_folder".into())
        ]
    );
    assert_eq!(
        &r.raw["properties"]["tabz"].as_array().unwrap()[..3],
        before.as_array().unwrap()
    );
    drop(r);
    let shell = state.peek();
    assert!(super::super::entry(&shell).unwrap().pending.is_none());
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn folder_rename_move_share_existing_controller_and_exact_sibling_readback() {
    let (dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let unknown = cap.0.lock().unwrap().raw["properties"]["tabz"][2].clone();
    action(
        &dom,
        state,
        Some(Action::Rename {
            id: "Ct00000001".into(),
            label: "Team resources".into(),
        }),
    )
    .await;
    action(
        &dom,
        state,
        Some(Action::Move {
            id: "Ct00000001".into(),
            after: Some("Ct00000002".into()),
        }),
    )
    .await;
    assert_eq!(
        methods(&cap),
        [
            "conversations.info",
            "conversations.updateTab",
            "conversations.info",
            "conversations.info",
            "conversations.moveTab",
            "conversations.info"
        ]
    );
    let r = cap.0.lock().unwrap();
    assert_eq!(r.raw["properties"]["tabz"][2], unknown);
    assert_eq!(r.raw["properties"]["tabz"][1]["label"], "Team resources");
    assert_eq!(
        r.raw["properties"]["tabz"][1]["data"]["folder_bookmark_id"],
        "Bk1"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn lost_folder_response_retains_name_and_review_never_repeats_or_matches_same_label() {
    let (dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().uncertain = true;
    action(
        &dom,
        state,
        Some(Action::AddFolder {
            label: "Launch".into(),
        }),
    )
    .await;
    cap.0.lock().unwrap().uncertain = false;
    cap.0.lock().unwrap().raw["properties"]["tabz"].as_array_mut().unwrap().push(json!({"id":"Ct90000002","type":"folder","label":"Launch","data":{"folder_bookmark_id":"Bk9"}}));
    action(&dom, state, None).await;
    action(&dom, state, None).await;
    assert_eq!(
        methods(&cap)
            .iter()
            .filter(|m| m.as_str() == "bookmarks.add")
            .count(),
        1
    );
    let shell = state.peek();
    let pending = super::super::entry(&shell)
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(
        pending.action(),
        &Action::AddFolder {
            label: "Launch".into()
        }
    );
    assert!(pending.created_folder().is_none());
}
#[test]
fn automatic_folder_navigation_requires_ack_membership_and_original_immediate_visit() {
    let (_dom, state) = harness();
    let base = super::super::inventory(&state.peek()).unwrap();
    let mut intent = Intent::capture(
        base.clone(),
        Action::AddFolder {
            label: "Launch".into(),
        },
    )
    .unwrap();
    let mut raw = state
        .peek()
        .channel_tabs
        .fixture
        .as_ref()
        .unwrap()
        .0
        .lock()
        .unwrap()
        .raw
        .clone();
    raw["properties"]["tabz"].as_array_mut().unwrap().push(json!({"id":"Ct90000001","type":"folder","label":"Launch","data":{"folder_bookmark_id":"Bk9"}}));
    let current = Inventory::from_channel(&raw).unwrap();
    assert!(selection::confirmed_added(&intent, &current, true, true).is_none());
    intent
        .acknowledge(
            &json!({"ok":true,"bookmark":{"id":"Bk9","type":"local_folder","title":"Launch"}}),
        )
        .unwrap();
    assert_eq!(
        selection::confirmed_added(&intent, &current, true, true).as_deref(),
        Some("Ct90000001")
    );
    assert!(selection::confirmed_added(&intent, &base, true, true).is_none());
    assert!(selection::confirmed_added(&intent, &current, false, true).is_none());
    assert!(selection::confirmed_added(&intent, &current, true, false).is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn late_creation_ack_retains_exact_bookmark_identity_without_new_account_navigation() {
    let (dom, mut state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
    let job = async {
        tokio::join!(
            action(
                &dom,
                state,
                Some(Action::AddFolder {
                    label: "Launch".into()
                })
            ),
            async {
                entered.notified().await;
                state.write().core.account_epoch += 1;
                release.notify_one();
            }
        );
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), job)
        .await
        .unwrap();
    let shell = state.peek();
    let pending = super::super::entry(&shell)
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(pending.created_folder(), Some("Bk90000001"));
    assert_eq!(methods(&cap), ["conversations.info", "bookmarks.add"]);
    assert_eq!(
        selection::current(&shell)
            .and_then(|r| match r {
                selection::Route::Folder(f) => Some(f.tab().to_owned()),
                _ => None,
            })
            .as_deref(),
        Some("Ct00000001")
    );
}
#[tokio::test(flavor = "current_thread")]
async fn stale_original_callback_and_fresh_permission_refusal_do_not_create() {
    let (dom, mut state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let visit = Visit::capture(&state.peek()).unwrap();
    let base = super::super::inventory(&state.peek()).unwrap();
    state.write().channel_generation += 1;
    dom.in_runtime(|| {
        super::super::start(
            state,
            &visit,
            Some((
                base,
                Action::AddFolder {
                    label: "Stale".into(),
                },
            )),
        )
    });
    assert!(methods(&cap).is_empty());
    cap.0.lock().unwrap().raw["is_member"] = false.into();
    action(
        &dom,
        state,
        Some(Action::AddFolder {
            label: "No permission".into(),
        }),
    )
    .await;
    assert_eq!(methods(&cap), ["conversations.info"]);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn folder_fixture_requires_explicit_unrestricted_posting_facts_before_any_write() {
    for field in ["is_read_only", "is_thread_only"] {
        for value in [None, Some(Value::Null), Some(Value::Bool(true))] {
            let (dom, state) = harness();
            let cap = state.peek().channel_tabs.fixture.clone().unwrap();
            let owner = Owner::capture(&state.peek()).unwrap();
            {
                let mut r = cap.0.lock().unwrap();
                assert_eq!(r.raw["is_read_only"], false);
                assert_eq!(r.raw["is_thread_only"], false);
                assert_eq!(owner.permission(&r.raw), Permission::Allowed);
                match value.as_ref() {
                    Some(value) => r.raw[field] = value.clone(),
                    None => {
                        r.raw.as_object_mut().unwrap().remove(field);
                    }
                }
                assert_ne!(owner.permission(&r.raw), Permission::Allowed);
            }
            action(
                &dom,
                state,
                Some(Action::AddFolder {
                    label: "Must remain unsent".into(),
                }),
            )
            .await;
            assert_eq!(methods(&cap), ["conversations.info"]);
            assert!(
                super::super::entry(&state.peek())
                    .unwrap()
                    .pending
                    .is_none()
            );
            assert!(state.peek().core.session.is_none());
            assert!(state.peek().core.transport.is_none());
        }
    }
}
