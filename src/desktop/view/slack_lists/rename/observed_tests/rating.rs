use super::*;
use super_platinum_core::slack_lists::RatingOptions;
#[test]
fn actual_panel_exposes_all_rating_quantities_and_locks_retained_emoji_after_admission() {
    for max in 1..=5 {
        for locked in [false, true] {
            let output = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(output, max, locked): (Rc<RefCell<Option<Node>>>, u8, bool)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list-rating-field-create",
                        )
                    });
                    let mut p = rename::presentation(&state.peek()).unwrap();
                    p.rating_draft =
                        Some(RatingOptions::capture(":heart:", max, Some("T_FIXTURE")).unwrap());
                    if locked {
                        p.stage = Stage::Uncertain;
                        p.can_confirm = false;
                    }
                    let rendered = render_panel(state, p);
                    *output.borrow_mut() = Some(Node {
                        children: expand(rendered.as_ref().unwrap()),
                        ..Default::default()
                    });
                    assert!(rename::text_create::rating::close_fixture(&state.peek()));
                    rendered
                },
                (output.clone(), max, locked),
            );
            dom.rebuild_to_vec();
            let tree = output.borrow_mut().take().unwrap();
            let quantity = tree.all("id", "slack-list-new-rating-max")[0];
            assert_eq!(quantity.attr("value"), max.to_string());
            assert_eq!(quantity.attr("disabled"), locked.to_string());
            assert_eq!(quantity.children.len(), 5);
            for (index, option) in quantity.children.iter().enumerate() {
                assert_eq!(option.attr("value"), (index + 1).to_string());
                assert_eq!(option.text(), (index + 1).to_string());
            }
            let emoji = tree.all("id", "slack-list-new-rating-emoji")[0];
            assert_eq!(emoji.attr("disabled"), locked.to_string());
            assert!(emoji.text().contains(":heart:"));
            assert_eq!(
                tree.all("id", "slack-list-new-field-kind")[0]
                    .children
                    .len(),
                10
            );
        }
    }
}
#[test]
fn actual_readonly_table_slots_and_board_detail_selected_marks_remain_distinct() {
    for table in [false, true] {
        for value in [0, 1, 5] {
            let output = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(output, table, value): (Rc<RefCell<Option<Node>>>, bool, u8)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list",
                        )
                    });
                    let glyph = crate::model::RichNode::Emoji {
                        name: "heart".into(),
                        glyph: "♥".into(),
                    };
                    let rendered = crate::view::slack_lists::rating::display(
                        state, value, 5, &glyph, table, 0,
                    );
                    *output.borrow_mut() = Some(Node {
                        children: expand(rendered.as_ref().unwrap()),
                        ..Default::default()
                    });
                    rendered
                },
                (output.clone(), table, value),
            );
            dom.rebuild_to_vec();
            let tree = output.borrow_mut().take().unwrap();
            assert_eq!(
                tree.all("class", "slack-list-rating-mark").len(),
                if table { 5 } else { usize::from(value) }
            );
            assert_eq!(tree.text().matches('♥').count(), usize::from(value));
            assert!(tree.all("data-rating-choice", "1").is_empty());
            if value == 0 && !table {
                assert!(tree.all("role", "img").is_empty());
            }
        }
    }
}

#[test]
fn actual_unavailable_emoji_renderers_keep_one_readable_fallback_and_bounded_digit_buttons() {
    for pending in [false, true] {
        for table in [false, true] {
            let output = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(output, pending, table): (Rc<RefCell<Option<Node>>>, bool, bool)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list",
                        )
                    });
                    let selected = use_signal(|| Some(3));
                    let name = format!("quality_{}", "long".repeat(45));
                    let code = format!(":{name}:");
                    let glyph = if pending {
                        let id = state
                            .peek()
                            .media
                            .register_emoji(&name, "https://fixture.invalid/rating.png");
                        assert!(!state.peek().media.is_ready(&id));
                        crate::model::RichNode::EmojiImage {
                            id,
                            name: name.clone(),
                        }
                    } else {
                        crate::slack_lists::rating::glyph(
                            None,
                            &state.peek().media,
                            &code,
                            Some("OTHER"),
                        )
                    };
                    let display =
                        crate::view::slack_lists::rating::display(state, 3, 5, &glyph, table, 0);
                    let options = RatingOptions::capture(&code, 5, Some("OTHER")).unwrap();
                    let fields =
                        crate::view::slack_lists::rating::fields(state, &options, selected, false);
                    let rendered = rsx! {{display}{fields}};
                    *output.borrow_mut() = Some(Node {
                        children: expand(rendered.as_ref().unwrap()),
                        ..Default::default()
                    });
                    rendered
                },
                (output.clone(), pending, table),
            );
            dom.rebuild_to_vec();
            let tree = output.borrow_mut().take().unwrap();
            let fallbacks = tree.all("class", "slack-list-rating-fallback");
            assert_eq!(fallbacks.len(), 2);
            let name = format!("quality_{}", "long".repeat(45));
            let code = format!(":{name}:");
            assert_eq!(fallbacks[0].text(), format!("3 of 5 {code}"));
            assert_eq!(fallbacks[1].text(), code);
            assert!(tree.all("class", "slack-list-rating-mark").is_empty());
            let buttons = tree.all("class", "slack-list-rating-choice");
            assert_eq!(buttons.len(), 5);
            for (i, b) in buttons.iter().enumerate() {
                assert_eq!(b.text(), (i + 1).to_string());
            }
        }
    }
}
