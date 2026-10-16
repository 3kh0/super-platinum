use super::*;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Mutex,
    time::Duration,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    file_sharing::{
        Intent, Scope,
        store::{Begin, Journal},
    },
    slack::Transport,
};
const ACCOUNT: &str = "4b02f26e-393b-4fd1-a7da-5d11f76296ae";
fn sample() -> (ShellState, crate::file_actions::Target) {
    let (mut shell, _, _) = crate::file_actions::attachment::tests::sample();
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.active_account = Some(ACCOUNT.into());
    shell.self_account.user_id = user.clone();
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team.clone(),
                user_id: user.clone(),
                enterprise_id: None,
                name: "Synthetic".into(),
                url: "https://fixture.slack.com".into(),
                token: "synthetic-token".into(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    let file = &mut crate::file_actions::attachment::tests::message(&mut shell).files[0];
    file.extra.insert("mode".into(), json!("hosted"));
    file.is_external = Some(false);
    shell.core.workspaces.get_mut(&team).unwrap().channels.insert("C00000001".into(),serde_json::from_value(json!({"id":"C00000001","name":"ship","is_channel":true,"is_member":true,"is_ext_shared":false})).unwrap());
    let channel = shell.core.active_channel.as_ref().unwrap();
    let workspace = &shell.core.workspaces[&team];
    let raw = &workspace.messages[channel].messages[0];
    let row = crate::message_vm::message_vm(workspace, raw, &shell.media);
    let source = crate::message_buttons::source::Source::capture(
        &shell,
        &row,
        channel,
        crate::message_buttons::source::Bag::Timeline,
    )
    .unwrap();
    let target = crate::file_actions::attachment::target(
        &shell,
        &source,
        &super_platinum_core::file_actions::identity(&raw.files[0]).unwrap(),
        &super_platinum_core::state::file_title(&raw.files[0]),
    )
    .unwrap();
    (shell, target)
}
fn note() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","future":7,"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"\u{65e5}\u{672c} \u{1f98a}","style":{"bold":true,"future":3}}]}]}),
    ]
}
fn key() -> admission::Key {
    admission::Key {
        account: ACCOUNT.into(),
        team: "T00000001".into(),
        user: "U00000001".into(),
    }
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sp-share-parent-{}-{}-{}",
            std::process::id(),
            recovery::now(),
            next_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn storage(&self) -> recovery::Storage {
        recovery::Storage::at(key(), self.0.join("receipt.sqlite"))
    }
    fn receipt(&self) -> Receipt {
        let scope = Scope {
            team: key().team,
            user: key().user,
            file: "FFILE".into(),
            destination: "C00000001".into(),
        };
        let intent = Intent::new(
            scope.file.clone(),
            scope.destination.clone(),
            Some(note()),
            None,
        )
        .unwrap();
        match Journal::open(self.0.join("receipt.sqlite"), ACCOUNT)
            .unwrap()
            .begin(scope, intent, "File".into(), "ship".into(), 10)
            .unwrap()
        {
            Begin::Started(r) => r,
            _ => panic!("new"),
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Tests use the production asynchronous claim/publication boundary and an
/// owned private database. No dialog is marked editable before actual SQLite.
async fn open(
    shell: &mut ShellState,
    target: crate::file_actions::Target,
    focus: String,
    temp: &Temp,
) -> Result<String, Refusal> {
    if visible(shell).is_some_and(uncaptured) {
        return Err(Refusal::Busy);
    }
    let source = admission::Source::capture(shell, &target).ok_or(Refusal::Source)?;
    let storage = recovery::Storage::at(source.context.key.clone(), temp.0.join("draft.sqlite"));
    let load = durable::open::prepare_captured(shell, source, focus, Ok(storage))?;
    assert!(
        visible(shell)
            .is_some_and(|d| d.phase == Phase::Loading && !d.editor_ready && d.durable.is_none())
    );
    let outcome = load.outcome().await;
    load.publish(shell, outcome);
    Ok(load.id)
}

#[test]
fn full_metadata_and_credential_changes_retire_source_even_when_attachment_identity_survives() {
    for change in 0..8 {
        let (mut shell, target) = sample();
        let source = admission::Source::capture(&shell, &target).unwrap();
        assert!(source.current(&shell));
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("other-account".into()),
            2 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            3 => {
                crate::file_actions::attachment::tests::message(&mut shell).files[0]
                    .extra
                    .insert("mode".into(), json!("external"));
            }
            4 => {
                crate::file_actions::attachment::tests::message(&mut shell).files[0]
                    .extra
                    .insert("future".into(), json!(9));
            }
            5 => {
                let f =
                    crate::file_actions::attachment::tests::message(&mut shell).files[0].clone();
                crate::file_actions::attachment::tests::message(&mut shell)
                    .files
                    .push(f);
            }
            6 => shell.channel_generation += 1,
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
        }
        assert!(!source.current(&shell), "mutation {change}");
    }
}
#[test]
fn fresh_file_info_requires_positive_exact_hosted_file() {
    let (shell, target) = sample();
    let source = admission::Source::capture(&shell, &target).unwrap();
    assert!(source.fresh(json!({"ok":true,"file":source.file})).is_ok());
    for bad in [
        json!({"ok":false,"file":source.file}),
        json!({"ok":true,"file":{"id":"OTHER","mode":"hosted","is_external":false,"filetype":"text"}}),
        json!({"ok":true,"error":"denied","file":source.file}),
    ] {
        assert!(source.fresh(bad).is_err());
    }
}
#[test]
fn destinations_never_grant_missing_raw_policy_and_only_offer_joined_internal_channels() {
    let (mut shell, _) = sample();
    let team = shell.core.active_team.clone().unwrap();
    for (id, extra) in [
        ("D00000001", json!({"is_im":true})),
        ("C00000002", json!({"is_member":false})),
        ("C00000003", json!({"is_ext_shared":true})),
        ("C00000004", json!({"is_archived":true})),
    ] {
        let mut raw = json!({"id":id,"name":id,"is_channel":true,"is_member":true});
        raw.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), serde_json::from_value(raw).unwrap());
    }
    let choices = destinations(&shell, "");
    assert!(choices.iter().any(|d| d.id == "C00000001"));
    assert!(
        !choices.iter().any(|d| d.id == "D00000001"
            || ["C00000002", "C00000003", "C00000004"].contains(&d.id.as_str()))
    );
    assert!(shell.file_share.policies.is_empty());
}
#[test]
fn picker_keeps_deterministic_top_thirty_and_exact_lookup_outside_visible_page() {
    let (mut shell, _) = sample();
    let team = shell.core.active_team.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .clear();
    for n in (0..100).rev() {
        let id = format!("C{n:08}");
        shell.core.workspaces.get_mut(&team).unwrap().channels.insert(id.clone(),serde_json::from_value(json!({"id":id,"name":format!("channel-{n:03}"),"is_channel":true,"is_member":true,"is_ext_shared":false})).unwrap());
    }
    let choices = destinations(&shell, "");
    assert_eq!(choices.len(), 30);
    assert_eq!(choices.first().unwrap().id, "C00000000");
    assert_eq!(choices.last().unwrap().id, "C00000029");
    assert!(admission::selected(&shell, "C00000099").is_some());
}

