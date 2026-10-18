use super::*;
#[test]
fn typing_keeps_native_time_nodes_but_explicit_date_and_slot_choices_replace_them() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| super::super::tests::setup().0);
            slot.set(Some(state));
            rsx! { div {} }
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    let mut state = slot.get().unwrap();
    let id = state
        .peek()
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .unwrap()
        .id;
    let original = active(&state.peek(), id).unwrap().time_revision;
    dom.in_runtime(|| edit(state, id, Field::Start, "".into()));
    assert!(event(&state.peek(), id).is_none());
    assert_eq!(active(&state.peek(), id).unwrap().time_revision, original);
    dom.in_runtime(|| edit(state, id, Field::Title, "unrelated rerender".into()));
    assert_eq!(active(&state.peek(), id).unwrap().time_revision, original);
    dom.in_runtime(|| edit(state, id, Field::Start, "09:15".into()));
    assert!(event(&state.peek(), id).is_some());
    dom.in_runtime(|| edit(state, id, Field::Date, "2026-10-19".into()));
    let next = active(&state.peek(), id).unwrap().time_revision;
    assert_ne!(next, original);
    assert!(active(&state.peek(), id).unwrap().start_text.is_empty());
    let selected = cal::time::parse("2026-10-19", "09:30").unwrap();
    dom.in_runtime(|| {
        active_mut(&mut state.write(), id).unwrap().slots = vec![cal::Slot {
            start: selected,
            end: selected + 3600,
            all_free: true,
            busy: vec![],
        }];
    });
    dom.in_runtime(|| super::slot(state, id, selected.to_string()));
    assert_ne!(active(&state.peek(), id).unwrap().time_revision, next);
    assert!(event(&state.peek(), id).is_some());
}
#[test]
fn fresh_draft_retires_old_reads_and_callbacks_even_when_query_generation_repeats() {
    let (mut s, old) = super::super::tests::setup();
    let c = active_mut(&mut s, old).unwrap();
    c.stage = Stage::Uncertain;
    c.slot_generation = 1;
    c.guest_generation = 1;
    c.share_generation = 1;
    s.core.composer.text = "retained composer".into();
    let (new, ctx) = fresh_owned(&mut s, old).unwrap();
    assert_ne!(new, old);
    assert!(super::super::context(&s, new).is_some());
    let c = active_mut(&mut s, new).unwrap();
    c.stage = Stage::Editing;
    c.slot_generation = 1;
    c.guest_generation = 1;
    c.share_generation = 1;
    c.title = "newer draft".into();
    assert!(!current(&s, old));
    assert!(active_mut(&mut s, old).is_none());
    close(&mut s, old, true);
    assert_eq!(active(&s, new).unwrap().title, "newer draft");
    assert_eq!(context(&s, new).unwrap().epoch, ctx.epoch);
    assert_eq!(s.core.composer.text, "retained composer");
    s.channel_generation += 1;
    assert!(fresh_owned(&mut s, new).is_none());
}

#[test]
fn actual_edit_retains_overflow_and_cannot_send_the_previous_valid_text() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| super::super::tests::setup().0);
            slot.set(Some(state));
            rsx! { div {} }
        },
        slot.clone(),
    );
    dom.rebuild_in_place();
    let mut state = slot.get().unwrap();
    let id = state
        .peek()
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .unwrap()
        .id;
    for (field, limit) in [(Field::Title, 4096), (Field::Description, 16384)] {
        dom.in_runtime(|| edit(state, id, field, "valid previous text".into()));
        assert!(event(&state.peek(), id).is_some());
        // A multibyte paste can exceed the byte bound even when its character
        // count is smaller. The visible draft must remain the captured draft.
        let overflow = "\u{6771}".repeat(limit / 3 + 1);
        dom.in_runtime(|| edit(state, id, field, overflow.clone()));
        {
            let s = state.peek();
            let c = active(&s, id).unwrap();
            let retained = match field {
                Field::Title => &c.title,
                Field::Description => &c.description,
                _ => unreachable!(),
            };
            assert_eq!(retained, &overflow);
            assert!(event(&s, id).is_none());
            assert!(s.message_buttons.shortcuts.calendar_attempts.is_empty());
        }
        dom.in_runtime(|| edit(state, id, field, "corrected text".into()));
        let captured = event(&state.peek(), id).unwrap();
        match field {
            Field::Title => assert_eq!(captured.title, "corrected text"),
            Field::Description => assert_eq!(captured.description, "corrected text"),
            _ => unreachable!(),
        }
    }
    dom.in_runtime(|| state.write().channel_generation += 1);
    dom.in_runtime(|| edit(state, id, Field::Title, "stale callback".into()));
    assert_eq!(
        state
            .peek()
            .message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .calendar
            .as_ref()
            .unwrap()
            .title,
        "corrected text"
    );
}
