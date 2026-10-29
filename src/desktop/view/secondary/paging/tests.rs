//! Generated control/state evidence, not native focus, layout or trusted-input acceptance.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn mount(alias: &'static str, loaded: bool) -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(slot, alias, loaded): (Slot, &'static str, bool)| {
            let state = use_signal(|| {
                let mut s =
                    ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
                s.core.dms.loaded = loaded;
                s
            });
            slot.set(Some(state));
            let shell = state.read();
            crate::view::secondary::dm_list_panel(state, &shell)
        },
        (slot.clone(), alias, loaded),
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
fn button(edits: &Mutations) -> Option<ElementId> {
    edits.edits.iter().find_map(|m| match m {
        Mutation::SetAttribute {
            id,
            name: "id",
            value: AttributeValue::Text(value),
            ..
        } if value == "dm-load-older" => Some(*id),
        _ => None,
    })
}
fn disabled(edits: &Mutations, id: ElementId, expected: bool) {
    assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"disabled",value:AttributeValue::Bool(value),..}if *other==id&&*value==expected)));
}
#[test]
fn generated_dm_paging_control_survives_zero_matches_and_busy_state_without_false_empty_completion()
{
    for alias in ["dms-paging-filtered", "dms-paging-filtered-narrow"] {
        let (_dom, state, edits) = mount(alias, true);
        assert_eq!(
            state.peek().core.dms.next_cursor.as_deref(),
            Some("fixture-older-page")
        );
        assert!(
            !state
                .peek()
                .core
                .dms
                .entries
                .iter()
                .any(|e| state.peek().dm_row_visible(e))
        );
        assert_eq!(
            empty_message(&state.peek()),
            Some("No matching loaded conversations.")
        );
        let id = button(&edits).expect("paging remains reachable with zero visible rows");
        disabled(&edits, id, false);
        assert!(
            edits
                .edits
                .iter()
                .any(|m| matches!(m,Mutation::CreateTextNode{value,..}if value==LABEL))
        );
    }
    for alias in ["dms-paging-loading", "dms-paging-loading-narrow"] {
        let (_dom, state, edits) = mount(alias, true);
        assert!(state.peek().core.dms.loading);
        assert_eq!(empty_message(&state.peek()), None);
        disabled(&edits, button(&edits).unwrap(), true);
        assert!(
            edits
                .edits
                .iter()
                .any(|m| matches!(m,Mutation::CreateTextNode{value,..}if value==LABEL))
        );
    }
    for alias in ["dms-paging-empty", "dms-paging-empty-narrow"] {
        for loaded in [false, true] {
            let (_dom, state, edits) = mount(alias, loaded);
            assert!(state.peek().core.dms.entries.is_empty());
            assert!(button(&edits).is_none());
            assert_eq!(
                empty_message(&state.peek()),
                loaded.then_some("No direct messages to show.")
            );
        }
    }
}

#[cfg(feature = "protected-native-fixture")]
#[tokio::test(flavor = "current_thread")]
async fn actual_dm_panel_paging_click_runs_the_disconnected_read_and_keeps_the_control_retryable() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let (mut dom, mut state, edits) = mount("dms-paging-filtered", true);
    let id = button(&edits).expect("actual DM panel owns the paging control");
    state.write().core.composer.text = "Keep paging draft".into();
    dom.render_immediate_to_vec();
    let before_rows = serde_json::to_value(&state.peek().core.dms.entries).unwrap();
    let before_channel = state.peek().core.active_channel.clone();
    for _ in 0..2 {
        let sequence = state.peek().core.dms.load_seq;
        let payload = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
        dom.in_scope(dioxus::core::ScopeId::ROOT, || {
            dom.runtime().handle_event(
                "click",
                dioxus::core::Event::new(payload.into_any(), true),
                id,
            )
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                dom.render_immediate_to_vec();
                if state.peek().core.dms.load_seq == sequence.wrapping_add(1)
                    && !state.peek().core.dms.loading
                {
                    break;
                }
                dom.wait_for_work().await;
            }
        })
        .await
        .expect("actual paging callback must execute and settle without any HTTP capability");
        let s = state.peek();
        assert_eq!(
            s.core.dms.next_cursor.as_deref(),
            Some("fixture-older-page")
        );
        assert_eq!(
            serde_json::to_value(&s.core.dms.entries).unwrap(),
            before_rows
        );
        assert_eq!(s.core.active_channel, before_channel);
        assert_eq!(s.core.composer.text, "Keep paging draft");
    }
}

#[cfg(feature = "protected-native-fixture")]
#[tokio::test(flavor = "current_thread")]
async fn actual_initial_dm_control_shows_unloaded_status_and_retries_disconnected_reads_without_scroll()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for alias in ["dms-initial-loading", "dms-initial-loading-narrow"] {
        let (_dom, state, edits) = mount(alias, false);
        assert!(state.peek().core.dms.loading);
        let id = edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(value),
                    ..
                } if value == "dm-load-initial" => Some(*id),
                _ => None,
            })
            .expect("initial control stays mounted while source is busy/unavailable");
        disabled(&edits, id, true);
        assert!(edits.edits.iter().any(
            |m| matches!(m,Mutation::CreateTextNode{value,..}if value=="Loading direct messages…")
        ));
    }
    for alias in ["dms-initial-unloaded", "dms-initial-unloaded-narrow"] {
        let (mut dom, mut state, edits) = mount(alias, false);
        let id = edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(value),
                    ..
                } if value == "dm-load-initial" => Some(*id),
                _ => None,
            })
            .expect("actual initial read control is mounted in DM panel");
        assert!(button(&edits).is_none());
        disabled(&edits, id, false);
        assert!(edits.edits.iter().any(|m|matches!(m,Mutation::CreateTextNode{value,..}if value=="Direct messages haven't loaded.")));
        let frame = crate::bootstrap::dms::Frame::capture(&state.peek()).unwrap();
        assert!(
            crate::bootstrap::dms::PageSource::capture(&state.peek(), &frame).is_none(),
            "scroll path cannot capture an initial action"
        );
        state.write().core.composer.text = "Keep initial draft".into();
        dom.render_immediate_to_vec();
        let before = state.peek().core.active_channel.clone();
        for _ in 0..2 {
            let sequence = state.peek().core.dms.load_seq;
            let payload =
                dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
            dom.in_scope(dioxus::core::ScopeId::ROOT, || {
                dom.runtime().handle_event(
                    "click",
                    dioxus::core::Event::new(payload.into_any(), true),
                    id,
                )
            });
            tokio::time::timeout(std::time::Duration::from_secs(2),async{loop{dom.render_immediate_to_vec();if state.peek().core.dms.load_seq==sequence.wrapping_add(1)&&!state.peek().core.dms.loading{break;}dom.wait_for_work().await;}}).await.expect("initial action must execute and return to retryable state without any HTTP capability");
            let s = state.peek();
            assert!(!s.core.dms.loaded);
            assert!(s.core.dms.entries.is_empty());
            assert!(s.core.dms.next_cursor.is_none());
            assert_eq!(s.core.active_channel, before);
            assert_eq!(s.core.composer.text, "Keep initial draft");
        }
    }
}
