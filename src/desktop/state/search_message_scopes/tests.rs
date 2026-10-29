use super::*;
use crate::bootstrap::test_support::{self, Harness};
use dioxus::prelude::*;
use serde_json::json;
use super_platinum_core::slack::models::{Channel, SearchPagination, User};
pub(crate) const C1: &str = "C12345678";
pub(crate) const C2: &str = "C87654321";
pub(crate) const U1: &str = "U12345678";
pub(crate) const U2: &str = "W87654321";
pub(crate) fn setup(extra: usize) -> Harness {
    let h = test_support::harness();
    let mut state = h.state;
    {
        let mut s = state.write();
        s.set_search_kind(SearchKind::Messages);
        s.set_search_query("release".into());
        assert!(s.core.cache.is_none());
        let w = s.core.workspaces.get_mut("T1").unwrap();
        for (id, name) in [(C1, "ship"), (C2, "design")] {
            w.channels.insert(
                id.into(),
                Channel {
                    id: id.into(),
                    name: Some(name.into()),
                    is_channel: true,
                    ..Default::default()
                },
            );
        }
        for (id, name) in [(U1, "person00"), (U2, "person01")] {
            w.users.insert(
                id.into(),
                User {
                    id: id.into(),
                    name: Some(name.into()),
                    real_name: Some("Maya Chen".into()),
                    ..Default::default()
                },
            );
        }
        for i in 2..extra {
            let id = format!("U{i:08}");
            w.users.insert(
                id.clone(),
                User {
                    id,
                    name: Some(format!("person{i:02}")),
                    real_name: Some(format!("Person {i:02}")),
                    ..Default::default()
                },
            );
        }
        let projected = crate::channel_vm::project_channels(
            &s.core.workspaces["T1"],
            &s.media,
            s.core.active_channel.as_deref(),
            &mut Default::default(),
        );
        s.channels = projected.0;
    }
    h
}
pub(crate) fn response(page: u32) -> serde_json::Value {
    json!({"ok":true,"items":[{"channel":{"id":C1,"name":"ship"},"messages":[{"user":U1,"ts":format!("120.{page:06}"),"text":format!("Page {page} release")}]}],"pagination":{"page":page,"page_count":2,"total_count":2}})
}
#[test]
fn choices_require_original_canonical_projection_and_current_metadata_without_human_only_or_unknown_defaults()
 {
    fn send<T: Send>() {}
    send::<ShellState>();
    let h = setup(2);
    let mut state = h.state;
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.users.get_mut(U1).unwrap().is_bot = true;
        w.users.insert(
            "U99999999".into(),
            User {
                id: "U88888888".into(),
                ..Default::default()
            },
        );
        w.users.insert(
            "U77777777".into(),
            User {
                id: "U77777777".into(),
                deleted: true,
                ..Default::default()
            },
        );
        w.channels.insert(
            "C99999999".into(),
            Channel {
                id: "C99999999".into(),
                name: Some("not projected".into()),
                ..Default::default()
            },
        );
    }
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let source = Source::capture(&state.peek()).unwrap();
    let authors = rows(&state.peek(), Scope::Authors);
    assert!(
        authors.iter().any(|r| r.id == U1),
        "existing cached bot eligibility preserved"
    );
    assert!(
        !authors
            .iter()
            .any(|r| r.id == "U99999999" || r.id == "U77777777")
    );
    assert!(
        !rows(&state.peek(), Scope::Channels)
            .iter()
            .any(|r| r.id == "C99999999")
    );
    set(
        &mut state.write(),
        &source,
        Scope::Channels,
        "C99999999",
        true,
    );
    assert_eq!(
        count(&state.peek(), Scope::Channels),
        0,
        "a raw ID cannot create a canonical projected choice"
    );
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut(U1)
        .unwrap()
        .deleted = true;
    set(&mut state.write(), &source, Scope::Authors, U1, true);
    assert_eq!(count(&state.peek(), Scope::Authors), 0);
    assert!(state.peek().search_filter_error.is_some());
    assert_eq!(state.peek().search_query, "release");
}
#[test]
fn apply_is_transactional_and_clear_rotates_shared_intent_before_a_queued_submission_can_reach_successor_query()
 {
    let h = setup(2);
    let mut state = h.state;
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let first = Source::capture(&state.peek()).unwrap();
    set(&mut state.write(), &first, Scope::Channels, C1, true);
    set(&mut state.write(), &first, Scope::Authors, U1, true);
    state.write().search_filters.as_mut().unwrap().on = "2026-02-29".into();
    assert!(apply(&mut state.write(), &first).is_none());
    assert_eq!(state.peek().search_query, "release");
    assert!(Source::capture(&state.peek()).is_some());
    assert!(
        !state.write().apply_search_filters(),
        "legacy Apply cannot bypass a typed draft"
    );
    clear(&mut state.write(), &first, Some(Scope::Authors), false);
    let current = Source::capture(&state.peek()).unwrap();
    assert!(!first.current(&state.peek()));
    set(&mut state.write(), &first, Scope::Authors, U2, true);
    assert_eq!(count(&state.peek(), Scope::Authors), 0);
    assert_eq!(count(&state.peek(), Scope::Channels), 1);
    state.write().search_filters.as_mut().unwrap().on = "2024-02-29".into();
    let work = apply(&mut state.write(), &current).unwrap();
    assert_eq!(
        state.peek().search_query,
        "release in:<#C12345678|ship> on:2024-02-29"
    );
    assert!(work.current(&state.peek()));
    state.write().set_search_query("a successor".into());
    assert!(!work.current(&state.peek()));
    assert!(!current.current(&state.peek()));
}
#[test]
fn actual_page_completion_retires_a_file_draft_even_when_request_revision_does_not_change() {
    use crate::state::search_filter_types as files;
    let h = setup(2);
    let mut state = h.state;
    let request = {
        let mut s = state.write();
        s.set_search_kind(SearchKind::Files);
        let epoch = s.core.account_epoch;
        let team = s.core.active_team.clone().unwrap();
        let query = s.search_query.clone();
        s.search_pager.begin(epoch, &team, &query, false).unwrap()
    };
    let revision = state.peek().search_pager.revision();
    let source = files::Toggle::capture(&state.peek());
    files::toggle(&mut state.write(), source);
    let draft = files::Source::capture(&state.peek()).unwrap();
    assert!(files::allows(&state.peek(), Some(&draft)));
    assert!(state.write().search_pager.complete(
        &request,
        Some(&SearchPagination {
            page: Some(1),
            page_count: Some(2),
            total_count: Some(2),
            ..Default::default()
        })
    ));
    assert_eq!(state.peek().search_pager.revision(), revision);
    assert!(!files::allows(&state.peek(), Some(&draft)));
    assert!(files::apply(&mut state.write(), &draft).is_none());
    assert_eq!(state.peek().search_query, "release");
}
