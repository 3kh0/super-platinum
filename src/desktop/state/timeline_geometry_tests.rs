//! Integration oracle for cached geometry across real shell mutation paths.
use super::{ChannelOpen, ShellState};
use crate::media::MediaRegistry;
use super_platinum_core::{domain::PendingScrollTarget, slack::models::Message};

fn message(index: usize) -> Message {
    serde_json::from_value(
        serde_json::json!({"ts":format!("{}.000001",1_719_900_000 + index),
        "user":"U1","text":"Synthetic geometry row"}),
    )
    .unwrap()
}

fn shell(count: usize) -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let channel = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .unwrap();
    shell.select_channel(channel, ChannelOpen::Global);
    replace_rows(&mut shell, (0..count).collect());
    shell.core.pending_scroll_to = None;
    shell
}

fn replace_rows(shell: &mut ShellState, ids: Vec<usize>) {
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages = ids.into_iter().map(message).collect();
    shell.refresh_from_core();
}

fn heights(shell: &ShellState) -> Vec<f64> {
    shell
        .channel_rows
        .iter()
        .map(|row| shell.estimated_row_height(&row.id))
        .collect()
}

// Preserve the former production semantics independently of the new prefix
// representation, including the asymmetric > top / >= bottom boundaries.
fn linear_window(heights: &[f64], top: f64, height: f64) -> (usize, usize) {
    let top = if top.is_finite() { top.max(0.0) } else { 0.0 };
    let bottom = top
        + if height.is_finite() {
            height.max(0.0)
        } else {
            0.0
        };
    let last = heights.len().saturating_sub(1);
    let mut first = None;
    let mut offset = 0.0;
    for (index, height) in heights.iter().enumerate() {
        offset += height;
        if first.is_none() && offset > top {
            first = Some(index);
        }
        if offset >= bottom {
            return (first.unwrap_or(index), index);
        }
    }
    (first.unwrap_or(last), last)
}

fn assert_geometry(shell: &ShellState) {
    let heights = heights(shell);
    let count = heights.len();
    for (start, end) in [
        (0, 0),
        (0, count),
        (count, count),
        (count / 3, count * 2 / 3),
    ] {
        assert_eq!(
            shell.channel_geometry.spacer_heights(start, end),
            (heights[..start].iter().sum(), heights[end..].iter().sum()),
            "spacers for {start}..{end}/{count}"
        );
    }
    let mut tops = vec![
        -1.0,
        0.0,
        f64::NAN,
        f64::INFINITY,
        heights.iter().sum::<f64>() + 100.0,
    ];
    let mut offset = 0.0;
    for height in &heights {
        offset += height;
        tops.extend([offset - 0.25, offset, offset + 0.25]);
    }
    for top in tops {
        for viewport in [0.0, 1.0, 132.0, 700.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                shell.estimated_timeline_window(top, viewport),
                linear_window(&heights, top, viewport),
                "viewport top={top:?}, height={viewport:?}, rows={count}"
            );
        }
    }
}

fn measurements(shell: &ShellState) -> Vec<(String, f64)> {
    shell
        .channel_rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id.clone(), 24.25 + (index % 17) as f64 * 9.5))
        .collect()
}

#[test]
fn canonical_replacement_prepend_delete_and_equal_length_reorder_refresh_geometry() {
    let mut shell = shell(40);
    let measured = measurements(&shell);
    shell.set_timeline_window(14, 20, measured, false);
    assert_geometry(&shell);
    for ids in [
        (0..40).rev().collect(),
        (100..105).chain(0..40).collect(),
        (100..105)
            .chain((0..40).filter(|id| ![3, 17, 31].contains(id)))
            .collect(),
        (0..40).rev().collect(),
        (40..80).collect(),
        vec![],
    ] {
        replace_rows(&mut shell, ids);
        assert_geometry(&shell);
    }
}

#[test]
fn synthetic_fixture_replacement_rebuilds_same_length_and_empty_geometry() {
    let mut shell = shell(10);
    let measured = measurements(&shell);
    shell.set_timeline_window(0, 8, measured, false);
    let mut rows = shell.messages.clone();
    rows.reverse();
    shell.install_fixture_messages(rows);
    assert_geometry(&shell);
    shell.install_fixture_messages(vec![]);
    assert_geometry(&shell);
}

