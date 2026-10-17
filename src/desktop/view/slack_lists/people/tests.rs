use super::*;
use dioxus::core::{AttributeValue, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn srcs(edits: &Mutations) -> Vec<String> {
    edits
        .edits
        .iter()
        .filter_map(|m| match m {
            Mutation::SetAttribute {
                name: "src",
                value: AttributeValue::Text(v),
                ..
            } => Some(v.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn actual_component_uses_ready_media_epoch_and_live_avatar_metadata_without_label_reprojection() {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-people-avatars",
                )
            });
            slot.set(Some(state));
            let epoch = state.read().media_epoch;
            rsx! {PeopleAvatars{state,media_epoch:epoch,people:vec![PeopleReference{user_id:"U00000002".into(),label:"@Bob Guest".into(),known:true}]}}
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    assert!(
        srcs(&initial).is_empty(),
        "pending image must show initials"
    );
    assert!(initial.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"aria-label",value:AttributeValue::Text(v),..}if v=="@Bob Guest")));
    let mut state = slot.get().unwrap();
    let rows = crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .rows;
    let asset = state
        .peek()
        .media
        .register_avatar("U00000002", "https://avatar.invalid/pending.png");
    state.peek().media.insert(
        asset,
        "image/png",
        include_bytes!("../../../../../assets/icons/icon-256.png").as_slice(),
    );
    state.write().media_epoch += 1;
    let loaded = dom.render_immediate_to_vec();
    let first = srcs(&loaded);
    assert_eq!(first.len(), 1);
    assert!(first[0].contains("?v="));
    {
        let mut s = state.write();
        let asset = s
            .media
            .register_avatar("U00000002", "https://avatar.invalid/replacement.png");
        s.media.insert(
            asset,
            "image/png",
            include_bytes!("../../../../../assets/icons/icon-256.png").as_slice(),
        );
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .get_mut("U00000002")
            .unwrap()
            .profile
            .as_mut()
            .unwrap()
            .image_48 = Some("https://avatar.invalid/replacement.png".into());
    }
    // No mark_dirty or cached-row rebuild: the shared renderer subscribes directly.
    let updated = dom.render_immediate_to_vec();
    let next = srcs(&updated);
    assert_eq!(next.len(), 1);
    assert_ne!(first, next);
    assert!(std::sync::Arc::ptr_eq(
        &rows,
        &crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .rows
    ));
}
#[test]
fn unknown_and_missing_avatar_have_content_fallback_without_image_or_fake_action() {
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-people-avatars",
    );
    for (id, label, known) in [
        ("W00000003", "@Whitney External", true),
        ("U99999999", "U99999999", false),
    ] {
        let p = PeopleReference {
            user_id: id.into(),
            label: label.into(),
            known,
        };
        assert!(ready_avatar(&s, &p, 0).is_none());
        assert!(!initial(label).is_empty());
    }
    let mut dom = VirtualDom::new(|| {
        let state = use_signal(|| {
            ShellState::fixture_variant(
                crate::media::MediaRegistry::default(),
                "slack-list-people-avatars",
            )
        });
        rsx! {PeopleAvatars{state,media_epoch:0,people:vec![PeopleReference{user_id:"U99999999".into(),label:"U99999999".into(),known:false}]}}
    });
    let edits = dom.rebuild_to_vec();
    assert!(srcs(&edits).is_empty());
    assert!(
        !edits
            .edits
            .iter()
            .any(|m| matches!(m, Mutation::NewEventListener { name, .. } if name == "click"))
    );
    assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"aria-label",value:AttributeValue::Text(v),..}if v=="U99999999")));
}
