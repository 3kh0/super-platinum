use super::*;
use crate::media::{MediaRegistry, MediaUri};
use dioxus::core::{AttributeValue, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};

fn shell(name: &str) -> ShellState {
    ShellState::fixture_variant(MediaRegistry::default(), name)
}
fn bookmark(emoji: Option<&str>) -> Bookmark {
    Bookmark {
        id: "B1".into(),
        title: Some("Launch guide".into()),
        link: Some("https://example.com/launch".into()),
        kind: Some("link".into()),
        emoji: emoji.map(str::to_owned),
        ..Default::default()
    }
}
fn catalog(shell: &mut ShellState, values: &[(&str, &str)]) {
    shell
        .core
        .workspaces
        .get_mut(shell.core.active_team.as_deref().unwrap())
        .unwrap()
        .replace_emojis(
            values
                .iter()
                .map(|(name, value)| {
                    serde_json::from_value(serde_json::json!({"name":name,"value":value})).unwrap()
                })
                .collect(),
        );
}

#[test]
fn standard_shortcodes_and_custom_aliases_resolve_without_mutating_metadata() {
    let mut state = shell("bookmarks");
    catalog(&mut state, &[("launch", "alias:rocket")]);
    for name in ["rocket", ":rocket:", " :launch: "] {
        let item = bookmark(Some(name));
        let before = item.clone();
        assert_eq!(project(&state, &item), Decoration::Glyph("🚀".into()));
        assert_eq!(item, before);
    }
}

#[test]
fn absent_unknown_empty_invalid_and_cyclic_metadata_use_material_fallback() {
    let mut state = shell("bookmarks");
    catalog(
        &mut state,
        &[("bad", "javascript:alert(1)"), ("cycle", "alias:cycle")],
    );
    for value in [
        None,
        Some(""),
        Some(" : : "),
        Some(":unknown-bookmark-value:"),
        Some("bad"),
        Some("cycle"),
    ] {
        assert_eq!(
            project(&state, &bookmark(value)),
            Decoration::Bookmark,
            "{value:?}"
        );
    }
}

#[test]
fn pending_custom_alias_becomes_ready_and_decoder_failure_restores_fallback() {
    let mut state = shell("bookmarks");
    catalog(
        &mut state,
        &[
            ("team", "https://example.test/team.png"),
            ("alias", "alias:team"),
        ],
    );
    let item = bookmark(Some(":alias:"));
    assert_eq!(project(&state, &item), Decoration::Bookmark);
    let id = custom_id(
        &state,
        state.core.active_team.as_deref().unwrap(),
        "alias",
        "https://example.test/team.png",
    );
    state
        .media
        .insert(id.clone(), "image/png", b"fixture bytes".as_slice());
    assert_eq!(project(&state, &item), Decoration::Image(id.clone()));
    let revision = state.media.image_state(&id).0.unwrap();
    assert!(state.media.reject_image(&id, revision));
    assert_eq!(project(&state, &item), Decoration::Bookmark);
}

#[test]
fn workspace_switch_and_catalog_replacement_do_not_reuse_another_catalog_image() {
    let mut state = shell("bookmarks-emoji");
    let first_team = state.core.active_team.clone().unwrap();
    let item = bookmark(Some(":bookmark-ready:"));
    let Decoration::Image(first) = project(&state, &item) else {
        panic!("fixture must start ready")
    };
    let mut other = state.core.workspaces[&first_team].clone();
    other.team_id = "T_OTHER".into();
    other.replace_emojis(vec![]);
    state.core.workspaces.insert("T_OTHER".into(), other);
    state.core.active_team = Some("T_OTHER".into());
    assert_eq!(project(&state, &item), Decoration::Bookmark);
    catalog(
        &mut state,
        &[("bookmark-ready", "https://example.test/other.png")],
    );
    assert_eq!(project(&state, &item), Decoration::Bookmark);
    let second = custom_id(
        &state,
        "T_OTHER",
        "bookmark-ready",
        "https://example.test/other.png",
    );
    assert_ne!(first, second);
    state
        .media
        .insert(second.clone(), "image/png", b"other bytes".as_slice());
    assert_eq!(project(&state, &item), Decoration::Image(second));
    state.core.active_team = Some(first_team);
    assert_eq!(project(&state, &item), Decoration::Image(first.clone()));
    catalog(&mut state, &[]);
    assert_eq!(project(&state, &item), Decoration::Bookmark);
    assert_ne!(cache_key("A", "T", "team"), cache_key("B", "T", "team"));
    assert_ne!(cache_key("A", "T", "team"), cache_key("A", "U", "team"));
}

