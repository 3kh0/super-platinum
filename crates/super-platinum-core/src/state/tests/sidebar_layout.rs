//! The sidebar layout against shapes captured from the real client
//! (`client.userBoot` prefs, `users.channelSections.list`, `client.counts`).

use super::super::*;
use serde_json::json;

fn workspace() -> Workspace {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://t".into(),
        token: "xoxc".into(),
    });
    ws.priority_sidebar_section = true;
    ws.vip_users.insert("U_VIP".into());
    ws.apply_channel_sections(
        serde_json::from_value(json!({
            "channel_sections": [
                {"channel_section_id":"L_CODE","type":"coding_channels","next_channel_section_id":"L_DMS"},
                {"channel_section_id":"L_DMS","name":"Direct Messages","type":"direct_messages","next_channel_section_id":"L_CONNECT"},
                {"channel_section_id":"L_CONNECT","name":"Slack Connect","type":"slack_connect","next_channel_section_id":"L_STARS"},
                {"channel_section_id":"L_STARS","type":"stars","next_channel_section_id":"L_UG",
                 "channel_ids_page":{"channel_ids":["C_STAR","C_STAR_MUTED","C_BOTH"]}},
                {"channel_section_id":"L_UG","name":"helpers","type":"user_group","next_channel_section_id":"L_APPS"},
                {"channel_section_id":"L_APPS","name":"Recent Apps","type":"recent_apps","next_channel_section_id":"L_CHANNELS"},
                {"channel_section_id":"L_CHANNELS","name":"Channels","type":"channels","next_channel_section_id":"L_AGENTS"},
                {"channel_section_id":"L_AGENTS","name":"Agents","type":"agents","next_channel_section_id":null}
            ]
        }))
        .unwrap(),
    );
    ws.sidebar.apply_prefs(
        &serde_json::from_value(json!({
            "sidebar_behavior": "hide_read_channels_unless_starred",
            "channel_sort": "default",
            "boost_mentions": true,
            "channel_sections": r#"{"L_CONNECT":{"c":"0","sidebar":"all"},"L_DMS":{"sidebar":"hid","c":"0","sort":"recent"},"L_APPS":{"c":"0","sidebar":"hid"},"L_CHANNELS":{"sidebar":"hid","c":"0"},"priority":{"sort":"recent"}}"#,
            "all_notifications_prefs": r#"{"channels":{"C_STAR_MUTED":{"muted":true},"C_MUTED_PING":{"muted":true}}}"#,
        }))
        .unwrap(),
    );
    ws
}

fn add(ws: &mut Workspace, channel: Channel) {
    ws.channels.insert(channel.id.clone(), channel);
}

fn channel(id: &str, name: &str) -> Channel {
    Channel {
        id: id.into(),
        name: Some(name.into()),
        is_channel: true,
        ..Default::default()
    }
}

fn dm(id: &str, user: &str) -> Channel {
    let mut c = Channel {
        id: id.into(),
        is_im: true,
        user: Some(user.into()),
        ..Default::default()
    };
    c.extra.insert("is_open".into(), json!(true));
    c
}

fn unread(ws: &mut Workspace, id: &str, unread: u32, mentions: u32) {
    let cm = ws.messages.entry(id.into()).or_default();
    cm.unread_count = unread;
    cm.mention_count = mentions;
}

fn layout(ws: &Workspace, active: Option<&str>) -> Vec<SidebarSection> {
    grouped_sidebar_sections(ws, active, &mut SidebarMemory::default())
}

fn rows<'a>(sections: &'a [SidebarSection], title: &str) -> Vec<&'a str> {
    sections
        .iter()
        .find(|s| s.title == title)
        .map(|s| s.channel_ids.iter().map(String::as_str).collect())
        .unwrap_or_default()
}

fn titles(sections: &[SidebarSection]) -> Vec<&str> {
    sections.iter().map(|s| s.title.as_str()).collect()
}

