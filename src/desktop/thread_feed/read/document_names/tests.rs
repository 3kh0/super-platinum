//! One complete Doc-only name journey through original page/policy/member IO.
use super::*;
use crate::{
    app_home::autocomplete::observations,
    bootstrap::test_support::{harness, users},
    resource_reads::render_test::{Captured, settle, text},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::{
    slack::{
        PreparedRequest,
        realtime::{self, RtUpdate},
    },
    state::posting_metadata::authority::{Authority, BootRead},
};

fn roster() -> ThreadsViewPage {
    let threads = (0..30).map(|i| {
        let seconds = 1719800000u64+i*10;
        let root = format!("{seconds}.000000");
        json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"subscribed":true,"user":"UPOST","reply_count":3,"text":format!("Doc root {i}"),"document_comment":{"most_recent_editor":format!("UD{:03}",i*4)}},
            "latest_replies":(1..=3).map(|n|json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":format!("{}.{n:06}",seconds+1),"user":"UPOST","text":"Doc reply","document_comment":{"authors":[format!("UD{:03}",i*4+n)]}})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({"threads":threads,"has_more":false})).unwrap()
}
fn ids(from: usize, to: usize) -> Vec<String> {
    (from..to).map(|i| format!("UD{i:03}")).collect()
}
fn members(from: usize, to: usize) -> Value {
    json!({"results":ids(from,to).into_iter().map(|id|json!({"id":id,"team_id":"T1","enterprise_user":null,"name":format!("person_{id}"),"is_bot":false,"is_app_user":false,"deleted":false,"can_interact":true,"profile":{"display_name":format!("Display {id}"),"real_name":format!("Real {id}")}})).collect::<Vec<_>>()})
}
fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Captured) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = crate::view::thread_list_for_test(state, &state.read());
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), captured)
}
fn body(c: &Captured) -> String {
    text(c.borrow().as_ref().unwrap())
}
fn article_text(c: &Captured, marker: &str) -> String {
    use dioxus::core::{DynamicNode, TemplateNode};
    fn plain(v: &VNode, n: &TemplateNode) -> String {
        match n {
            TemplateNode::Text { text } => (*text).into(),
            TemplateNode::Element { children, .. } => {
                children.iter().map(|n| plain(v, n)).collect()
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => t.value.clone(),
                DynamicNode::Fragment(nodes) => nodes.iter().map(text).collect(),
                _ => String::new(),
            },
        }
    }
    fn find(v: &VNode, n: &TemplateNode, marker: &str) -> Option<String> {
        match n {
            TemplateNode::Element { tag, children, .. } => {
                let value = plain(v, n);
                if *tag == "article" && value.contains(marker) {
                    return Some(value);
                }
                children.iter().find_map(|n| find(v, n, marker))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => nodes
                    .iter()
                    .find_map(|v| v.template.roots.iter().find_map(|n| find(v, n, marker))),
                _ => None,
            },
            _ => None,
        }
    }
    let captured = c.borrow();
    let v = captured.as_ref().unwrap();
    v.template
        .roots
        .iter()
        .find_map(|n| find(v, n, marker))
        .unwrap()
}
fn apply(state: &mut Signal<ShellState>, updates: impl IntoIterator<Item = RtUpdate>) {
    for update in updates {
        match update {
            RtUpdate::Connected {
                generation,
                connection,
            } => {
                let s = state.peek();
                let workspace = &s.core.workspaces["T1"];
                assert_eq!(generation, workspace.rt_generation);
                let super_platinum_core::state::RealtimeStatus::Connected(current) = &workspace.rt
                else {
                    panic!("the actual inert connection must already be installed");
                };
                assert!(current.read_identity().is_for(&connection));
            }
            RtUpdate::Preference {
                generation,
                carrier,
            } => {
                // The native worker unwraps this ORIGINAL carrier to the same
                // typed event before the existing account/core handlers.
                let event = carrier.event().clone();
                assert!(matches!(
                    event,
                    super_platinum_core::slack::events::RtEvent::PreferencesChanged { .. }
                ));
                let mut shell = state.write();
                assert_eq!(generation, shell.core.workspaces["T1"].rt_generation);
                crate::app_home::realtime(&mut shell, "T1", generation, &event);
                crate::realtime::apply_in_view_with_context(
                    &mut shell, "T1", generation, event, true, true,
                );
            }
            RtUpdate::Event { generation, event } => {
                let mut shell = state.write();
                crate::app_home::realtime(&mut shell, "T1", generation, &event);
                crate::realtime::apply_in_view_with_context(
                    &mut shell, "T1", generation, *event, true, true,
                );
            }
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => crate::thread_feed::accept_read_barrier(
                &mut state.write(),
                "T1",
                generation,
                barrier,
            ),
            _ => panic!("only inert event/barrier updates"),
        }
    }
}
fn request_method(r: &PreparedRequest) -> String {
    url::Url::parse(&r.url)
        .unwrap()
        .path()
        .strip_prefix("/api/")
        .unwrap()
        .into()
}
fn policy_raw(method: &str, state: Signal<ShellState>, preference: Value) -> Value {
    let s = state.peek();
    let me = &s.core.workspaces["T1"].self_user_id;
    match method {
        "client.gantryBoot" => {
            json!({"ok":true,"self":{"id":me},"team":{"id":"T1"},"workspaces":[{"id":"T1","prefs":{"display_real_names":true}}]})
        }
        "users.prefs.get" => {
            json!({"ok":true,"prefs":{"display_real_names_override":preference,"hidden_users":"UD001","frecency":"{}"}})
        }
        "experiments.getByUser" => json!({"ok":true,"assignments":{}}),
        other => panic!("unexpected policy read {other}"),
    }
}
fn start_policy(
    dom: &VirtualDom,
    state: Signal<ShellState>,
    preference: Value,
    hold: Option<tokio::sync::oneshot::Receiver<()>>,
    calls: Rc<RefCell<Vec<String>>>,
) -> Rc<Cell<bool>> {
    // Keep the original negative/default fixture unqualified with an explicit
    // truthy nonBoolean raw preference: the fresh Source recipes disagree.
    let team = if preference == json!(0) {
        json!("yes")
    } else {
        json!(true)
    };
    start_policy_with_team(dom, state, preference, team, hold, calls)
}
fn start_policy_with_team(
    dom: &VirtualDom,
    state: Signal<ShellState>,
    preference: Value,
    team_preference: Value,
    hold: Option<tokio::sync::oneshot::Receiver<()>>,
    calls: Rc<RefCell<Vec<String>>>,
) -> Rc<Cell<bool>> {
    let context = Arc::new(crate::scheduled::Context::capture(&state.peek()).unwrap());
    let authority = Authority::capture(&state.peek().core, "T1").unwrap();
    let hold = Rc::new(RefCell::new(hold));
    let done = Rc::new(Cell::new(false));
    let completed = done.clone();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            let read = |r: PreparedRequest| {
                let method = request_method(&r);
                calls.borrow_mut().push(method.clone());
                let wait = if method == "users.prefs.get" {
                    hold.borrow_mut().take()
                } else {
                    None
                };
                let mut raw = policy_raw(&method, state, preference.clone());
                if method == "client.gantryBoot" {
                    raw["workspaces"][0]["prefs"]["display_real_names"] = team_preference.clone();
                }
                async move {
                    if let Some(wait) = wait {
                        wait.await.unwrap();
                    }
                    Ok(raw)
                }
            };
            let result =
                observations::acquire_with(state, context, authority, &|_| true, &|_| true, &read)
                    .await
                    .unwrap();
            assert!(result.store.current(&state.peek()));
            completed.set(true);
        })
    });
    done
}
async fn admit(state: Signal<ShellState>, mode: Mode) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, mode, |_| async { Ok(roster()) })
            .await
            .is_some()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thread_document_labels_original_doc_only_authors_cold_policy_masked_batches_and_retired_responses()
 {
    let h = harness();
    let mut initial = h.state;
    let (connection, updates) = realtime::protected_connection(12);
    {
        let mut s = initial.write();
        s.main_view = MainView::Threads;
        s.overlay = None;
        s.core.threads_view = Default::default();
        s.core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled
            .insert("T1".into(), false);
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 12;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        w.users.insert("UD119".into(),serde_json::from_value(json!({"id":"UD119","name":"CACHE MUST NOT CLASSIFY","profile":{"display_name":"CACHE MUST NOT CLASSIFY"}})).unwrap());
    }
    apply(&mut initial, updates);
    {
        let context = crate::scheduled::Context::capture(&initial.peek()).unwrap();
        let mut s = initial.write();
        let boot = BootRead::begin(&mut s.core, &context.transport, &context.session).unwrap();
        assert!(boot.apply(&mut s.core,json!({"ok":true,"team":{"id":"T1","plan":"plus"},"self":{"id":context.session.user_id,"team_id":"T1"},"workspaces":[{"id":"T1"}],"read_only_channels":[],"thread_only_channels":[]}),false).unwrap());
    }
    admit(initial, Mode::Refresh).await;
    assert!(
        initial.peek().thread_feed_reads.profiles.is_none(),
        "no ordinary actor is required or created"
    );
    let canonical = users(initial);
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let io = Rc::new(profiles::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    assert!(
        body(&captured).contains("UD000") && !body(&captured).contains("CACHE MUST NOT CLASSIFY")
    );
    dom.in_scope(ScopeId::ROOT, || schedule(state));
    assert!(
        state
            .peek()
            .thread_feed_reads
            .document_names
            .as_ref()
            .unwrap()
            .phase
            .lock()
            .unwrap()
            .requested
            .is_empty()
    );

    let first = io.expect(ids(0, 100));
    let remaining = io.expect(ids(100, 120));
    let (release, wait) = tokio::sync::oneshot::channel();
    let policy_calls = Rc::new(RefCell::new(Vec::new()));
    let completed = start_policy(&dom, state, json!(-1), Some(wait), policy_calls.clone());
    settle(&mut dom, || policy_calls.borrow().len() == 2).await;
    assert!(
        io.reads.borrow().is_empty(),
        "cold policy waiting cannot consume label admission"
    );
    release.send(()).unwrap();
    settle(&mut dom, || completed.get() && io.reads.borrow().len() == 1).await;
    assert!(
        body(&captured).contains("Name hidden"),
        "current mask applies before human IO finishes"
    );
    first.send(Ok(members(0, 100))).unwrap();
    settle(&mut dom, || {
        io.reads.borrow().len() == 2 && body(&captured).contains("Display UD000")
    })
    .await;
    assert!(body(&captured).contains("UD119") && !body(&captured).contains("Real UD000"));
    assert!(!body(&captured).contains("View Display UD000 profile"));
    // The original native mutation boundary observes a changed Doc coordinate.
    // Restoring its public fields cannot revive this original label candidate.
    for editor in ["UCHANGED", "UD119"] {
        let raw = json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"type":"message","channel":"C1","team":"T1","thread_ts":"1719800290.000000","ts":"1719800291.000003","document_comment":{"most_recent_editor":editor}}});
        apply(
            &mut state,
            realtime::protected_updates(&connection, 12, &raw.to_string()),
        );
    }
    remaining.send(Ok(members(100, 120))).unwrap();
    settle(&mut dom, || {
        io.completed.get() == 2 && body(&captured).contains("Display UD118")
    })
    .await;
    assert!(!body(&captured).contains("Display UD119"));
    assert_eq!(users(state), canonical);
    assert_eq!(*io.reads.borrow(), vec![ids(0, 100), ids(100, 120)]);

    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async {
                Err(Error::Offline("closed outage".into()))
            })
            .await
            .is_none()
    );
    dom.render_immediate_to_vec();
    assert!(
        body(&captured).contains("Display UD000") && body(&captured).contains("Display UD118"),
        "failed refresh preserves valid loaded label Strings"
    );
    // A domain-only/public replacement does not own the native page even when
    // all Doc coordinates and editor IDs remain equal.
    let native_sequence = state.peek().core.threads_view.load_seq;
    let unowned = state.write().core.threads_view.begin(false).unwrap();
    assert!(state.write().core.apply_thread_feed_page(
        "T1",
        &unowned,
        roster(),
        &Default::default()
    ));
    dom.render_immediate_to_vec();
    assert!(!body(&captured).contains("Display UD000"));
    state.write().core.threads_view.load_seq = native_sequence;
    dom.render_immediate_to_vec();
    assert!(
        !body(&captured).contains("Display UD000"),
        "restoring public counters cannot revive the observed unowned page"
    );
    admit(state, Mode::Refresh).await;
    let first = io.expect(ids(0, 100));
    let last = io.expect(ids(100, 120));
    dom.in_scope(ScopeId::ROOT, || schedule(state));
    settle(&mut dom, || io.reads.borrow().len() == 3).await;
    first.send(Ok(members(0, 100))).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 4).await;
    last.send(Ok(members(100, 120))).unwrap();
    settle(&mut dom, || body(&captured).contains("Display UD119")).await;

    // An accepted native preference event retires the old immutable policy.
    for preference in [0, 1] {
        let raw =
            json!({"type":"pref_change","name":"display_real_names_override","value":preference});
        apply(
            &mut state,
            realtime::protected_updates(&connection, 12, &raw.to_string()),
        );
        dom.render_immediate_to_vec();
        assert!(!body(&captured).contains("Display UD000"));
        let first = if preference == 1 {
            Some(io.expect(ids(0, 100)))
        } else {
            None
        };
        let remaining = if preference == 1 {
            Some(io.expect(ids(100, 120)))
        } else {
            None
        };
        let done = start_policy(&dom, state, json!(preference), None, policy_calls.clone());
        settle(&mut dom, || done.get()).await;
        if preference == 0 {
            assert_eq!(
                io.reads.borrow().len(),
                4,
                "unproved default policy never starts a human label read"
            );
        } else {
            settle(&mut dom, || io.reads.borrow().len() == 5).await;
            first.unwrap().send(Ok(members(0, 100))).unwrap();
            settle(&mut dom, || {
                io.reads.borrow().len() == 6 && body(&captured).contains("Real UD000")
            })
            .await;
            assert!(
                body(&captured).contains("Name hidden")
                    && !body(&captured).contains("Display UD000")
            );
            let epoch = state.peek().core.account_epoch;
            state.write().core.account_epoch = epoch + 1;
            retire_missing(&state.peek());
            state.write().core.account_epoch = epoch;
            remaining.unwrap().send(Ok(members(100, 120))).unwrap();
            settle(&mut dom, || io.completed.get() == 6).await;
            dom.render_immediate_to_vec();
            assert!(!body(&captured).contains("Real UD118"));
            dom.in_scope(ScopeId::ROOT, || schedule(state));
            dom.render_immediate_to_vec();
            assert_eq!(
                io.reads.borrow().len(),
                6,
                "observed account retirement cannot replay after scalar restoration"
            );
        }
    }
    assert_eq!(users(state), canonical);
    assert!(state.peek().thread_feed_reads.profiles.is_none());
    {
        let s = state.peek();
        let source = Source::capture(&s).unwrap();
        assert!(
            s.core
                .threads_view
                .items
                .iter()
                .all(|item| source.profile(&s, item, &item.root_msg).is_none())
        );
    }
    // A genuine ordinary actor sharing the Doc editor can provide an older
    // strict Member. A known mask must still win when override0 leaves the new
    // formatting/IO profile unqualified; do not leak via legacy fallback.
    let raw = json!({"type":"pref_change","name":"display_real_names_override","value":0});
    apply(
        &mut state,
        realtime::protected_updates(&connection, 12, &raw.to_string()),
    );
    let done = start_policy(&dom, state, json!(0), None, policy_calls.clone());
    settle(&mut dom, || done.get()).await;
    let mut mixed = roster();
    mixed.threads[29] = serde_json::from_value(json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":"1719900999.000000","user":"UD001","text":"Ordinary mask control","reply_count":0},"latest_replies":[]})).unwrap();
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(mixed) })
            .await
            .is_some()
    );
    let human = io.expect(ids(1, 2));
    let source = Source::capture(&state.peek()).unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(profiles::load(source, state))
    });
    settle(&mut dom, || io.reads.borrow().len() == 7).await;
    human.send(Ok(members(1, 2))).unwrap();
    settle(&mut dom, || body(&captured).contains("Display UD001")).await;
    let doc = article_text(&captured, "Doc root 0");
    assert!(doc.contains("Name hidden") && !doc.contains("Display UD001"));
    let s = state.peek();
    let source = Source::capture(&s).unwrap();
    let ordinary = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.text.as_deref() == Some("Ordinary mask control"))
        .unwrap();
    assert!(
        source.profile(&s, ordinary, &ordinary.root_msg).is_some(),
        "the original ordinary actor control remains qualified"
    );
    let doc = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.text.as_deref() == Some("Doc root 0"))
        .unwrap();
    assert!(source.profile(&s, doc, &doc.latest_replies[0]).is_none());
    assert_eq!(users(state), canonical);
}

mod defaults;