#[tokio::test]
async fn stale_dialog_callbacks_and_owner_rotation_do_not_mutate_newer_editor() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let first = open(&mut shell, target.clone(), "a".into(), &temp)
        .await
        .unwrap();
    assert_eq!(
        open(&mut shell, target.clone(), "blocked".into(), &temp).await,
        Err(Refusal::Busy)
    );
    close(&mut shell, &first);
    let second = open(&mut shell, target, "b".into(), &temp).await.unwrap();
    assert_eq!(
        choose(&mut shell, &first, "C00000001"),
        Err(Refusal::Changed)
    );
    close(&mut shell, &first);
    assert_eq!(visible(&shell).unwrap().id, second);
    shell.core.account_epoch += 1;
    assert!(visible(&shell).is_none());
    assert!(recovery_visible(&shell).is_none());
}
#[test]
fn actual_policy_registry_reclaims_idle_owners_and_retires_canceled_tickets() {
    let (mut shell, _) = sample();
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .entry(user.clone())
        .or_insert_with(|| serde_json::from_value(json!({"id":user,"is_bot":false})).unwrap());
    let mut state = std::mem::take(&mut shell.file_share);
    state.sync(&shell);
    for n in 1..=64 {
        let id = format!("C{n:08}");
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(
            id.clone(),
            serde_json::from_value(
                json!({"id":id,"name":id,"is_channel":true,"is_member":true,"is_ext_shared":false}),
            )
            .unwrap(),
        );
        let read = state.begin_policy(&shell, &id).unwrap();
        drop(delivery::Ticket {
            custody: state.custody.clone(),
            channel: id,
            read: Some(read),
        });
    }
    assert_eq!(state.policies.len(), 32);
    let next = "C00000065";
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(
            next.into(),
            serde_json::from_value(json!({"id":next,"is_channel":true,"is_member":true})).unwrap(),
        );
    let next_read = state.begin_policy(&shell, next).unwrap();
    state.policies.get_mut(next).unwrap().failed(&next_read);
    drop(next_read);
    assert_eq!(state.policies.len(), 32);
    let again = state.begin_policy(&shell, "C00000001").unwrap();
    assert!(state.begin_policy(&shell, "C00000001").is_err());
    drop(delivery::Ticket {
        custody: state.custody.clone(),
        channel: "C00000001".into(),
        read: Some(again),
    });
    assert!(state.begin_policy(&shell, "C00000001").is_ok());
}
#[tokio::test]
async fn actual_parent_permission_and_connection_hooks_invalidate_raw_grants() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .entry(user.clone())
        .or_insert_with(|| serde_json::from_value(json!({"id":user,"is_bot":false})).unwrap());
    open(&mut shell, target, "source".into(), &temp)
        .await
        .unwrap();
    let mut state = std::mem::take(&mut shell.file_share);
    let read = state.begin_policy(&shell, "C00000001").unwrap();
    let snapshot=state.policies.get_mut("C00000001").unwrap().finish(&shell,read,json!({"ok":true,"channel":{"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false}})).unwrap();
    assert_eq!(snapshot.eligibility(), Eligibility::Allowed);
    shell.file_share = state;
    let generation = shell.core.workspaces[&team].rt_generation;
    let frame = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"channel_updated","channel":"C00000001","updates":{"permissions":true}})
            .to_string(),
    )
    .unwrap();
    event(&mut shell, &team, generation, &frame);
    assert!(!shell.file_share.policies["C00000001"].current(&shell, &snapshot));
    let mut state = std::mem::take(&mut shell.file_share);
    let read = state.begin_policy(&shell, "C00000001").unwrap();
    let snapshot=state.policies.get_mut("C00000001").unwrap().finish(&shell,read,json!({"ok":true,"channel":{"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false}})).unwrap();
    assert_ne!(snapshot.eligibility(), Eligibility::Allowed);
    shell.file_share = state;
    connection(&mut shell, &team, generation);
    assert!(!shell.file_share.policies["C00000001"].current(&shell, &snapshot));
}
#[test]
fn lease_drop_releases_only_before_admission_and_retains_actual_admitted_note() {
    let temp = Temp::new();
    let receipt = temp.receipt();
    let custody = Arc::new(Mutex::new(recovery::Custody::default()));
    let lease = delivery::Lease::reserve(custody.clone(), &key(), "first".into()).unwrap();
    assert!(delivery::Lease::reserve(custody.clone(), &key(), "double".into()).is_err());
    drop(lease);
    assert!(custody.lock().unwrap().pending.is_empty());
    let mut lease = delivery::Lease::reserve(custody.clone(), &key(), "admitted".into()).unwrap();
    lease.admit(&receipt).unwrap();
    drop(lease);
    assert_eq!(
        custody.lock().unwrap().pending["admitted"]
            .receipt
            .as_ref()
            .unwrap()
            .intent
            .note(),
        Some(note().as_slice())
    );
    assert!(delivery::Lease::reserve(custody, &key(), "later".into()).is_err());
}
#[tokio::test]
async fn settlement_uses_captured_storage_after_owner_drop_and_preserves_unknown_note() {
    let temp = Temp::new();
    let storage = temp.storage();
    let receipt = temp.receipt();
    let custody = Arc::new(Mutex::new(recovery::Custody::default()));
    let mut lease = delivery::Lease::reserve(custody.clone(), &key(), "run".into()).unwrap();
    lease.admit(&receipt).unwrap();
    let (mut shell, _) = sample();
    shell.file_share.custody = custody.clone();
    let (mut replacement, _) = sample();
    replacement.core.active_account = Some("other".into());
    carry_pending(&mut shell, &mut replacement);
    drop(shell);
    let settled = delivery::settle(&storage, &lease, &receipt, Ok(json!({"ok":true})))
        .await
        .unwrap();
    assert!(!settled.unresolved());
    assert_eq!(settled.intent.note(), Some(note().as_slice()));
    assert!(Arc::ptr_eq(&replacement.file_share.custody, &custody));
    let query = receipt.scope.clone();
    assert!(
        !storage
            .run(move |j| j.get(&query))
            .await
            .unwrap()
            .unwrap()
            .unresolved()
    );
}
#[tokio::test]
async fn failed_durable_settlement_retains_cloned_result_and_route_wide_429_barrier() {
    let temp = Temp::new();
    let receipt = temp.receipt();
    let custody = Arc::new(Mutex::new(recovery::Custody::default()));
    let mut lease = delivery::Lease::reserve(custody.clone(), &key(), "run".into()).unwrap();
    lease.admit(&receipt).unwrap();
    let bad = recovery::Storage::at(key(), temp.0.clone());
    let result = delivery::settle(
        &bad,
        &lease,
        &receipt,
        Err(Error::RateLimited {
            retry_after_secs: Some(120),
        }),
    )
    .await;
    assert!(matches!(result, Err(Refusal::Storage)));
    drop(lease);
    let held = custody.lock().unwrap();
    assert!(matches!(
        held.pending["run"].outcome,
        Some(recovery::Retained::Result(Err(Error::RateLimited {
            retry_after_secs: Some(120)
        })))
    ));
    assert_eq!(held.check(&key(), recovery::now()), Err(Refusal::Cooldown));
    assert_eq!(
        held.pending["run"].receipt.as_ref().unwrap().intent.note(),
        Some(note().as_slice())
    );
}
#[tokio::test]
async fn actual_task_abort_after_admission_leaves_durable_and_memory_uncertainty() {
    let temp = Temp::new();
    let receipt = temp.receipt();
    let custody = Arc::new(Mutex::new(recovery::Custody::default()));
    let mut lease = delivery::Lease::reserve(custody.clone(), &key(), "run".into()).unwrap();
    lease.admit(&receipt).unwrap();
    let (started, received) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _lease = lease;
        let _ = started.send(());
        std::future::pending::<()>().await;
    });
    received.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(
        custody.lock().unwrap().pending["run"]
            .receipt
            .as_ref()
            .unwrap()
            .unresolved()
    );
    let scope = receipt.scope;
    assert!(
        temp.storage()
            .run(move |j| j.get(&scope))
            .await
            .unwrap()
            .unwrap()
            .unresolved()
    );
}

struct Mounted {
    job: RefCell<
        Option<(
            recovery::Storage,
            delivery::Lease,
            Receipt,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    >,
    shown: Cell<Option<Signal<bool>>>,
    done: Cell<bool>,
}

#[component]
fn SettlementChild() -> Element {
    let shared = use_context::<Rc<Mounted>>();
    use_hook(move || {
        let (storage, lease, receipt, gate) = shared.job.borrow_mut().take().unwrap();
        dioxus::core::spawn_forever(async move {
            let _ = gate.await;
            let saved = delivery::settle(&storage, &lease, &receipt, Ok(json!({"ok":true})))
                .await
                .unwrap();
            assert!(!saved.unresolved());
            shared.done.set(true);
        });
    });
    rsx! {div{"child"}}
}
#[tokio::test]
async fn root_owned_settlement_continues_after_originating_component_unmount() {
    let temp = Temp::new();
    let receipt = temp.receipt();
    let mut lease = delivery::Lease::reserve(
        Arc::new(Mutex::new(recovery::Custody::default())),
        &key(),
        "root".into(),
    )
    .unwrap();
    lease.admit(&receipt).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let shared = Rc::new(Mounted {
        job: RefCell::new(Some((temp.storage(), lease, receipt, rx))),
        shown: Cell::new(None),
        done: Cell::new(false),
    });
    let mut dom = VirtualDom::new_with_props(
        |shared: Rc<Mounted>| {
            use_context_provider(|| shared.clone());
            let shown = use_signal(|| true);
            shared.shown.set(Some(shown));
            rsx! {if shown(){SettlementChild{}}}
        },
        shared.clone(),
    );
    dom.rebuild_in_place();
    shared.shown.get().unwrap().set(false);
    dom.render_immediate(&mut dioxus::core::NoOpMutations);
    tx.send(()).unwrap();
    for _ in 0..100 {
        if shared.done.get() {
            break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(10), dom.wait_for_work()).await;
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
    }
    assert!(shared.done.get());
}

mod draft_cases;
mod retry_cases;

#[tokio::test]
async fn retained_recovery_review_cannot_replace_editing_or_a_newer_recovery_cycle() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let first = open(&mut shell, target.clone(), "source".into(), &temp)
        .await
        .unwrap();
    let context = admission::Context::capture(&shell).unwrap();
    close(&mut shell, &first);
    let scope = Scope {
        team: context.key.team,
        user: context.key.user,
        file: target.id.clone(),
        destination: "C00000001".into(),
    };
    let intent = Intent::new(
        scope.file.clone(),
        scope.destination.clone(),
        Some(note()),
        None,
    )
    .unwrap();
    let Begin::Started(receipt) = Journal::open(temp.0.join("review.sqlite"), ACCOUNT)
        .unwrap()
        .begin(scope, intent, "File".into(), "ship".into(), 10)
        .unwrap()
    else {
        panic!("new")
    };
    shell.file_share.recovery.open = true;
    shell.file_share.recovery.serial = 1;
    shell.file_share.recovery.receipts = vec![receipt.clone()];
    shell.overlay = Some(Overlay::FileShare);
    close_recovery(&mut shell, 1);
    let editing = open(&mut shell, target, "source".into(), &temp)
        .await
        .unwrap();
    shell.file_share.dialog.as_mut().unwrap().note = note();
    assert_eq!(
        open_receipt(&mut shell, 1, &receipt.attempt_id),
        Err(Refusal::Changed)
    );
    assert_eq!(visible(&shell).unwrap().id, editing);
    assert_eq!(visible(&shell).unwrap().note, note());
    close(&mut shell, &editing);
    shell.file_share.recovery.open = true;
    shell.file_share.recovery.serial = 2;
    shell.overlay = Some(Overlay::FileShare);
    assert_eq!(
        open_receipt(&mut shell, 1, &receipt.attempt_id),
        Err(Refusal::Changed)
    );
    assert!(visible(&shell).is_none());
    assert!(recovery_visible(&shell).is_some());
    let reviewed = open_receipt(&mut shell, 2, &receipt.attempt_id).unwrap();
    assert_eq!(visible(&shell).unwrap().id, reviewed);
    assert_eq!(
        visible(&shell)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .attempt_id,
        receipt.attempt_id
    );
}

mod policy_capacity;

mod durable_io;

mod durable_ui;
