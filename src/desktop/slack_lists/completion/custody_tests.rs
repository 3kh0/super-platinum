use super::*;
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell, "slack-list-todo-completion");
    shell
}
fn begin_first(s: &mut ShellState) -> Task {
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    begin(
        s,
        "R_LIST_000",
        "C_TODO_DONE",
        EditAuthority::RecordDetail(detail.generation),
    )
    .unwrap()
}
fn sending(s: &mut ShellState) -> Task {
    let task = begin_first(s);
    fixture::start(s, task.clone());
    assert!(s.slack_lists.completion.entries[&task.serial].phase == Phase::PrimarySending);
    task
}
#[test]
fn detail_close_or_replacement_revokes_initial_dispatch_but_not_result_custody() {
    for replace in [false, true] {
        let mut s = shell();
        let task = begin_first(&mut s);
        let stamps = {
            let v = s.slack_lists.view.as_ref().unwrap();
            (v.generation, v.revision, v.layout.selection_generation)
        };
        record_detail::dismiss_shell(&mut s);
        if replace {
            let v = s.slack_lists.view.as_ref().unwrap();
            let args = (
                v.rows[0].instance_id.clone(),
                v.generation,
                v.layout.generation,
            );
            assert!(record_detail::open_shell(&mut s, &args.0, args.1, args.2));
        }
        let v = s.slack_lists.view.as_ref().unwrap();
        assert_eq!(
            stamps,
            (v.generation, v.revision, v.layout.selection_generation)
        );
        fixture::start(&mut s, task.clone());
        assert!(s.slack_lists.completion.fixture.forms.is_empty());
        assert!(s.slack_lists.completion.entries[&task.serial].phase == Phase::Stopped);
    }
    let mut s = shell();
    let sent = sending(&mut s);
    record_detail::dismiss_shell(&mut s);
    assert!(!sent.current(&s));
    assert!(!fixture::complete(&mut s, true, true));
    let read = task(&mut s, sent.serial).unwrap();
    assert!(read.authority.is_none() && read.current(&s));
    fixture::check(&mut s, read, false);
    assert!(s.slack_lists.completion.entries[&sent.serial].phase == Phase::ReferenceReady);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
    let next = dispatch_task(&mut s, sent.serial).unwrap();
    assert_eq!(next.authority, Some(EditAuthority::Layout));
    fixture::reference(&mut s, next);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 2);
}
#[test]
fn automatic_continuation_cannot_adopt_replacement_detail_or_read_only_authority() {
    let mut s = shell();
    let sent = sending(&mut s);
    fixture::complete(&mut s, true, true);
    record_detail::dismiss_shell(&mut s);
    let old = continuation_task(&mut s, &sent).unwrap();
    assert_eq!(old.authority, sent.authority);
    assert!(!old.current(&s));
    fixture::check(&mut s, old, false);
    let read = task(&mut s, sent.serial).unwrap();
    fixture::check(&mut s, read.clone(), false);
    assert!(continuation_task(&mut s, &read).is_none());
    let read = task(&mut s, sent.serial).unwrap();
    fixture::reference(&mut s, read);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
}
fn reference_view(s: &mut ShellState) {
    let loaded = s.slack_lists.completion.fixture.reference.clone().unwrap();
    let v = s.slack_lists.view.as_mut().unwrap();
    v.id = loaded.document.id.clone();
    v.loaded = Some(loaded);
    v.detail = None;
}
#[test]
fn retained_reference_excludes_schema_and_subitem_owners_but_not_unrelated_lists() {
    let mut s = shell();
    let sent = sending(&mut s);
    fixture::complete(&mut s, true, true);
    let read = task(&mut s, sent.serial).unwrap();
    fixture::check(&mut s, read, false);
    let next = dispatch_task(&mut s, sent.serial).unwrap();
    fixture::reference(&mut s, next);
    fixture::complete(&mut s, true, true);
    assert!(s.slack_lists.completion.entries[&sent.serial].phase == Phase::ReferenceUnknown);
    reference_view(&mut s);
    assert!(blocks_mutation(&s));
    assert!(!rename::completion_test_available(&s));
    assert!(!subitem::completion_test_available(&s));
    let entries = std::mem::take(&mut s.slack_lists.completion.entries);
    assert!(rename::completion_test_available(&s));
    assert!(subitem::completion_test_available(&s));
    s.slack_lists.completion.entries = entries;
    s.slack_lists.view.as_mut().unwrap().id = "F_UNRELATED".into();
    assert!(!blocks_mutation(&s));
}
#[test]
fn referenced_schema_receipt_blocks_begin_and_a_previously_queued_dispatch() {
    let mut s = shell();
    let queued = begin_first(&mut s);
    let own = std::mem::take(&mut s.slack_lists.completion.entries);
    let original = s.slack_lists.view.clone();
    reference_view(&mut s);
    // The schema owner requires a supported ordinary Text field. Keep this
    // setup local; the completion fixture's bare Text shape is display-only.
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    let column = raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["type"] == "text")
        .unwrap();
    column["options"] = serde_json::json!({"format":"text"});
    loaded.document = Document::parse(raw).unwrap();
    let (id, visit) = {
        let v = s.slack_lists.view.as_ref().unwrap();
        (
            v.loaded
                .as_ref()
                .unwrap()
                .document
                .columns
                .iter()
                .find(|c| c.kind == "text")
                .unwrap()
                .id
                .clone(),
            v.generation,
        )
    };
    assert!(rename::supports_column(&s, &id));
    assert!(rename::open_shell(&mut s, &id, visit));
    let scope = s.slack_lists.scope.clone().unwrap();
    assert!(rename::blocks_target(&scope, "F_TODO_SOURCE"));
    assert!(!rename::blocks_target(&scope, "F_OTHER"));
    s.slack_lists.view = original;
    assert!(conflicts(&s, &scope, "F_TODO_SOURCE"));
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    assert!(
        begin(
            &mut s,
            "R_LIST_000",
            "C_TODO_DONE",
            EditAuthority::RecordDetail(detail.generation)
        )
        .is_err()
    );
    s.slack_lists.completion.entries = own;
    fixture::start(&mut s, queued);
    assert!(s.slack_lists.completion.fixture.forms.is_empty());
}
struct Mounted {
    initial: RefCell<Option<ShellState>>,
    signal: Cell<Option<Signal<ShellState>>>,
}
#[test]
fn active_and_retired_referenced_cell_receipts_are_owner_scoped_conflicts() {
    let mut s = shell();
    let sent = sending(&mut s);
    let mirror = s.slack_lists.completion.entries[&sent.serial]
        .reference
        .as_ref()
        .unwrap()
        .clone();
    s.slack_lists.completion.entries.clear();
    let scope = s.slack_lists.scope.clone().unwrap();
    let v = s.slack_lists.view.as_ref().unwrap();
    let attempt = EditAttempt {
        date_policy: None,
        scope: scope.clone(),
        selection_generation: v.layout.selection_generation,
        authority: EditAuthority::Layout,
        serial: next_id(),
        generation: v.generation,
        revision: v.revision,
        edit: mirror.edit().clone(),
    };
    s.slack_lists.operations.insert(
        "F_TODO_SOURCE".into(),
        Mutation {
            attempt,
            phase: super::super::Phase::Uncertain,
        },
    );
    assert!(conflicts(&s, &scope, "F_TODO_SOURCE"));
    cell_receipts::retire(&mut s);
    assert!(s.slack_lists.operations.is_empty());
    assert!(conflicts(&s, &scope, "F_TODO_SOURCE"));
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    assert!(
        begin(
            &mut s,
            "R_LIST_000",
            "C_TODO_DONE",
            EditAuthority::RecordDetail(detail.generation)
        )
        .is_err()
    );
    assert!(!conflicts(&s, &scope, "F_OTHER"));
    s.core.active_account = Some("foreign".into());
    let foreign = Scope::capture(&s).unwrap();
    assert!(!conflicts(&s, &foreign, "F_TODO_SOURCE"));
}
fn mount(s: ShellState) -> (VirtualDom, Signal<ShellState>) {
    let state = Rc::new(Mounted {
        initial: RefCell::new(Some(s)),
        signal: Cell::new(None),
    });
    let mut dom = VirtualDom::new_with_props(
        |p: Rc<Mounted>| {
            let initial = p.clone();
            let s = use_signal(move || initial.initial.borrow_mut().take().unwrap());
            p.signal.set(Some(s));
            rsx! {div{}}
        },
        state.clone(),
    );
    dom.rebuild(&mut dioxus::core::NoOpMutations);
    (dom, state.signal.get().unwrap())
}
#[test]
fn actual_install_loaded_retains_unknown_and_late_receipts_across_accounts() {
    for foreign in [false, true] {
        let mut before = shell();
        let sent = sending(&mut before);
        let expected = before.slack_lists.completion.entries[&sent.serial]
            .intent
            .primary()
            .edit()
            .cells()
            .clone();
        let mut loaded = shell();
        if foreign {
            loaded.core.active_account = Some("different-account".into());
            loaded.slack_lists.scope = Scope::capture(&loaded);
        }
        let (dom, mut state) = mount(before);
        dom.in_scope(ScopeId::ROOT, || {
            crate::bootstrap::install_loaded(state, loaded)
        });
        assert!(sent.owned(&state.peek()));
        assert!(!sent.current(&state.peek()));
        assert!(!settled(
            &mut state.write(),
            &sent,
            false,
            Err(Error::Transport("lost reply".into()))
        ));
        let s = state.peek();
        let entry = &s.slack_lists.completion.entries[&sent.serial];
        assert!(entry.phase == Phase::PrimaryUnknown);
        assert_eq!(entry.intent.primary().edit().cells(), expected);
        assert!(entry.running.is_none());
        drop(s);
        assert_eq!(task(&mut state.write(), sent.serial).is_some(), !foreign);
    }
}
#[test]
fn actual_reload_preserves_reference_unknown_without_reauthorizing_old_job() {
    let mut s = shell();
    let first = sending(&mut s);
    fixture::complete(&mut s, true, true);
    let read = task(&mut s, first.serial).unwrap();
    fixture::check(&mut s, read, false);
    let second = dispatch_task(&mut s, first.serial).unwrap();
    fixture::reference(&mut s, second.clone());
    fixture::complete(&mut s, true, true);
    let before = s.slack_lists.completion.entries[&first.serial].clone();
    let (dom, state) = mount(s);
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::install_loaded(state, shell())
    });
    let s = state.peek();
    let after = &s.slack_lists.completion.entries[&first.serial];
    assert!(after.phase == Phase::ReferenceUnknown);
    assert_eq!(after.owner, before.owner);
    assert_eq!(
        after.reference.as_ref().unwrap().edit().cells(),
        before.reference.as_ref().unwrap().edit().cells()
    );
    assert_eq!(after.retry_at, before.retry_at);
    assert!(!second.current(&s));
    assert!(s.slack_lists.completion.fixture.forms.is_empty());
}

