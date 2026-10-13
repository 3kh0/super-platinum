use super::*;
use serde_json::json;
use super_platinum_core::state::ChannelMessages;

fn workspace(values: Vec<serde_json::Value>) -> Workspace {
    let mut core = crate::fixture::fixture_core();
    let mut workspace = core.workspaces.remove("T1").unwrap();
    workspace.messages.insert(
        "C2".into(),
        ChannelMessages {
            messages: values
                .into_iter()
                .map(|v| serde_json::from_value(v).unwrap())
                .collect(),
            ..Default::default()
        },
    );
    workspace
}

fn assert_equivalent(workspace: &Workspace, divider: Option<&str>) {
    let media = MediaRegistry::default();
    let full = crate::channel_vm::project_messages_for_channel(workspace, "C2", &media, divider);
    let rows = channel_rows(workspace, "C2", divider);
    assert_eq!(rows.len(), full.len());
    for start in 0..=rows.len() {
        for end in start..=rows.len() {
            assert_eq!(
                project_channel_window(workspace, "C2", &media, &rows, start..end),
                full[start..end],
                "window {start}..{end}, divider {divider:?}"
            );
        }
    }
}

#[test]
fn every_window_matches_full_projection_across_dates_groups_and_unread() {
    let mut workspace = workspace(vec![
        json!({"ts":"1719800400.000000","user":"U1","text":"First"}),
        json!({"ts":"1719800401.000000","user":"U1","text":"Compact"}),
        json!({"ts":"1719800402.000000","user":"U2","text":"Other author"}),
        json!({"ts":"1719800800.000000","user":"U2","text":"Time gap"}),
        json!({"ts":"1719887200.000000","user":"U2","text":"Next day"}),
        json!({"ts":"1719887201.000000","user":"U2","text":"Tail"}),
    ]);
    workspace.messages.get_mut("C2").unwrap().last_read = Some("1719800400.000000".into());
    assert_equivalent(&workspace, None);
    assert_equivalent(&workspace, Some("1719800402.000000"));
    assert_equivalent(&workspace, Some("9999999999.0"));
    let rows = channel_rows(&workspace, "C2", None);
    let tail = project_channel_window(&workspace, "C2", &MediaRegistry::default(), &rows, 5..6);
    assert!(tail[0].compact);
    assert!(tail[0].date_label.is_none());
    assert!(
        !tail[0].show_unread_divider,
        "offscreen unread must not move to the tail"
    );
}

#[test]
fn malformed_and_missing_timestamps_keep_original_identity_and_annotations() {
    let workspace = workspace(vec![
        json!({"ts":"1719800400.000000","user":"U1","text":"Valid"}),
        json!({"ts":"9223372036854775807.0","user":"U1","text":"Invalid calendar date"}),
        json!({"ts":"1719800401.000000","user":"U1","text":"Same valid date"}),
        json!({"client_msg_id":"optimistic-client","user":"U1","text":"No ts"}),
        json!({"text":"Cached fallback"}),
        json!({"ts":"","client_msg_id":"must-not-replace-empty","text":"Empty ts"}),
        json!({"ts":"not-a-timestamp","user":"U1","text":"Malformed"}),
    ]);
    assert_equivalent(&workspace, Some("0.0"));
    let rows = channel_rows(&workspace, "C2", None);
    assert_eq!(rows[3].id, "optimistic-client");
    assert_eq!(rows[4].id, "cached-Cached fallback");
    assert_eq!(rows[5].id, "");
    assert_eq!(rows[6].ts, "not-a-timestamp");
    assert!(
        rows[2].date_label.is_none(),
        "invalid dates do not reset the last valid date"
    );
}

