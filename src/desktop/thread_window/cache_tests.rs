use super::*;
use super_platinum_core::slack::models::Message;
fn bag(count: usize) -> ChannelMessages {
    ChannelMessages {
        messages: (0..count)
            .map(|i| Message {
                ts: Some(format!("{}.000000", 100 + i)),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
fn assert_pieces(window: &ThreadWindow, root: &str) {
    let selected = (0..window.rows.len())
        .filter(|&i| window.selected(i, root))
        .collect::<Vec<_>>();
    assert_eq!(window.selected_indices(), selected);
    for history in [false, true] {
        let mut pieces = Vec::new();
        let mut omitted = 0.0;
        let mut spacer_start = 0;
        for (index, row) in window
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.history == history)
        {
            if let Ok(projected) = selected.binary_search(&index) {
                if omitted > 0.0 {
                    pieces.push(PaintPiece::Spacer {
                        index: spacer_start,
                        height: omitted,
                    });
                    omitted = 0.0;
                }
                pieces.push(PaintPiece::Row { index, projected });
            } else {
                if omitted == 0.0 {
                    spacer_start = index;
                }
                omitted += window.height(&row.ts);
            }
        }
        if omitted > 0.0 {
            pieces.push(PaintPiece::Spacer {
                index: spacer_start,
                height: omitted,
            });
        }
        assert_eq!(window.pieces(history), pieces);
    }
}
#[test]
fn cached_pieces_match_linear_after_windows_heights_eviction_and_scope_changes() {
    for count in [0, 1, 200, 2000] {
        let mut bag = bag(count);
        if count > 100 {
            bag.pending = vec!["150.000000".into(), format!("{}.000000", count + 99)];
        }
        let history = ThreadHistory::default();
        let mut window = ThreadWindow::default();
        window.sync("one", &bag, &history, "100.000000", None, true);
        assert_pieces(&window, "100.000000");
        let heights = window
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.ts.clone(), 30.25 + (i % 7) as f64 * 25.5))
            .collect::<Vec<_>>();
        for first in [0, count / 2, count.saturating_sub(1)] {
            window.measure(first, first + 8, &heights, false);
            window.align_viewport(None, false);
            assert_pieces(&window, "100.000000");
            assert!(window.heights.len() <= 600);
        }
        window.selection_pinned = true;
        let selected = window.selected_indices().to_vec();
        window.measure(0, 5, &[], false);
        assert_eq!(window.selected_indices(), selected);
        window.sync("two", &bag, &history, "100.000000", None, true);
        assert!(window.heights.is_empty());
        assert_pieces(&window, "100.000000");
    }
}
#[test]
fn filtered_history_pending_and_prepend_delete_rebuild_source_and_piece_identity() {
    let mut bag = bag(400);
    let root = "100.000000";
    let mut history = ThreadHistory::default();
    history.watch_live(450);
    history.start(&ThreadHistory::initial(Some("250.000000"),450),root,&serde_json::from_value(serde_json::json!({"messages":[{"ts":root},{"ts":"250.000000"},{"ts":"270.000000"}],"has_more":true})).unwrap());
    bag.pending.push("200.000000".into());
    let mut window = ThreadWindow::default();
    window.sync("one", &bag, &history, root, None, true);
    assert_pieces(&window, root);
    assert!(
        window
            .rows
            .iter()
            .any(|row| row.ts == "200.000000" && row.pending)
    );
    assert!(!window.rows.iter().any(|row| row.ts == "201.000000"));
    bag.messages.insert(
        0,
        Message {
            ts: Some("90.000000".into()),
            ..Default::default()
        },
    );
    assert!(!window.source_shape_matches(&bag));
    window.sync("one", &bag, &history, root, None, false);
    for row in &window.rows {
        assert_eq!(
            bag.messages[row.source_index].ts.as_deref(),
            Some(row.ts.as_str())
        );
    }
    assert_pieces(&window, root);
    bag.messages
        .retain(|message| message.ts.as_deref() != Some("250.000000"));
    bag.pending.clear();
    window.sync("one", &bag, &history, root, None, false);
    assert_pieces(&window, root);
    assert!(
        !window
            .rows
            .iter()
            .any(|row| row.ts == "200.000000" || row.ts == "250.000000")
    );
}
#[test]
fn viewport_alignment_matches_full_sync_without_changing_source_rows() {
    let bag = bag(2000);
    let history = ThreadHistory::default();
    let root = "100.000000";
    for target in [None, Some("150.000000")] {
        for bottom in [false, true] {
            for first in [0, 995, 1990] {
                let mut indexed = ThreadWindow::default();
                let mut reference = ThreadWindow::default();
                for window in [&mut indexed, &mut reference] {
                    window.sync("one", &bag, &history, root, target, bottom);
                    window.measure(first, first + 8, &[], target.is_some());
                }
                let row_pointer = indexed.rows.as_ptr();
                indexed.align_viewport(target, bottom);
                reference.sync("one", &bag, &history, root, target, bottom);
                assert_eq!(indexed.rows.as_ptr(), row_pointer);
                assert_eq!(
                    (indexed.start, indexed.end),
                    (reference.start, reference.end)
                );
                assert_eq!(indexed.selected_indices(), reference.selected_indices());
                assert_pieces(&indexed, root);
            }
        }
    }
}

#[test]
fn canonical_refresh_retires_receipt_even_when_row_ids_and_input_are_unchanged() {
    let mut bag = bag(200);
    let history = ThreadHistory::default();
    let mut window = ThreadWindow::default();
    window.sync("visit", &bag, &history, "100.000000", None, true);
    let sampled = window.projection_generation;
    let sampled_dom = sampled.to_string();
    let input = window.input_generation;
    assert!(window.projection_receipt_is_current(sampled, Some(&sampled_dom)));
    let identities = window
        .rows
        .iter()
        .map(|row| row.ts.clone())
        .collect::<Vec<_>>();
    bag.messages[199].text = Some("Edited reply\nwith another line".into());
    window.sync("visit", &bag, &history, "100.000000", None, true);
    assert_eq!(
        identities,
        window
            .rows
            .iter()
            .map(|row| row.ts.clone())
            .collect::<Vec<_>>()
    );
    assert!(window.measurement_is_current("visit", input));
    assert!(!window.projection_receipt_is_current(sampled, Some(&sampled_dom)));
    let fresh = window.projection_generation;
    // A new Rust request can still encounter the old DOM before reconciliation.
    assert!(!window.projection_receipt_is_current(fresh, Some(&sampled_dom)));
    assert!(!window.projection_receipt_is_current(fresh, None));
    assert!(!window.projection_receipt_is_current(fresh, Some("invalid")));
    let committed = fresh.to_string();
    assert!(window.projection_receipt_is_current(fresh, Some(&committed)));
    window.measure(160, 170, &[("299.000000".into(), 120.0)], false);
    window.align_viewport(None, false);
    assert!(
        window.projection_receipt_is_current(fresh, Some(&committed)),
        "viewport-only updates must not continually retire their own measurements"
    );
    window.sync("other visit", &bag, &history, "100.000000", None, true);
    assert!(!window.projection_receipt_is_current(fresh, Some(&committed)));
}
