use super::*;
use crate::media::MediaRegistry;
use serde_json::json;
fn entry(id: &str) -> DmEntry {
    serde_json::from_value(
        json!({"id":id,"channel":{"id":id,"is_im":true,"user":"U_PEER","unread_count":0}}),
    )
    .unwrap()
}
#[test]
fn cold_dm_unread_filter_uses_existing_bag_including_zero_before_sparse_feed_counts() {
    let mut s = ShellState::fixture(MediaRegistry::default());
    let mut e = entry("D_FACTS");
    s.set_dm_unread_only(true);
    for canonical in [false, true] {
        {
            let ws = s.core.workspaces.get_mut("T1").unwrap();
            if canonical {
                ws.channels.insert(
                    e.id.clone(),
                    Channel {
                        id: e.id.clone(),
                        ..Default::default()
                    },
                );
            } else {
                ws.channels.remove(&e.id);
            }
            let bag = ws.messages.entry(e.id.clone()).or_default();
            bag.unread_count = 3;
            bag.mention_count = 1;
        }
        e.channel.as_mut().unwrap().unread_count_display = Some(0);
        assert_eq!(entry_unread(s.core.workspaces.get("T1"), &e), 3);
        assert!(s.dm_row_visible(&e));
        {
            let bag = s
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut(&e.id)
                .unwrap();
            bag.unread_count = 1;
            bag.mention_count = 4;
        }
        assert_eq!(entry_unread(s.core.workspaces.get("T1"), &e), 4);
        assert!(s.dm_row_visible(&e));

        {
            let bag = s
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut(&e.id)
                .unwrap();
            bag.unread_count = 0;
            bag.mention_count = 0;
        }
        e.channel.as_mut().unwrap().unread_count_display = Some(9);
        assert_eq!(entry_unread(s.core.workspaces.get("T1"), &e), 0);
        assert!(!s.dm_row_visible(&e));
    }
    {
        let ws = s.core.workspaces.get_mut("T1").unwrap();
        ws.channels.remove(&e.id);
        ws.messages.remove(&e.id);
    }
    assert_eq!(entry_unread(s.core.workspaces.get("T1"), &e), 9);
    assert_eq!(entry_unread(None, &e), 9);
    e.channel.as_mut().unwrap().id = "D_OTHER".into();
    assert_eq!(entry_unread(s.core.workspaces.get("T1"), &e), 0);
    assert_eq!(entry_unread(None, &e), 0);
}
#[test]
fn canonical_group_name_drives_dm_label_and_filter_but_skeleton_keeps_matching_feed_identity() {
    let mut s = ShellState::fixture(MediaRegistry::default());
    let e:DmEntry=serde_json::from_value(json!({"id":"G_FACTS","channel":{"id":"G_FACTS","is_mpim":true,"is_group":true,"name":"Before rename"}})).unwrap();
    s.core.workspaces.get_mut("T1").unwrap().channels.insert(
        e.id.clone(),
        serde_json::from_value(
            json!({"id":"G_FACTS","is_mpim":true,"is_group":true,"name":"Release Planning"}),
        )
        .unwrap(),
    );
    assert_eq!(s.dm_row_label(&e), "Release Planning");
    s.set_dm_query("release PLANNING".into());
    assert!(s.dm_row_visible(&e));
    s.set_dm_query("Before rename".into());
    assert!(!s.dm_row_visible(&e));
    s.core.workspaces.get_mut("T1").unwrap().channels.insert(
        e.id.clone(),
        Channel {
            id: e.id.clone(),
            ..Default::default()
        },
    );
    assert_eq!(s.dm_row_label(&e), "Before rename");
    assert!(s.dm_row_visible(&e));
    let mut peer = entry("D_PEER");
    {
        let ws = s.core.workspaces.get_mut("T1").unwrap();
        ws.users.insert(
            "U_PEER".into(),
            serde_json::from_value(json!({"id":"U_PEER","profile":{"display_name":"Cached peer"}}))
                .unwrap(),
        );
        ws.channels.insert(
            peer.id.clone(),
            Channel {
                id: peer.id.clone(),
                is_im: true,
                ..Default::default()
            },
        );
    }
    assert_eq!(s.dm_row_label(&peer), "Cached peer");
    s.core.workspaces.get_mut("T1").unwrap().channels.insert(peer.id.clone(),serde_json::from_value(json!({"id":"D_PEER","is_im":true,"user":" ","name":"Looks usable but peer wins in core"})).unwrap());
    assert_eq!(
        s.dm_row_label(&peer),
        "Cached peer",
        "blank canonical peer cannot hide a useful feed peer"
    );
    let mut blank_peer = peer.clone();
    blank_peer.channel.as_mut().unwrap().user = Some(" ".into());
    blank_peer.channel.as_mut().unwrap().name =
        Some("Name cannot rescue a blank explicit IM peer".into());
    assert_eq!(s.dm_row_label(&blank_peer), "Direct message");

    peer.channel.as_mut().unwrap().id = "D_OTHER".into();
    peer.channel.as_mut().unwrap().name = Some("Wrong identity".into());
    assert_eq!(s.dm_row_label(&peer), "Direct message");
}