#[test]
fn hidden_replies_do_not_change_visible_predecessor_or_source_positions() {
    let workspace = workspace(vec![
        json!({"ts":"100.0","user":"U1","text":"Root","thread_ts":"100.0"}),
        json!({"ts":"101.0","user":"U2","text":"Hidden reply","thread_ts":"100.0"}),
        json!({"ts":"102.0","user":"U2","subtype":"message_replied","text":"Hidden update"}),
        json!({"ts":"103.0","user":"U1","text":"Visible compact"}),
        json!({"ts":"104.0","user":"U1","thread_ts":"100.0","subtype":"thread_broadcast","text":"Broadcast"}),
    ]);
    assert_equivalent(&workspace, Some("100.0"));
    let rows = channel_rows(&workspace, "C2", Some("999.0"));
    assert_eq!(
        rows.iter().map(|r| r.source_index).collect::<Vec<_>>(),
        [0, 3, 4]
    );
    assert!(rows[1].compact);
}

#[test]
fn pending_state_comes_only_from_the_original_channel_and_refreshes() {
    let mut workspace = workspace(vec![json!({"ts":"100.0","user":"U1","text":"Optimistic"})]);
    workspace
        .messages
        .entry("OTHER".into())
        .or_default()
        .pending
        .push("100.0".into());
    assert!(!channel_rows(&workspace, "C2", None)[0].pending);
    workspace
        .messages
        .get_mut("C2")
        .unwrap()
        .pending
        .push("100.0".into());
    assert_equivalent(&workspace, None);
    let rows = channel_rows(&workspace, "C2", None);
    assert!(
        project_channel_window(&workspace, "C2", &MediaRegistry::default(), &rows, 0..1)[0].pending
    );
    workspace.messages.get_mut("C2").unwrap().pending.clear();
    assert!(!channel_rows(&workspace, "C2", None)[0].pending);
}

#[test]
fn metadata_and_unselected_rich_rows_do_not_register_media() {
    let workspace = workspace(vec![
        json!({"ts":"100.0","text":"Plain without avatar"}),
        json!({"ts":"101.0","blocks":[{"type":"context","elements":[{"type":"image","image_url":"https://example.test/window-only.png","alt_text":"Preview"}]}]}),
    ]);
    let media = MediaRegistry::default();
    let rows = channel_rows(&workspace, "C2", None);
    assert!(!media.has_pending());
    assert_eq!(
        project_channel_window(&workspace, "C2", &media, &rows, 0..1).len(),
        1
    );
    assert!(
        !media.has_pending(),
        "offscreen image must remain unregistered"
    );
    let shown = project_channel_window(&workspace, "C2", &media, &rows, 1..2);
    assert!(media.has_pending());
    assert_eq!(
        shown,
        project_channel_window(&workspace, "C2", &media, &rows, 1..2),
        "same media identity on re-entry"
    );
}

#[test]
fn bounds_are_clamped_and_shifted_stale_source_indices_cannot_render_another_message() {
    let mut workspace = workspace(vec![
        json!({"ts":"100.0","text":"One"}),
        json!({"ts":"101.0","text":"Two"}),
    ]);
    let media = MediaRegistry::default();
    let rows = channel_rows(&workspace, "C2", None);
    assert_eq!(
        project_channel_window(&workspace, "C2", &media, &rows, 1..usize::MAX).len(),
        1
    );
    assert!(project_channel_window(&workspace, "C2", &media, &rows, 4..5).is_empty());
    assert!(project_channel_window(&workspace, "C2", &media, &rows, 1..0).is_empty());
    workspace.messages.get_mut("C2").unwrap().messages[1].ts = Some("102.0".into());
    assert!(
        project_channel_window(&workspace, "C2", &media, &rows, 0..2).is_empty(),
        "one stale row cannot compact the window and shift absolute indices"
    );
    workspace.messages.get_mut("C2").unwrap().messages[1].ts = Some("101.0".into());
    workspace.messages.get_mut("C2").unwrap().messages.remove(0);
    assert!(project_channel_window(&workspace, "C2", &media, &rows, 0..2).is_empty());
    let current = channel_rows(&workspace, "C2", None);
    assert_eq!(
        project_channel_window(&workspace, "C2", &media, &current, 0..1)[0].id,
        "101.0"
    );
}