#[test]
fn vip_unreads_holds_vip_dms_and_vip_mentions_not_channels_vips_posted_in() {
    let mut ws = workspace();
    add(&mut ws, dm("D_VIP", "U_VIP"));
    add(&mut ws, dm("D_VIP_READ", "U_VIP"));
    add(&mut ws, dm("D_OTHER", "U_OTHER"));
    add(&mut ws, channel("C_POSTED", "goslings"));
    add(&mut ws, channel("C_MENTIONED", "hq"));
    let mut group = Channel {
        id: "G_MPIM".into(),
        name: Some("mpdm-a--b-1".into()),
        is_mpim: true,
        ..Default::default()
    };
    group
        .extra
        .insert("members".into(), json!(["U_SELF", "U_VIP"]));
    add(&mut ws, group);
    unread(&mut ws, "D_VIP", 2, 0);
    unread(&mut ws, "D_OTHER", 1, 0);
    unread(&mut ws, "G_MPIM", 1, 0);
    // A VIP's message in a channel, without mentioning anyone.
    unread(&mut ws, "C_POSTED", 3, 0);
    ws.messages
        .get_mut("C_POSTED")
        .unwrap()
        .upsert(SlackMessage {
            ts: Some("200.000000".into()),
            user: Some("U_VIP".into()),
            ..Default::default()
        });
    unread(&mut ws, "C_MENTIONED", 1, 1);
    ws.messages.get_mut("C_MENTIONED").unwrap().vip_count = 1;

    let sections = layout(&ws, None);

    let mut vip = rows(&sections, "VIP unreads");
    vip.sort();
    assert_eq!(vip, ["C_MENTIONED", "D_VIP", "G_MPIM"]);
    assert_eq!(rows(&sections, "Direct messages"), ["D_OTHER"]);
    assert_eq!(rows(&sections, "Channels"), ["C_POSTED"]);
}

#[test]
fn vip_count_is_read_from_client_counts_and_cleared_when_absent() {
    let mut ws = workspace();
    add(&mut ws, channel("C1", "one"));
    let counts = |vip: Option<u32>| {
        let mut c = json!({"id":"C1","mention_count":1,"has_unreads":true,"latest":"300.000100"});
        if let Some(vip) = vip {
            c["vip_count"] = json!(vip);
        }
        serde_json::from_value::<crate::slack::models::CountsPage>(json!({"channels":[c]})).unwrap()
    };
    ws.apply_counts(counts(Some(2)));
    assert_eq!(ws.messages["C1"].vip_count, 2);
    assert_eq!(ws.messages["C1"].latest.as_deref(), Some("300.000100"));
    ws.apply_counts(counts(None));
    assert_eq!(ws.messages["C1"].vip_count, 0);
}

#[test]
fn sections_keep_slack_order_and_headings_even_when_empty() {
    let mut ws = workspace();
    add(&mut ws, channel("C_STAR", "announcements"));
    let mut shared = channel("C_EXT", "vercel-embassy");
    shared.is_ext_shared = true;
    add(&mut ws, shared);

    let sections = layout(&ws, None);

    // No VIP rows, no code channels, no user-group channels, no agents:
    // those disappear. Channels and Agents & apps keep their heading.
    assert_eq!(
        titles(&sections),
        [
            "Direct messages",
            "External connections",
            "Starred",
            "Agents & apps",
            "Channels"
        ]
    );
}

#[test]
fn unless_starred_shows_read_starred_channels_but_hides_muted_ones() {
    let mut ws = workspace();
    add(&mut ws, channel("C_STAR", "announcements"));
    add(&mut ws, channel("C_STAR_MUTED", "lounge"));
    add(&mut ws, channel("C_READ", "quiet"));
    add(&mut ws, channel("C_UNREAD", "busy"));
    add(&mut ws, channel("C_MUTED_PING", "pinged"));
    unread(&mut ws, "C_UNREAD", 4, 0);
    unread(&mut ws, "C_MUTED_PING", 1, 1);

    let sections = layout(&ws, None);

    assert_eq!(rows(&sections, "Starred"), ["C_STAR"]);
    // A muted channel still surfaces for a mention, but muted rows sort
    // after every unmuted one.
    assert_eq!(rows(&sections, "Channels"), ["C_UNREAD", "C_MUTED_PING"]);
}

#[test]
fn the_open_conversation_is_always_shown() {
    let mut ws = workspace();
    add(&mut ws, channel("C_READ", "quiet"));
    assert_eq!(rows(&layout(&ws, Some("C_READ")), "Channels"), ["C_READ"]);
}

