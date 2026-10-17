use super::*;
#[test]
fn actual_vote_panel_has_emoji_without_quantity_and_locked_choice_is_retained() {
    for locked in [false, true] {
        let output = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |(output, locked): (Rc<RefCell<Option<Node>>>, bool)| {
                let state = use_signal(|| {
                    ShellState::fixture_variant(
                        crate::media::MediaRegistry::default(),
                        "slack-list-vote-field-create",
                    )
                });
                let mut p = rename::presentation(&state.peek()).unwrap();
                if locked {
                    p.stage = Stage::Uncertain;
                    p.can_confirm = false;
                }
                let rendered = render_panel(state, p);
                *output.borrow_mut() = Some(Node {
                    children: expand(rendered.as_ref().unwrap()),
                    ..Default::default()
                });
                assert!(rename::text_create::rating::close_vote_fixture(
                    &state.peek()
                ));
                rendered
            },
            (output.clone(), locked),
        );
        dom.rebuild_to_vec();
        let tree = output.borrow_mut().take().unwrap();
        assert!(tree.all("id", "slack-list-new-rating-max").is_empty());
        let emoji = tree.all("id", "slack-list-new-rating-emoji")[0];
        assert_eq!(emoji.attr("disabled"), locked.to_string());
        assert!(emoji.text().contains(":sparkling_heart:"));
        let kinds = tree.all("id", "slack-list-new-field-kind")[0];
        assert_eq!(kinds.children.len(), 10);
        assert_eq!(kinds.attr("value"), "vote");
    }
}
#[test]
fn actual_vote_content_exposes_owned_action_count_and_readonly_board_without_action() {
    for readonly in [false, true] {
        for voted in [false, true] {
            for disabled in [false, true] {
                let output = Rc::new(RefCell::new(None));
                let mut dom = VirtualDom::new_with_props(
                    |(output, readonly, voted, disabled): (
                        Rc<RefCell<Option<Node>>>,
                        bool,
                        bool,
                        bool,
                    )| {
                        let state = use_signal(|| {
                            ShellState::fixture_variant(
                                crate::media::MediaRegistry::default(),
                                "slack-list",
                            )
                        });
                        let glyph = crate::slack_lists::rating::glyph(
                            None,
                            &state.peek().media,
                            ":sparkling_heart:",
                            None,
                        );
                        let rendered = crate::view::slack_lists::vote::control(
                            state,
                            2,
                            voted,
                            "You, other",
                            &glyph,
                            0,
                            readonly,
                            disabled,
                            false,
                            || {},
                        );
                        *output.borrow_mut() = Some(Node {
                            children: expand(rendered.as_ref().unwrap()),
                            ..Default::default()
                        });
                        rendered
                    },
                    (output.clone(), readonly, voted, disabled),
                );
                dom.rebuild_to_vec();
                let tree = output.borrow_mut().take().unwrap();
                if readonly {
                    assert_eq!(tree.all("role", "img").len(), 1);
                    assert!(
                        tree.all("class", "slack-list-vote slack-list-vote-action")
                            .is_empty()
                    );
                } else {
                    let action = tree.all("class", "slack-list-vote slack-list-vote-action")[0];
                    assert_eq!(action.attr("aria-pressed"), voted.to_string());
                    assert_eq!(action.attr("disabled"), disabled.to_string());
                    assert_eq!(action.attr("tabindex"), "-1");
                    assert!(action.attr("aria-label").contains(if voted {
                        "Remove your vote"
                    } else {
                        "Add your vote"
                    }));
                }
                assert!(tree.text().ends_with('2'));
            }
        }
    }
}
#[test]
fn actual_vote_type_and_name_callbacks_keep_same_render_input_and_reset_after_type_aba() {
    use dioxus::core::{ElementId, Mutation, ScopeId};
    use std::cell::Cell;
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-vote-field-create",
                )
            });
            slot.set(Some(state));
            panel(state)
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let find = |name| {
        initial
            .edits
            .iter()
            .filter_map(|e| match e {
                Mutation::NewEventListener { name: n, id } if *n == name => Some(*id),
                _ => None,
            })
            .collect::<Vec<ElementId>>()
    };
    let changes = find("change");
    assert_eq!(changes.len(), 1);
    let inputs = find("input");
    assert_eq!(inputs.len(), 1);
    let event = |dom: &VirtualDom, id, name, value: &str| {
        let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            value.into(),
            Vec::new(),
        ));
        dom.runtime()
            .handle_event(name, dioxus::core::Event::new(payload.into_any(), true), id);
    };
    event(&dom, inputs[0], "input", "S");
    event(&dom, inputs[0], "input", "Support");
    assert_eq!(rename::presentation(&state.peek()).unwrap().name, "Support");
    let first = rename::presentation(&state.peek())
        .unwrap()
        .field_choice
        .unwrap();
    event(&dom, changes[0], "change", "text");
    event(&dom, inputs[0], "input", "stale");
    assert_eq!(rename::presentation(&state.peek()).unwrap().name, "Support");
    dom.mark_dirty(ScopeId::ROOT);
    dom.render_immediate_to_vec();
    event(&dom, changes[0], "change", "vote");
    let current = rename::presentation(&state.peek()).unwrap();
    assert_eq!(current.field_choice.unwrap().kind, ColumnCreateKind::Vote);
    assert_ne!(current.field_choice.unwrap().revision, first.revision);
    assert_eq!(current.rating_draft.unwrap().emoji(), ":sparkling_heart:");
    dom.in_runtime(|| {
        assert!(rename::text_create::rating::close_vote_fixture(
            &state.peek()
        ))
    });
}