#[test]
fn channel_switch_clears_measurements_even_when_message_ids_match() {
    let mut shell = shell(20);
    let measured = measurements(&shell);
    shell.set_timeline_window(0, 15, measured, false);
    assert_geometry(&shell);
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default()
        .messages = (0..20).map(message).collect();
    let other = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C1")
        .unwrap();
    shell.select_channel(other, ChannelOpen::Global);
    assert!(shell.row_heights.is_empty());
    assert_eq!(heights(&shell), vec![66.0; 20]);
    assert_geometry(&shell);
    let back = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .unwrap();
    shell.select_channel(back, ChannelOpen::Global);
    assert_eq!(heights(&shell), vec![66.0; 20]);
    assert_geometry(&shell);
}

#[test]
fn accepted_changed_heights_update_geometry_and_unchanged_receipts_are_noops() {
    let mut shell = shell(40);
    let id = shell.channel_rows[3].id.clone();
    shell.set_timeline_window(12, 20, [(id.clone(), 500.25)], false);
    assert_geometry(&shell);
    assert!(!shell.timeline_window_changed(12, 20, &[(id.clone(), 500.25)], false));
    assert!(shell.timeline_window_changed(12, 20, &[(id.clone(), 21.5)], false));
    shell.set_timeline_window(12, 20, [(id.clone(), 21.5)], false);
    assert_geometry(&shell);
    // Invalid browser geometry is never allowed to replace accepted heights.
    for height in [f64::NAN, f64::INFINITY, -3.0, 0.0] {
        shell.set_timeline_window(12, 20, [(id.clone(), height)], false);
        assert_eq!(shell.estimated_row_height(&id), 21.5);
        assert_geometry(&shell);
    }
}

#[test]
fn selection_pin_keeps_rendered_span_but_still_accepts_new_geometry() {
    let mut shell = shell(160);
    shell.set_timeline_window(60, 68, [], false);
    let span = (shell.timeline_start, shell.timeline_end);
    let projected = shell
        .messages
        .iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    shell.selection_pinned = true;
    let id = shell.channel_rows[60].id.clone();
    shell.set_timeline_window(0, 1, [(id.clone(), 700.5)], false);
    assert_eq!((shell.timeline_start, shell.timeline_end), span);
    assert_eq!(
        shell
            .messages
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        projected
    );
    assert_eq!(shell.estimated_row_height(&id), 700.5);
    assert_geometry(&shell);
}

#[test]
fn pending_navigation_early_returns_keep_accepted_height_cache_current() {
    for latest in [false, true] {
        let mut shell = shell(160);
        let target = shell.channel_rows[40].ts.clone();
        shell.core.pending_scroll_to = Some((
            "C2".into(),
            if latest {
                PendingScrollTarget::Latest
            } else {
                PendingScrollTarget::Message(target.clone())
            },
        ));
        let id = shell.channel_rows[20].id.clone();
        shell.set_timeline_window(0, 1, [(id.clone(), 950.25)], false);
        assert!(shell.core.pending_scroll_to.is_some());
        assert_eq!(shell.estimated_row_height(&id), 950.25);
        if latest {
            assert_eq!(shell.timeline_end, 160);
        } else {
            assert!(shell.messages.iter().any(|row| row.ts == target));
        }
        assert_geometry(&shell);
    }
}

#[test]
fn height_eviction_restores_default_geometry_outside_retained_neighborhood() {
    let mut shell = shell(1200);
    let measured = measurements(&shell);
    let forgotten = shell.channel_rows[0].id.clone();
    let retained = shell.channel_rows[600].id.clone();
    let newest = shell.channel_rows[1199].id.clone();
    shell.set_timeline_window(600, 605, measured, false);
    assert!(shell.row_heights.len() <= 600);
    assert!(!shell.row_heights.contains_key(&forgotten));
    assert!(shell.row_heights.contains_key(&retained));
    assert!(shell.row_heights.contains_key(&newest));
    assert_eq!(shell.estimated_row_height(&forgotten), 66.0);
    assert_geometry(&shell);
}

#[cfg(test)]
mod benchmark;