#[test]
fn collapsed_sections_show_only_unread_rows() {
    let mut ws = workspace();
    add(&mut ws, channel("C_STAR", "announcements"));
    add(&mut ws, channel("C_BOTH", "both"));
    unread(&mut ws, "C_BOTH", 1, 0);
    ws.sidebar.toggle_collapsed("L_STARS");

    let sections = layout(&ws, None);
    let starred = sections.iter().find(|s| s.title == "Starred").unwrap();

    assert!(starred.collapsed);
    assert_eq!(starred.channel_ids, ["C_BOTH"]);
}

#[test]
fn alphabetical_sections_sort_like_intl_collator_with_numeric() {
    let mut ws = workspace();
    let names = [
        "2026-gap-year-fellowship",
        "123-dld",
        "1-richard-nixon",
        "345-dldive",
        "bartosz-fans",
        "ɐʇǝɯ",
        "atlantis-help",
        "cаmpfire-nyc", // Cyrillic а
        "crescent",
        "Hq",
        "hq-engineering",
    ];
    let mut ids = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let id = format!("C{i:02}");
        let mut c = channel(&id, name);
        c.is_starred = true;
        add(&mut ws, c);
        ids.push(id);
    }
    // No server sections: the starred flag drives the Starred section.
    ws.sidebar.sections.clear();

    let sections = layout(&ws, None);
    let order: Vec<&str> = rows(&sections, "Starred")
        .into_iter()
        .map(|id| ws.channels[id].name.as_deref().unwrap())
        .collect();

    assert_eq!(
        order,
        [
            "1-richard-nixon",
            "123-dld",
            "345-dldive",
            "2026-gap-year-fellowship",
            "atlantis-help",
            "ɐʇǝɯ",
            "bartosz-fans",
            "crescent",
            "cаmpfire-nyc",
            "Hq",
            "hq-engineering",
        ]
    );
}

#[test]
fn dms_sort_by_latest_message_with_unread_ones_first() {
    let mut ws = workspace();
    for (id, user) in [("D_OLD", "U_A"), ("D_NEW", "U_B"), ("D_READ", "U_C")] {
        add(&mut ws, dm(id, user));
    }
    add(&mut ws, dm("D_SELF", "U_SELF"));
    unread(&mut ws, "D_OLD", 1, 0);
    unread(&mut ws, "D_NEW", 1, 0);
    unread(&mut ws, "D_SELF", 1, 0);
    for (id, latest) in [("D_OLD", "100.0"), ("D_NEW", "300.0"), ("D_READ", "400.0")] {
        ws.messages.entry(id.into()).or_default().latest = Some(latest.into());
    }

    let sections = layout(&ws, Some("D_READ"));

    // Your own DM sorts last in Direct messages, behind even read DMs.
    assert_eq!(
        rows(&sections, "Direct messages"),
        ["D_NEW", "D_OLD", "D_READ", "D_SELF"]
    );
}

#[test]
fn app_dms_and_slackbot_go_to_agents_and_apps() {
    let mut ws = workspace();
    ws.users.insert(
        "B_BOT".into(),
        User {
            id: "B_BOT".into(),
            is_bot: true,
            ..Default::default()
        },
    );
    add(&mut ws, dm("D_BOT", "B_BOT"));
    add(&mut ws, dm("D_SLACKBOT", "USLACKBOT"));
    unread(&mut ws, "D_BOT", 1, 0);
    unread(&mut ws, "D_SLACKBOT", 1, 0);

    let sections = layout(&ws, None);

    assert_eq!(rows(&sections, "Agents & apps"), ["D_SLACKBOT", "D_BOT"]);
    assert!(rows(&sections, "Direct messages").is_empty());
}

#[test]
fn closed_and_deleted_dms_stay_out_of_the_sidebar() {
    let mut ws = workspace();
    let mut closed = dm("D_CLOSED", "U_A");
    closed.extra.insert("is_open".into(), json!(false));
    add(&mut ws, closed);
    let mut closed_unread = dm("D_CLOSED_UNREAD", "U_B");
    closed_unread.extra.insert("is_open".into(), json!(false));
    add(&mut ws, closed_unread);
    unread(&mut ws, "D_CLOSED_UNREAD", 1, 0);
    ws.users.insert(
        "U_GONE".into(),
        User {
            id: "U_GONE".into(),
            deleted: true,
            ..Default::default()
        },
    );
    add(&mut ws, dm("D_GONE", "U_GONE"));
    unread(&mut ws, "D_GONE", 1, 0);

    assert_eq!(
        rows(&layout(&ws, None), "Direct messages"),
        ["D_CLOSED_UNREAD"]
    );
}

