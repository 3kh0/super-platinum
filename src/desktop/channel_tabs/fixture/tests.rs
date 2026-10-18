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
                        manager_response: json!({"ok":true,"role_assignments":[]}),
                        revoke_on_share: false,
                        manager_rate_limited: false,
                        enterprise: None,
                        permission_response: json!({"ok":true,"allowed":[]}),
                        knowledge_loss: 0,
                        knowledge_file_only: false,
                        folder_bookmarks: json!([]),
                        #[cfg(test)]
                        folder_file_error: None,
                        #[cfg(test)]
                        folder_file_pause: None,
                        manager_pause: None,
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

#[tokio::test(flavor = "current_thread")]
async fn current_channel_manager_uses_real_request_before_mutation_and_preserves_unknown_rows() {
    let (_dom, mut state) = harness();
    install_manager(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let unknown = cap.0.lock().unwrap().raw["properties"]["tabz"][1].clone();
    action(
        state,
        Action::Rename {
            id: "Ct00000001".into(),
            label: "Manager rename".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 1);
    let record = cap.0.lock().unwrap();
    assert_eq!(record.raw["properties"]["tabz"][1], unknown);
    let methods: Vec<_> = record
        .requests
        .iter()
        .map(|r| r.url.split('?').next().unwrap().rsplit('/').next().unwrap())
        .collect();
    assert_eq!(
        methods,
        vec![
            "conversations.info",
            "admin.roles.entity.listAssignments",
            "conversations.updateTab",
            "conversations.info"
        ]
    );
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
}
#[tokio::test(flavor = "current_thread")]
async fn revoked_manager_after_canvas_sharing_checks_cannot_use_old_positive_grant() {
    let (_dom, mut state) = harness();
    install_manager(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().revoke_on_share = true;
    action(
        state,
        Action::AddCanvas {
            file: "F00000003".into(),
            label: "New".into(),
            access: super_platinum_core::channel_tabs::Access::Read,
            after: None,
        },
    )
    .await;
    assert_eq!(writes(&cap), 0);
    let r = cap.0.lock().unwrap();
    assert_eq!(
        r.requests
            .iter()
            .filter(|r| r.url.contains("admin.roles.entity.listAssignments?"))
            .count(),
        2
    );
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
}
#[tokio::test(flavor = "current_thread")]
async fn manager_lookup_rate_limit_retains_cooldown_without_mutation_or_replay() {
    let (dom, mut state) = harness();
    install_manager(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().manager_rate_limited = true;
    let mut operation = Box::pin(action(
        state,
        Action::Remove {
            id: "Ct00000001".into(),
        },
    ));
    std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(operation.as_mut(), cx)))
        .await;
    assert_eq!(writes(&cap), 0);
    let shell = state.peek();
    let entry = super::super::entry(&shell).unwrap();
    assert!(entry.cooling);
    assert!(entry.retry_at.is_some());
    assert!(!entry.busy);
    assert!(entry.pending.is_none());
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn manager_result_cannot_grant_after_account_or_ui_visit_retirement() {
    for account in [false, true] {
        let (_dom, mut state) = harness();
        install_manager(&mut state.write());
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
        let owner = Owner::capture(&state.peek()).unwrap();
        let raw = cap.0.lock().unwrap().raw.clone();
        let future = async {
            tokio::join!(owner.read_permission(state, &raw), async {
                entered.notified().await;
                if account {
                    state.write().core.account_epoch += 1;
                } else {
                    state.write().channel_generation += 1;
                }
                release.notify_one();
            })
            .0
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), future)
            .await
            .expect("bounded synthetic manager response");
        assert!(result.is_err());
        assert_eq!(writes(&cap), 0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn enterprise_default_tab_policy_uses_actual_edge_request_and_keeps_unknown_inventory() {
    let (_dom, mut state) = harness();
    install_enterprise(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let unknown = cap.0.lock().unwrap().raw["properties"]["tabz"][1].clone();
    action(
        state,
        Action::Rename {
            id: "Ct00000001".into(),
            label: "Enterprise rename".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 1);
    let record = cap.0.lock().unwrap();
    assert_eq!(record.raw["properties"]["tabz"][1], unknown);
    let calls: Vec<_> = record
        .requests
        .iter()
        .map(|r| r.url.split('?').next().unwrap())
        .collect();
    assert_eq!(
        calls[1],
        "https://fixture.invalid/api/api.getFlannelHttpUrl"
    );
    assert_eq!(
        calls[2],
        format!(
            "https://edgeapi.slack.com/cache/{}/permissions/info",
            record.key.team
        )
    );
    assert_eq!(record.requests.len(), 5);
    assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn enterprise_permission_revocation_during_file_checks_refuses_add() {
    let (_dom, mut state) = harness();
    install_enterprise(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().revoke_on_share = true;
    action(
        state,
        Action::AddCanvas {
            file: "F00000003".into(),
            label: "Launch".into(),
            access: super_platinum_core::channel_tabs::Access::Read,
            after: None,
        },
    )
    .await;
    assert_eq!(writes(&cap), 0);
    assert_eq!(
        cap.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|r| r.url.contains("/permissions/info?"))
            .count(),
        2
    );
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .pending
            .is_none()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn retired_discovery_does_not_issue_permission_or_mutation_request() {
    for account in [false, true] {
        let (_dom, mut state) = harness();
        install_enterprise(&mut state.write());
        let cap = state.peek().channel_tabs.fixture.clone().unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
        let owner = Owner::capture(&state.peek()).unwrap();
        let raw = cap.0.lock().unwrap().raw.clone();
        let future = async {
            tokio::join!(owner.read_permission(state, &raw), async {
                entered.notified().await;
                if account {
                    state.write().core.account_epoch += 1;
                } else {
                    state.write().channel_generation += 1;
                }
                release.notify_one();
            })
            .0
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), future)
            .await
            .expect("bounded discovery");
        assert!(result.is_err());
        let record = cap.0.lock().unwrap();
        assert_eq!(record.requests.len(), 1);
        assert!(
            record.requests[0]
                .url
                .contains("/api/api.getFlannelHttpUrl?")
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn discovery_rate_limit_retains_existing_fence_without_permission_or_write() {
    let (dom, mut state) = harness();
    install_enterprise(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().manager_rate_limited = true;
    let mut operation = Box::pin(action(
        state,
        Action::Remove {
            id: "Ct00000001".into(),
        },
    ));
    std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(operation.as_mut(), cx)))
        .await;
    assert_eq!(writes(&cap), 0);
    let shell = state.peek();
    let entry = super::super::entry(&shell).unwrap();
    assert!(entry.cooling && entry.retry_at.is_some() && !entry.busy && entry.pending.is_none());
    let record = cap.0.lock().unwrap();
    assert_eq!(record.requests.len(), 2);
    assert!(
        record
            .requests
            .iter()
            .all(|r| !r.url.contains("/permissions/info"))
    );
}

#[test]
fn unified_enterprise_owner_never_uses_nonenterprise_default_grant() {
    let (_dom, mut state) = harness();
    install_enterprise(&mut state.write());
    let mut owner = Owner::capture(&state.peek()).unwrap();
    owner.session.team_id = "E00000001".into();
    owner.session.enterprise_id = None;
    let raw = owner.cached.clone();
    assert_eq!(owner.permission(&raw), Permission::Unavailable);
}

async fn knowledge_action(mut state: Signal<ShellState>, finish: bool) {
    let owner = Owner::capture(&state.peek()).unwrap();
    let serial = {
        let mut shell = state.write();
        let e = shell.channel_tabs.entries.get_mut(&owner.key).unwrap();
        e.serial += 1;
        e.busy = true;
        e.serial
    };
    super::super::knowledge::execute(state, owner, serial, finish).await;
}
fn arm_knowledge(mut state: Signal<ShellState>) {
    let key = key(&state.peek()).unwrap();
    let base = super::super::inventory(&state.peek()).unwrap();
    state
        .write()
        .channel_tabs
        .entries
        .get_mut(&key)
        .unwrap()
        .knowledge = Some(
        super_platinum_core::channel_tabs::KnowledgeRemoval::capture(base, "Ct00000001").unwrap(),
    );
}
fn knowledge_calls(cap: &Capability) -> Vec<String> {
    cap.0
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| url_path(&r.url))
        .collect()
}
fn url_path(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap()
        .split("/api/")
        .nth(1)
        .unwrap_or(url)
        .to_owned()
}
#[tokio::test(flavor = "current_thread")]
async fn knowledge_actual_controller_disables_then_removes_and_keeps_unknown_raw() {
    let (_dom, mut state) = harness();
    install_knowledge(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let before = cap.0.lock().unwrap().raw.clone();
    arm_knowledge(state);
    knowledge_action(state, false).await;
    assert_eq!(
        knowledge_calls(&cap),
        vec![
            "conversations.info",
            "ai.channelKnowledge.disable",
            "conversations.info",
            "conversations.removeTab",
            "conversations.info"
        ]
    );
    let r = cap.0.lock().unwrap();
    assert_eq!(
        r.raw["properties"]["tabz"],
        json!([before["properties"]["tabz"][1]])
    );
    assert_eq!(
        r.raw["properties"]["channel_knowledge_canvas"]["opaque"],
        before["properties"]["channel_knowledge_canvas"]["opaque"]
    );
    assert_eq!(
        r.raw["properties"]["channel_knowledge_canvas"]["is_enabled"],
        false
    );
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .is_none()
    );
    assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn lost_disable_response_requires_review_then_explicit_finish_never_disable_replay() {
    let (_dom, mut state) = harness();
    install_knowledge(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().knowledge_loss = 1;
    arm_knowledge(state);
    knowledge_action(state, false).await;
    assert_eq!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .as_ref()
            .unwrap()
            .stage(),
        super_platinum_core::channel_tabs::KnowledgeStage::DisableUnconfirmed
    );
    knowledge_action(state, false).await;
    assert_eq!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .as_ref()
            .unwrap()
            .stage(),
        super_platinum_core::channel_tabs::KnowledgeStage::Disabled
    );
    assert!(
        !knowledge_calls(&cap)
            .iter()
            .any(|p| p == "conversations.removeTab")
    );
    knowledge_action(state, true).await;
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .is_none()
    );
    assert_eq!(
        knowledge_calls(&cap)
            .iter()
            .filter(|p| p.as_str() == "ai.channelKnowledge.disable")
            .count(),
        1
    );
    assert_eq!(writes(&cap), 1);
}
#[tokio::test(flavor = "current_thread")]
async fn lost_remove_response_settles_by_read_only_review() {
    let (_dom, mut state) = harness();
    install_knowledge(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().knowledge_loss = 2;
    arm_knowledge(state);
    knowledge_action(state, false).await;
    assert_eq!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .as_ref()
            .unwrap()
            .stage(),
        super_platinum_core::channel_tabs::KnowledgeStage::RemoveUnconfirmed
    );
    knowledge_action(state, false).await;
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .is_none()
    );
    assert_eq!(writes(&cap), 1);
}
#[tokio::test(flavor = "current_thread")]
async fn knowledge_response_after_navigation_can_settle_disable_but_cannot_remove() {
    let (_dom, mut state) = harness();
    install_knowledge(&mut state.write());
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    cap.0.lock().unwrap().manager_pause = Some((entered.clone(), release.clone()));
    arm_knowledge(state);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(knowledge_action(state, false), async {
            entered.notified().await;
            state.write().channel_generation += 1;
            release.notify_one();
        });
    })
    .await
    .expect("bounded disable response");
    assert_eq!(
        super::super::entry(&state.peek())
            .unwrap()
            .knowledge
            .as_ref()
            .unwrap()
            .stage(),
        super_platinum_core::channel_tabs::KnowledgeStage::Disabled
    );
    assert_eq!(writes(&cap), 0);
}
#[tokio::test(flavor = "current_thread")]
async fn file_only_knowledge_metadata_cannot_skip_disable_service() {
    let (_dom, state) = harness();
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    cap.0.lock().unwrap().knowledge_file_only = true;
    action(
        state,
        Action::Remove {
            id: "Ct00000001".into(),
        },
    )
    .await;
    assert_eq!(writes(&cap), 0);
    assert!(
        super::super::entry(&state.peek())
            .unwrap()
            .notice
            .contains("Knowledge Canvas")
    );
}
#[tokio::test(flavor = "current_thread")]
async fn selected_knowledge_editor_and_removal_reservation_exclude_each_other() {
    let (_dom, mut state) = harness();
    install_knowledge(&mut state.write());
    let base = super::super::inventory(&state.peek()).unwrap();
    let route = super::super::selection::CanvasRoute::capture(&base, "Ct00000001").unwrap();
    assert!(super::super::selection::record(
        &mut state.write(),
        super::super::selection::Route::Canvas(route),
        None
    ));
    state.write().channel_tab = crate::state::ChannelTab::Canvas;
    let generation = state.peek().shortcut_generation;
    assert!(crate::canvas::surface::hold_local(
        &mut state.write(),
        generation
    ));
    arm_knowledge(state);
    knowledge_action(state, false).await;
    let cap = state.peek().channel_tabs.fixture.clone().unwrap();
    assert_eq!(writes(&cap), 0);
    assert!(
        !knowledge_calls(&cap)
            .iter()
            .any(|p| p == "ai.channelKnowledge.disable")
    );
    crate::canvas::surface::release_local(&mut state.write(), generation);
    arm_knowledge(state);
    assert!(super::super::selection::removal_pending(&state.peek()));
    assert!(!crate::canvas::surface::hold_local(
        &mut state.write(),
        generation
    ));
}
