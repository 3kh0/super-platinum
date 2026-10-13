use super::*;
use crate::{
    slack::models::{Channel, Message, UserGroup},
    state::ChannelMessages,
};
use serde_json::json;

fn workspace() -> Workspace {
    let mut ws = Workspace::from_session(&WorkspaceSession {
        team_id: "T".into(),
        enterprise_id: None,
        user_id: "U".into(),
        name: "old".into(),
        url: "https://example.slack.com".into(),
        token: String::new(),
    });
    ws.channels.insert(
        "C".into(),
        Channel {
            id: "C".into(),
            name: Some("general".into()),
            ..Default::default()
        },
    );
    ws.messages.insert(
        "C".into(),
        ChannelMessages {
            messages: vec![Message {
                ts: Some("1.000001".into()),
                text: Some("saved transcript".into()),
                ..Default::default()
            }],
            loaded: true,
            pending: vec!["1.000001".into()],
            last_read: Some("0.000001".into()),
            unread_count: 2,
            mention_count: 1,
            vip_count: 1,
            latest: Some("1.000001".into()),
            ..Default::default()
        },
    );
    ws
}

fn rows(cache: &Cache, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
    let mut stmt = cache
        .conn
        .prepare(&format!("select * from {table} order by 1, 2"))
        .unwrap();
    let columns = stmt.column_count();
    stmt.query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn metadata_persistence_matches_full_metadata_without_overwriting_transcripts_or_state() {
    let cache = Cache::open(":memory:").unwrap();
    let reference = Cache::open(":memory:").unwrap();
    let mut ws = workspace();
    cache.save_workspace(&ws).unwrap();
    let messages = rows(&cache, "messages");
    let state = rows(&cache, "channel_state");
    ws.name = "new".into();
    ws.locale = Some("fr-FR".into());
    ws.deleted_files.insert("F".into());
    ws.last_active_channel = Some("C".into());
    ws.recent_channels.push("C".into());
    ws.starred_order.push("C".into());
    ws.usergroups.insert(
        "S".into(),
        UserGroup {
            id: "S".into(),
            handle: "group".into(),
            ..Default::default()
        },
    );
    // Metadata persistence must not clear known history or replace its cursor.
    ws.messages.get_mut("C").unwrap().messages.clear();
    ws.messages.get_mut("C").unwrap().last_read = Some("99.000000".into());
    WorkspaceSnapshot::metadata(&ws).save(&cache).unwrap();
    reference.save_workspace(&ws).unwrap();
    for table in ["workspaces", "channels", "users", "usergroups"] {
        assert_eq!(rows(&cache, table), rows(&reference, table), "{table}");
    }
    assert_eq!(rows(&cache, "messages"), messages);
    assert_eq!(rows(&cache, "channel_state"), state);
    // Metadata deletion semantics (removed groups) remain exact too.
    ws.usergroups.clear();
    WorkspaceSnapshot::metadata(&ws).save(&cache).unwrap();
    assert!(rows(&cache, "usergroups").is_empty());
    assert_eq!(rows(&cache, "messages"), messages);
}

#[test]
fn newest_metadata_keeps_queued_full_transcripts_and_new_full_supersedes_everything() {
    let mut ws = workspace();
    let old_full = WorkspaceSnapshot::full(&ws);
    ws.name = "new".into();
    ws.messages.get_mut("C").unwrap().messages.clear();
    let mut newest = WorkspaceSnapshot::metadata(&ws);
    newest.retain_older_transcripts(old_full);
    assert_eq!(newest.workspace.name, "new");
    assert_eq!(newest.workspace.messages["C"].messages.len(), 1);
    let mut next_metadata = WorkspaceSnapshot::metadata(&ws);
    next_metadata.retain_older_transcripts(newest);
    assert_eq!(next_metadata.workspace.messages["C"].messages.len(), 1);
    let mut next_full = WorkspaceSnapshot::full(&ws);
    next_full.retain_older_transcripts(next_metadata);
    assert!(next_full.workspace.messages["C"].messages.is_empty());
    assert!(!next_full.metadata_only);
}

#[test]
fn metadata_snapshot_never_retains_other_team_transcripts_or_live_runtime() {
    let ws = workspace();
    let mut snapshot = WorkspaceSnapshot::metadata(&ws);
    assert!(snapshot.workspace.messages.is_empty());
    assert!(!snapshot.workspace.rt.is_connected());
    let mut other = ws.clone();
    other.team_id = "OTHER".into();
    snapshot.retain_older_transcripts(WorkspaceSnapshot::full(&other));
    assert!(snapshot.workspace.messages.is_empty());
}

#[test]
#[ignore = "paired CPU-only snapshot benchmark; no Slack or disk persistence"]
fn metadata_snapshot_clone_cost() {
    let mut ws = workspace();
    for i in 0..2000 {
        let id = format!("C{i:05}");
        ws.channels.insert(
            id.clone(),
            Channel {
                id,
                name: Some(format!("channel-{i}")),
                ..Default::default()
            },
        );
    }
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Synthetic rich message for clone-cost measurement. ".repeat(8)},
        {"type":"user","user_id":"U_FIXTURE"},{"type":"text","text":"unmodified"}]}]}),
    ];
    for channel in 0..40 {
        ws.messages.insert(
            format!("C{channel:05}"),
            ChannelMessages {
                loaded: true,
                messages: (0..200)
                    .map(|i| Message {
                        ts: Some(format!("{i}.000001")),
                        blocks: blocks.clone(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        );
    }
    let mut full = Vec::new();
    let mut metadata = Vec::new();
    // Interleave order to reduce warmup/thermal ordering bias. Time capture,
    // excluding drop and disk I/O, because capture is the main-thread work.
    for round in 0..45 {
        for narrow in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = std::time::Instant::now();
            let snapshot = if narrow {
                WorkspaceSnapshot::metadata(&ws)
            } else {
                WorkspaceSnapshot::full(&ws)
            };
            std::hint::black_box(&snapshot);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            if round >= 5 {
                if narrow { &mut metadata } else { &mut full }.push(ms);
            }
            drop(snapshot);
        }
    }
    for (label, mut samples) in [("full", full), ("metadata", metadata)] {
        samples.sort_by(f64::total_cmp);
        println!(
            "workspace_snapshot mode={label} channels={} messages=8001 samples={} median_ms={:.4} p95_ms={:.4}",
            ws.channels.len(),
            samples.len(),
            (samples[19] + samples[20]) / 2.0,
            samples[37]
        );
    }
}