#[test]
fn starred_wins_over_a_custom_section() {
    let mut ws = workspace();
    let mut sections = ws.sidebar.sections.clone();
    sections.push(
        serde_json::from_value(json!({
            "channel_section_id":"L_MINE","name":"mine","type":"standard",
            "channel_ids_page":{"channel_ids":["C_BOTH","C_MINE"]}
        }))
        .unwrap(),
    );
    ws.sidebar.sections = sections;
    add(&mut ws, channel("C_BOTH", "both"));
    add(&mut ws, channel("C_MINE", "mine-only"));

    let layout = layout(&ws, None);

    assert_eq!(rows(&layout, "Starred"), ["C_BOTH"]);
    assert_eq!(rows(&layout, "mine"), ["C_MINE"]);
}

#[test]
fn the_open_vip_conversation_stays_put_after_it_is_read() {
    let mut ws = workspace();
    add(&mut ws, dm("D_VIP", "U_VIP"));
    unread(&mut ws, "D_VIP", 1, 0);
    let mut memory = SidebarMemory::default();
    grouped_sidebar_sections(&ws, Some("D_VIP"), &mut memory);

    unread(&mut ws, "D_VIP", 0, 0);
    let still_open = grouped_sidebar_sections(&ws, Some("D_VIP"), &mut memory);
    assert_eq!(rows(&still_open, "VIP unreads"), ["D_VIP"]);

    let moved_on = grouped_sidebar_sections(&ws, None, &mut memory);
    assert!(rows(&moved_on, "VIP unreads").is_empty());
}

#[test]
fn counts_clear_a_stale_unread_for_a_channel_read_elsewhere() {
    let mut ws = workspace();
    let mut c = channel("C1", "quiet");
    c.has_unreads = true;
    add(&mut ws, c);
    unread(&mut ws, "C1", 1, 0);
    ws.apply_counts(
        serde_json::from_value(json!({"channels":[
            {"id":"C1","last_read":"5.0","latest":"5.0","mention_count":0,"has_unreads":false}
        ]}))
        .unwrap(),
    );
    assert_eq!(ws.messages["C1"].unread_count, 0);
    assert!(!ws.channels["C1"].has_unreads);
    assert!(rows(&layout(&ws, None), "Channels").is_empty());
}

#[test]
fn starred_hides_read_dormant_channels_unless_the_section_shows_all() {
    let mut ws = workspace();
    add(&mut ws, channel("C_STAR", "announcements"));
    let mut dormant = channel("C_BOTH", "sleepy");
    dormant
        .extra
        .insert("properties".into(), json!({"is_dormant": true}));
    add(&mut ws, dormant);

    assert_eq!(rows(&layout(&ws, None), "Starred"), ["C_STAR"]);

    unread(&mut ws, "C_BOTH", 0, 1);
    assert_eq!(rows(&layout(&ws, None), "Starred"), ["C_BOTH", "C_STAR"]);
}

#[test]
fn code_channels_show_only_while_open_and_in_their_own_section() {
    let mut ws = workspace();
    let code = |id: &str, open: Option<bool>| {
        let mut c = channel(id, id);
        c.extra.insert(
            "properties".into(),
            json!({"record_channel":{"record_id":format!("AC:1:{id}"),"record_type":"agent_channel"}}),
        );
        if let Some(open) = open {
            c.extra.insert("is_open".into(), json!(open));
        }
        c
    };
    add(&mut ws, code("C_CODE_OPEN", Some(true)));
    add(&mut ws, code("C_CODE_UNSET", None));
    unread(&mut ws, "C_CODE_UNSET", 1, 0);

    let sections = layout(&ws, None);

    assert_eq!(rows(&sections, "Code channels"), ["C_CODE_OPEN"]);
    assert!(rows(&sections, "Channels").is_empty());
}