#[test]
fn folder_disclosure_wins_even_without_workspace_and_link_identity_is_retained() {
    let mut state = shell("bookmarks-emoji");
    let before = state.core.bookmarks.items.clone();
    let folder = before.iter().find(|b| b.folder()).unwrap();
    assert_eq!(project(&state, folder), Decoration::Folder);
    state.core.active_team = None;
    assert_eq!(project(&state, folder), Decoration::Folder);
    assert_eq!(
        project(&state, &bookmark(Some("rocket"))),
        Decoration::Bookmark
    );
    assert_eq!(state.core.bookmarks.items, before);
}

#[test]
fn named_normal_and_narrow_fixtures_are_disconnected_and_cover_all_decorations() {
    for name in ["bookmarks-emoji", "bookmarks-emoji-narrow"] {
        let state = shell(name);
        assert!(state.core.session.is_none() && state.core.transport.is_none());
        assert_eq!(state.core.bookmarks.items.len(), 7);
        assert!(super::super::visible(&state, "C2").is_some());
        let decorations: Vec<_> = state
            .core
            .bookmarks
            .items
            .iter()
            .map(|b| project(&state, b))
            .collect();
        assert_eq!(decorations[0], Decoration::Glyph("🚀".into()));
        assert!(matches!(decorations[1], Decoration::Image(_)));
        assert_eq!(
            &decorations[2..5],
            &[
                Decoration::Bookmark,
                Decoration::Bookmark,
                Decoration::Bookmark
            ]
        );
        assert_eq!(decorations[5], Decoration::Folder);
        assert_eq!(
            state.core.bookmarks.items[6].parent_id.as_deref(),
            Some("BkEmojiFolder")
        );
    }
}

fn attribute(edits: &Mutations, name: &str, value: &str) -> bool {
    edits.edits.iter().any(|edit| {
        matches!(edit, Mutation::SetAttribute {
        name: actual, value: AttributeValue::Text(text), ..
    } if *actual == name && text == value)
    })
}

#[test]
fn actual_panel_mount_keeps_title_link_and_disclosure_and_ready_update_adds_image() {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| shell("bookmarks-emoji"));
            slot.set(Some(state));
            rsx! { super::super::BookmarksPanel {state, channel:"C2"} }
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    let before = state.peek().core.bookmarks.items.clone();
    let pending = {
        let read = state.peek();
        custom_id(
            &read,
            read.core.active_team.as_deref().unwrap(),
            "bookmark-pending",
            "https://example.test/bookmark-pending.png",
        )
    };
    assert!(!attribute(&edits, "src", &pending.webview_uri_at(0)));
    assert!(attribute(&edits, "title", "https://example.com/launch"));
    assert!(attribute(&edits, "aria-expanded", "true"));
    assert!(edits.edits.iter().any(
        |edit| matches!(edit, Mutation::CreateTextNode {value,..} if value == "Launch guide")
    ));
    dom.in_runtime(|| {
        let mut write = state.write();
        write
            .media
            .insert(pending.clone(), "image/png", b"fixture bytes".as_slice());
        write.media_epoch += 1;
    });
    let edits = dom.render_immediate_to_vec();
    assert!(attribute(
        &edits,
        "src",
        &pending.webview_uri_at(state.peek().media_epoch)
    ));
    assert!(attribute(&edits, "alt", ""));
    assert_eq!(state.peek().core.bookmarks.items, before);
}