#[test]
fn retained_assignment_excludes_primary_and_referenced_completion_without_losing_intent() {
    for referenced in [false, true] {
        let mut s = shell();
        s.core.active_account = Some(format!("mutual-{}", next_id()));
        s.slack_lists.scope = Scope::capture(&s);
        let original = s.slack_lists.view.clone();
        if referenced {
            reference_view(&mut s);
        } else {
            record_detail::dismiss_shell(&mut s);
        }
        assert!(editor::assignment::fixture::seed(&mut s));
        let scope = s.slack_lists.scope.clone().unwrap();
        let list = s.slack_lists.view.as_ref().unwrap().id.clone();
        assert!(editor::assignment::blocks_target(&scope, &list));
        s.slack_lists.view = original;
        let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
        assert!(
            begin(
                &mut s,
                "R_LIST_000",
                "C_TODO_DONE",
                EditAuthority::RecordDetail(detail.generation)
            )
            .is_err()
        );
        assert!(s.slack_lists.completion.entries.is_empty());
        assert!(s.slack_lists.completion.fixture.forms.is_empty());
        assert!(editor::assignment::blocks_target(&scope, &list));
        // Isolate the next iteration's account without removing retained grants.
        s.core.active_account = Some(format!("other-owner-{}", next_id()));
        let foreign = Scope::capture(&s).unwrap();
        assert!(!editor::assignment::blocks_target(&foreign, &list));
    }
}