#[test]
fn unknown_and_pending_vote_glyphs_have_full_visible_and_accessible_fallbacks() {
    for pending in [false, true] {
        for (readonly, detail) in [(false, false), (false, true), (true, false)] {
            let output = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(output, pending, readonly, detail): (
                    Rc<RefCell<Option<Node>>>,
                    bool,
                    bool,
                    bool,
                )| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list",
                        )
                    });
                    let name = format!("support_{}", "long".repeat(45));
                    let code = format!(":{name}:");
                    let glyph = if pending {
                        let id = state
                            .peek()
                            .media
                            .register_emoji(&name, "https://fixture.invalid/vote.png");
                        assert!(!state.peek().media.is_ready(&id));
                        crate::model::RichNode::EmojiImage { id, name }
                    } else {
                        crate::slack_lists::rating::glyph(
                            None,
                            &state.peek().media,
                            &code,
                            Some("OTHER"),
                        )
                    };
                    let rendered = crate::view::slack_lists::vote::control(
                        state,
                        2,
                        true,
                        "You, other",
                        &glyph,
                        0,
                        readonly,
                        false,
                        detail,
                        || {},
                    );
                    *output.borrow_mut() = Some(Node {
                        children: expand(rendered.as_ref().unwrap()),
                        ..Default::default()
                    });
                    rendered
                },
                (output.clone(), pending, readonly, detail),
            );
            dom.rebuild_to_vec();
            let tree = output.borrow_mut().take().unwrap();
            let code = format!(":support_{}:", "long".repeat(45));
            let fallback = tree.all("class", "slack-list-vote-fallback");
            assert_eq!(fallback.len(), 1);
            assert_eq!(fallback[0].text(), code);
            let surface = if readonly {
                tree.all("role", "img")[0]
            } else {
                tree.all("class", "slack-list-vote slack-list-vote-action")[0]
            };
            assert!(surface.attr("aria-label").contains(&code));
            assert!(surface.attr("title").contains(&code));
            assert!(surface.attr("title").contains("You, other"));
            assert_eq!(surface.text(), "2");
            assert_eq!(tree.all("class", "slack-list-vote-mark")[0].text(), "");
            if !readonly {
                assert_eq!(surface.attr("tabindex"), if detail { "0" } else { "-1" });
                assert_eq!(surface.attr("aria-pressed"), "true");
            }
        }
    }
}
