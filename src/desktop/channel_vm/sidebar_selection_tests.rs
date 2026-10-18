use super::*;
use crate::state::{ChannelOpen, MainView, ShellState};
use serde_json::{Value, json};
use super_platinum_core::{ComposerState, slack::models::Channel};

const PREVIEW: &str = "C0PREVIEW";

fn ids(shell: &ShellState) -> Vec<String> {
    shell.channels.iter().map(|c| c.id.clone()).collect()
}

fn sidebar_ids(shell: &ShellState) -> Vec<String> {
    shell
        .sidebar_sections
        .iter()
        .flat_map(|section| {
            section.channel_indices.iter().map(|&index| {
                shell
                    .channels
                    .get(index)
                    .expect("section index must address current rows")
                    .id
                    .clone()
            })
        })
        .collect()
}

fn append(shell: &mut ShellState, archived: bool) -> usize {
    let team = shell.core.active_team.clone().unwrap();
    let channel = Channel {
        id: PREVIEW.into(),
        name: Some("active-preview".into()),
        is_channel: true,
        is_archived: archived,
        ..Default::default()
    };
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.channels.insert(PREVIEW.into(), channel.clone());
    let row = channel_vm(workspace, &channel, &shell.media);
    let index = shell.channels.len();
    shell.channels.push(row);
    index
}

fn raw(shell: &ShellState) -> Value {
    let workspace = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
    json!({
        "channels": workspace.channels,
        "counts": workspace.messages.iter().map(|(id, messages)| {
            (id.clone(), json!({"unread":messages.unread_count,
                "mentions":messages.mention_count,"vip":messages.vip_count,
                "lastRead":messages.last_read}))
        }).collect::<std::collections::BTreeMap<_,_>>(),
        "lifecycle":workspace.channel_lifecycle.revision(),
    })
}

#[test]
fn appended_counts_nonmember_and_archived_previews_appear_immediately_then_retire() {
    for archived in [false, true] {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let original = shell.core.active_channel.clone().unwrap();
        let original_index = shell.active_channel;
        let appended = append(&mut shell, archived);
        let team = shell.core.active_team.clone().unwrap();
        // Actual counts omission supplies nonmembership; selection must not
        // fabricate membership to make this preview visible.
        shell.core.workspaces.get_mut(&team).unwrap().apply_counts(
            serde_json::from_value(json!({"channels":[{
                "id":original,"has_unreads":false,"mention_count":0,
                "last_read":"5.0","latest":"5.0"
            }]}))
            .unwrap(),
        );
        assert_eq!(
            shell.core.workspaces[&team].channels[PREVIEW].extra["is_member"],
            false
        );
        let before = raw(&shell);
        let order = ids(&shell);
        let generation = shell.channel_generation;
        assert!(!sidebar_ids(&shell).iter().any(|id| id == PREVIEW));
        shell.select_channel(appended, ChannelOpen::Global);
        assert_eq!(
            sidebar_ids(&shell)
                .iter()
                .filter(|id| id.as_str() == PREVIEW)
                .count(),
            1
        );
        assert_eq!(shell.active_channel, appended);
        assert_eq!(shell.core.active_channel.as_deref(), Some(PREVIEW));
        assert_eq!(shell.channel_generation, generation.wrapping_add(1));
        assert_eq!(ids(&shell), order);
        assert_eq!(raw(&shell), before);
        assert!(shell.composer_unavailable().is_some());
        shell.select_channel(original_index, ChannelOpen::Global);
        assert!(!sidebar_ids(&shell).iter().any(|id| id == PREVIEW));
        assert_eq!(raw(&shell), before);
        assert_eq!(ids(&shell), order);
    }
}

#[test]
fn section_mapping_uses_existing_unsorted_indices_without_inventing_unprojected_rows() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let appended = append(&mut shell, false);
    let order = ids(&shell);
    assert!(order[..appended].iter().any(|id| id.as_str() > PREVIEW));
    let team = shell.core.active_team.as_ref().unwrap();
    let workspace = &shell.core.workspaces[team];
    let sections = project_sidebar_sections(
        workspace,
        &shell.channels,
        Some(PREVIEW),
        &mut shell.sidebar_memory,
    );
    assert_eq!(ids(&shell), order);
    let preview_indices: Vec<_> = sections
        .iter()
        .flat_map(|s| &s.channel_indices)
        .copied()
        .filter(|&i| shell.channels[i].id == PREVIEW)
        .collect();
    assert_eq!(preview_indices, [appended]);
    let without_preview = &shell.channels[..appended];
    let sections = project_sidebar_sections(
        workspace,
        without_preview,
        Some(PREVIEW),
        &mut Default::default(),
    );
    assert!(
        sections
            .iter()
            .flat_map(|s| &s.channel_indices)
            .all(|&i| i < appended)
    );
}

#[test]
fn global_preview_retains_activity_conversation_and_both_composer_drafts() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let original = shell.core.active_channel.clone().unwrap();
    let original_index = shell.active_channel;
    shell.main_view = MainView::Activity;
    assert!(shell.select_activity_item(
        "selection-proof".into(),
        Some(&original),
        Some("1.000100"),
        Some("1.000000")
    ));
    // The async Activity opener starts the thread separately after selection.
    shell.begin_thread("1.000000".into(), None);
    shell.sync_composer_drafts();
    shell.core.composer = ComposerState::with_text("Original channel draft");
    shell.core.thread_composer = ComposerState::with_text("Original thread draft");
    let appended = append(&mut shell, false);
    shell.select_channel(appended, ChannelOpen::Global);
    assert_eq!(shell.main_view, MainView::Home);
    let remembered = &shell.surfaces[&MainView::Activity];
    assert_eq!(remembered.channel, original);
    assert_eq!(remembered.thread_root.as_deref(), Some("1.000000"));
    assert!(sidebar_ids(&shell).iter().any(|id| id == PREVIEW));
    assert!(shell.thread_root.is_none());
    shell.core.composer = ComposerState::with_text("Preview channel draft");
    shell.select_channel(original_index, ChannelOpen::Global);
    assert_eq!(shell.core.composer.text, "Original channel draft");
    shell.begin_thread("1.000000".into(), None);
    assert_eq!(shell.core.thread_composer.text, "Original thread draft");
    // Opening the thread performs a full projection, which may reorder rows.
    let appended = shell
        .channels
        .iter()
        .position(|row| row.id == PREVIEW)
        .unwrap();
    shell.select_channel(appended, ChannelOpen::Global);
    assert_eq!(shell.core.composer.text, "Preview channel draft");
}

#[test]
fn in_surface_dm_selection_keeps_dm_ownership_and_prior_home_target() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let original = shell.core.active_channel.clone().unwrap();
    shell.select_channel(shell.active_channel, ChannelOpen::Global);
    let home = shell.surfaces[&MainView::Home].channel.clone();
    let dm = shell.channels.iter().position(|c| c.is_im).unwrap();
    let dm_id = shell.channels[dm].id.clone();
    shell.main_view = MainView::Dms;
    assert!(!shell.detail_open());
    shell.select_channel(dm, ChannelOpen::InSurface);
    assert_eq!(shell.main_view, MainView::Dms);
    assert!(shell.detail_open());
    assert_eq!(shell.surfaces[&MainView::Dms].channel, dm_id);
    assert_eq!(shell.surfaces[&MainView::Home].channel, home);
    assert_eq!(home, original);
    assert!(sidebar_ids(&shell).iter().any(|id| id == &dm_id));
}
