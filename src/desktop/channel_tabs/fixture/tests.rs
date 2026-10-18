use super::*;
use std::{cell::Cell, rc::Rc};
fn harness() -> (VirtualDom, Signal<ShellState>) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                install(&mut shell);
                shell
            });
            slot.set(Some(state));
            rsx! {}
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap())
}
fn writes(cap: &Capability) -> usize {
    cap.0
        .lock()
        .unwrap()
        .requests
        .iter()
        .filter(|r| {
            [
                "conversations.addTab?",
                "conversations.moveTab?",
                "conversations.updateTab?",
                "conversations.removeTab?",
            ]
            .iter()
            .any(|s| r.url.contains(s))
        })
        .count()
}
async fn action(mut state: Signal<ShellState>, action: Action) {
    let owner = Owner::capture(&state.peek()).unwrap();
    let base = super::super::inventory(&state.peek()).unwrap();
    let intent = Intent::capture(base, action).unwrap();
    {
        let mut shell = state.write();
        let e = shell.channel_tabs.entries.get_mut(&owner.key).unwrap();
        e.serial += 1;
        e.busy = true;
    }
    let serial = state.peek().channel_tabs.entries[&owner.key].serial;
    super::super::run::execute(state, owner, serial, Some(intent)).await;
}
#[tokio::test(flavor = "current_thread")]
async fn real_controller_prepares_four_requests_and_preserves_unknown_sibling() {
    let (dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let unknown = cap.0.lock().unwrap().raw["properties"]["tabz"][1].clone();
    action(
        state,
        Action::Rename {
            id: "Ct00000001".into(),
            label: "Reviewed name".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 1);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
    action(
        state,
        Action::Move {
            id: "Ct00000001".into(),
            after: Some("Ct00000002".into()),
        },
    )
    .await;
    // Confirmed Add schedules the real channel-tab route on the Dioxus root.
    // Poll this controller future with the same runtime ownership as its UI caller.
    let mut add = Box::pin(action(
        state,
        Action::AddCanvas {
            file: "F00000003".into(),
            label: "Launch".into(),
            access: super_platinum_core::channel_tabs::Access::Read,
            after: None,
        },
    ));
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(add.as_mut(), cx))),
    )
    .await
    .expect("bounded disconnected Add controller");
    action(
        state,
        Action::Remove {
            id: "Ct00000003".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 4);
    assert_eq!(cap.0.lock().unwrap().raw["properties"]["tabz"][0], unknown);
    assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
}
#[tokio::test(flavor = "current_thread")]
async fn lost_response_is_retained_and_read_never_replays_the_mutation() {
    let (_dom, mut state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().uncertain = true;
    action(
        state,
        Action::Remove {
            id: "Ct00000001".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 1);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_some()
    );
    let owner = Owner::capture(&state.peek()).unwrap();
    let serial = {
        let mut s = state.write();
        let e = s.channel_tabs.entries.get_mut(&owner.key).unwrap();
        e.serial += 1;
        e.busy = true;
        e.serial
    };
    super::super::run::execute(state, owner, serial, None).await;
    assert_eq!(writes(&cap), 1);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_some()
    );
}
#[test]
fn fixture_has_no_live_fallback_and_exact_epoch_visit_and_property_guards() {
    for change in 0..5 {
        let (_dom, mut state) = harness();
        let owner = Owner::capture(&state.peek()).unwrap();
        assert!(owner.current(&state.peek()));
        {
            let mut s = state.write();
            match change {
                0 => s.core.account_epoch += 1,
                1 => s.channel_generation += 1,
                2 => s.core.active_account = Some("other".into()),
                3 => {
                    s.channel_tabs.fixture = None;
                }
                _ => {
                    s.core
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .channels
                        .get_mut(CHANNEL)
                        .unwrap()
                        .extra
                        .insert("properties".into(), json!({"tabz":[]}));
                }
            }
        }
        assert!(!owner.current(&state.peek()));
        assert!(state.peek().core.transport.is_none());
    }
}
#[tokio::test(flavor = "current_thread")]
async fn fresh_canonical_change_refuses_write_without_overwriting_unknown_data() {
    let (_dom, mut state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().raw["properties"]["tabz"][1]["opaque"]["ordered"] = json!([9, 8, 7]);
    action(
        state,
        Action::Remove {
            id: "Ct00000001".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 0);
    assert_eq!(
        cap.0.lock().unwrap().raw["properties"]["tabz"][1]["opaque"]["ordered"],
        json!([9, 8, 7])
    );
    let before = std::mem::take(&mut state.write().channel_tabs);
    let mut replacement = ShellState::fixture(crate::media::MediaRegistry::default());
    state.write().channel_tabs = before;
    super::super::carry(&mut state.write(), &mut replacement);
    assert_eq!(replacement.channel_tabs.entries.len(), 1);
}

#[test]
fn original_ui_visit_refuses_replaced_account_aba_and_capability_before_admission() {
    for change in 0..5 {
        let (dom, mut state) = harness();
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let visit = super::super::Visit::capture(&state.peek()).unwrap();
        let base = super::super::inventory(&state.peek()).unwrap();
        let (saved_key, saved_epoch, saved_raw) = {
            let r = cap.0.lock().unwrap();
            (r.key.clone(), r.epoch, r.raw.clone())
        };
        {
            let mut shell = state.write();
            match change {
                0 => shell.core.account_epoch += 1,
                1 => shell.channel_generation += 2, // A -> B -> A is a different visit.
                2 => shell.core.active_account = Some("replacement".into()),
                3 => {
                    shell.channel_tabs.fixture = Some(Capability(Arc::new(Mutex::new(Record {
                        key: saved_key.clone(),
                        epoch: saved_epoch,
                        raw: saved_raw.clone(),
                        requests: Vec::new(),
                        uncertain: false,
                    }))))
                }
                _ => shell.self_account.user_id = "other-user".into(),
            }
        }
        dom.in_runtime(|| {
            super::super::start(
                state,
                &visit,
                Some((
                    base,
                    Action::Remove {
                        id: "Ct00000001".into(),
                    },
                )),
            );
            super::super::refresh(state, &visit);
        });
        assert!(cap.0.lock().unwrap().requests.is_empty());
        assert_eq!(state.peek().channel_tabs.next, 0);
        assert!(
            state
                .peek()
                .channel_tabs
                .entries
                .values()
                .all(|e| !e.busy && e.pending.is_none())
        );
    }
}
#[test]
fn busy_or_uncertain_entry_cannot_admit_a_second_write() {
    for pending in [false, true] {
        let (dom, mut state) = harness();
        let visit = super::super::Visit::capture(&state.peek()).unwrap();
        let base = super::super::inventory(&state.peek()).unwrap();
        let action = Action::Remove {
            id: "Ct00000001".into(),
        };
        {
            let mut shell = state.write();
            let entry = shell.channel_tabs.entries.values_mut().next().unwrap();
            if pending {
                entry.pending = Some(Intent::capture(base.clone(), action.clone()).unwrap());
            } else {
                entry.busy = true;
            }
        }
        dom.in_runtime(|| super::super::start(state, &visit, Some((base, action))));
        assert_eq!(state.peek().channel_tabs.next, 0);
        assert!(
            state
                .peek()
                .channel_tabs
                .fixture
                .as_ref()
                .unwrap()
                .0
                .lock()
                .unwrap()
                .requests
                .is_empty()
        );
    }
}

#[test]
fn fixture_capability_preserves_startup_shell_send_boundary() {
    fn assert_send<T: Send>() {}
    assert_send::<Capability>();
    assert_send::<super::super::Root>();
    assert_send::<ShellState>();
}
