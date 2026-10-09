use super::super::*;

#[test]
fn resolves_section_order_from_real_linked_list() {
    let page: crate::slack::models::ChannelSectionsPage = serde_json::from_str(
            r#"{
                "ok": true,
                "channel_sections": [
                    {"channel_section_id":"L_CONNECT","name":"Slack Connect","type":"slack_connect","next_channel_section_id":"L_DMS"},
                    {"channel_section_id":"L_DMS","name":"Direct Messages","type":"direct_messages","next_channel_section_id":"L_STARS"},
                    {"channel_section_id":"L_STARS","name":"","type":"stars","next_channel_section_id":"L_UG"},
                    {"channel_section_id":"L_UG","name":"helpers","type":"user_group","next_channel_section_id":"L_APPS"},
                    {"channel_section_id":"L_APPS","name":"Recent Apps","type":"recent_apps","next_channel_section_id":"L_CHANNELS"},
                    {"channel_section_id":"L_CHANNELS","name":"Channels","type":"channels","next_channel_section_id":"L_AGENTS"},
                    {"channel_section_id":"L_AGENTS","name":"Agents","type":"agents","next_channel_section_id":null}
                ]
            }"#,
        )
        .unwrap();
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://t".into(),
        token: "xoxc".into(),
    });
    ws.priority_sidebar_section = true;
    ws.apply_channel_sections(page);

    let titles: Vec<String> = ws
        .resolved_sidebar_sections()
        .into_iter()
        .map(|s| s.title)
        .collect();

    assert_eq!(
        titles,
        [
            "VIP unreads",
            "External connections",
            "Direct messages",
            "Starred",
            "helpers",
            "Agents & apps",
            "Channels",
            "Agents"
        ]
    );
}

#[test]
fn channel_label_variants() {
    let public = Channel {
        id: "C1".into(),
        name: Some("general".into()),
        is_channel: true,
        ..Default::default()
    };
    assert_eq!(channel_label(&public), "#general");

    let dm = Channel {
        id: "D1".into(),
        name: Some("alice".into()),
        is_im: true,
        ..Default::default()
    };
    assert_eq!(channel_label(&dm), "alice");

    let unnamed = Channel {
        id: "C9".into(),
        ..Default::default()
    };
    assert_eq!(channel_label(&unnamed), "C9");
}
