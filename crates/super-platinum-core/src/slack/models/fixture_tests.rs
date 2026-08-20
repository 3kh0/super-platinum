use super::*;

#[test]
fn user_profile_accepts_null_custom_fields() {
    let profile: UserProfile = serde_json::from_value(serde_json::json!({
        "display_name": "Alice",
        "fields": null
    }))
    .unwrap();
    assert!(profile.fields.is_empty());
}

#[test]
fn activity_feed_decodes_mixed_item_shapes() {
    let page: ActivityFeedPage = serde_json::from_str(
            r#"{
                "items": [
                    {"is_unread":true,"feed_ts":"1783828299.522099","key":"thread_v2-C0B1-1",
                     "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{
                        "channel_id":"C0B1","thread_ts":"1783823177.936649","latest_ts":"1783828299.522099",
                        "min_unread_ts":"1783828200.000001","unread_msg_count":1}}}}},
                    {"is_unread":false,"feed_ts":"1783827397.000000","key":"reaction-1",
                     "item":{"type":"message_reaction","message":{"ts":"1783716780.838909","channel":"C05S"},
                             "reaction":{"user":"U08R","name":"yay"}}},
                    {"is_unread":false,"feed_ts":"1783825812.401009","key":"at_user-1",
                     "item":{"type":"at_user","message":{"ts":"1783825812.401009","channel":"C08G",
                             "thread_ts":"1783822728.610059","author_user_id":"U07U"}}},
                    {"is_unread":true,"feed_ts":"1784342170.887589","key":"channel-G01Q",
                     "item":{"type":"channel","bundle_info":{"payload":{"channel_entry":{
                        "latest_message":{"ts":"1784342170.887589","channel":"G01Q"},
                        "unread_msg_count":3}}}}}
                ]
            }"#,
        )
        .expect("decode activity feed");

    assert_eq!(page.items.len(), 4);

    let thread = &page.items[0];
    assert_eq!(thread.item.kind, "thread_v2");
    assert_eq!(thread.channel(), Some("C0B1"));
    assert_eq!(thread.ts(), Some("1783823177.936649"));
    assert_eq!(thread.min_unread_ts(), Some("1783828200.000001"));
    assert!(thread.is_unread);

    let reaction = &page.items[1];
    assert_eq!(reaction.channel(), Some("C05S"));
    assert_eq!(reaction.ts(), Some("1783716780.838909"));
    assert_eq!(reaction.author(), Some("U08R"));
    assert_eq!(reaction.item.reaction.as_ref().unwrap().name, "yay");

    let mention = &page.items[2];
    assert_eq!(mention.channel(), Some("C08G"));
    assert_eq!(mention.thread_ts(), Some("1783822728.610059"));
    assert_eq!(mention.author(), Some("U07U"));

    let channel = &page.items[3];
    assert_eq!(channel.item.kind, "channel");
    assert_eq!(channel.channel(), Some("G01Q"));
    assert_eq!(channel.ts(), Some("1784342170.887589"));
    assert_eq!(channel.thread_ts(), None);
    assert_eq!(channel.unread_msg_count(), 3);
    assert_eq!(channel.identity(), "channel:G01Q");
}

#[test]
fn deserialize_history_response() {
    let page: HistoryPage = serde_json::from_str(
            r#"{
                "ok": true,
                "messages": [
                    {"type":"message","user":"U01234567","text":"fixture message one","ts":"1783372360.741769"},
                    {"type":"message","user":"U07654321","text":"two","ts":"1783372400.100200",
                     "reactions":[{"name":"wave","users":["U01234567"],"count":1}]},
                    {"type":"message","subtype":"channel_join","user":"U01234567","ts":"1783372000.000001"}
                ],
                "has_more": true,
                "pin_count": 2,
                "response_metadata": {"next_cursor": "bmV4dF9jdXJzb3I="}
            }"#,
        )
        .unwrap();
    assert_eq!(page.messages.len(), 3);
    assert!(page.has_more);
    assert_eq!(page.pin_count, Some(2));
    assert_eq!(
        page.response_metadata
            .as_ref()
            .unwrap()
            .next_cursor
            .as_deref(),
        Some("bmV4dF9jdXJzb3I=")
    );
    assert_eq!(
        page.messages[0].text.as_deref(),
        Some("fixture message one")
    );
    assert_eq!(page.messages[1].reactions[0].name, "wave");
    assert_eq!(page.messages[2].subtype.as_deref(), Some("channel_join"));
}

#[test]
fn deserialize_edge_channels_info() {
    let page: EdgeResults<Channel> = serde_json::from_str(
        r#"{"ok":true,"results":[
                {"id":"C0159TSJVH8","name":"general","is_channel":true,"updated":1783372000},
                {"id":"C09876543","name":"random","is_channel":true,"updated":1783371800}
            ],"failed_ids":[]}"#,
    )
    .unwrap();
    assert!(page.ok);
    assert_eq!(page.results.len(), 2);
    assert_eq!(page.results[0].name.as_deref(), Some("general"));
    assert!(page.failed_ids.is_empty());
}

#[test]
fn deserialize_sidebar_dms_response() {
    let page: SidebarDmsPage = serde_json::from_str(
        r#"{
                "ok": true,
                "ims": [{"id":"D1","name":"alice","is_im":true,"user":"U1"}],
                "mpdms": [{"id":"G1","name":"alice--bob","is_mpim":true,"is_group":true}]
            }"#,
    )
    .unwrap();

    assert_eq!(page.all_channels().len(), 2);
    assert!(page.all_channels()[0].is_im);
    assert!(page.all_channels()[1].is_mpim);
}

#[test]
fn deserialize_profile_extras_recent_conversations() {
    let page: ProfileExtrasPage =
        serde_json::from_str(r#"{"ok":true,"im_mpim_ids":["D1","G2","G3"],"has_more_mpims":true}"#)
            .unwrap();
    assert_eq!(page.im_mpim_ids, ["D1", "G2", "G3"]);
    assert!(page.has_more_mpims);
}

#[test]
fn deserialize_boot_sidebar_preferences() {
    let boot: BootData = serde_json::from_str(
        r#"{
                "self": {"id":"U_SELF"},
                "starred": ["C_STAR", "C_OTHER"],
                "channels_priority": {"C_VIP": 0.9, "D_DM": 0.25},
                "prefs": {
                    "sidebar_behavior": "hide_read_channels_unless_starred",
                    "priority_sidebar_section": true,
                    "channel_sections": "{\"priority\":{\"sort\":\"recent\"}}"
                }
            }"#,
    )
    .unwrap();

    assert_eq!(boot.starred, ["C_STAR", "C_OTHER"]);
    assert_eq!(boot.channels_priority.get("C_VIP"), Some(&0.9));
    assert!(boot.prefs.priority_sidebar_section);
    assert_eq!(
        boot.prefs.sidebar_behavior.as_deref(),
        Some("hide_read_channels_unless_starred")
    );
}

#[test]
fn deserialize_channel_sections_page() {
    let page: ChannelSectionsPage = serde_json::from_str(
            r#"{
                "ok": true,
                "channel_sections": [
                    {"channel_section_id":"L_CONNECT","name":"Slack Connect","type":"slack_connect",
                     "emoji":"","next_channel_section_id":"L_DMS","last_updated":1754360820,
                     "channel_ids_page":{"channel_ids":[],"count":0},"is_redacted":false},
                    {"channel_section_id":"L_DMS","name":"Direct Messages","type":"direct_messages",
                     "next_channel_section_id":"L_STARS","channel_ids_page":{"channel_ids":[],"count":0}},
                    {"channel_section_id":"L_STARS","name":"","type":"stars",
                     "next_channel_section_id":"L_CUSTOM","channel_ids_page":{"channel_ids":[],"count":0}},
                    {"channel_section_id":"L_CUSTOM","name":"my section","type":"standard",
                     "next_channel_section_id":"L_CHANNELS",
                     "channel_ids_page":{"channel_ids":["C_IN_SECTION"],"count":1}},
                    {"channel_section_id":"L_CHANNELS","name":"Channels","type":"channels",
                     "next_channel_section_id":null,"channel_ids_page":{"channel_ids":[],"count":0}}
                ],
                "last_updated": 1783732486,
                "count": 5
            }"#,
        )
        .unwrap();

    assert_eq!(page.channel_sections.len(), 5);
    assert_eq!(page.channel_sections[0].kind, "slack_connect");
    assert_eq!(
        page.channel_sections[0].next_channel_section_id.as_deref(),
        Some("L_DMS")
    );
    assert_eq!(
        page.channel_sections[3].channel_ids_page.channel_ids,
        ["C_IN_SECTION"]
    );
    assert!(page.channel_sections[4].next_channel_section_id.is_none());
}

#[test]
fn deserialize_counts_unread_string_updated() {
    let page: CountsPage = serde_json::from_str(
        r#"{
                "ok": true,
                "activity_v2": {
                    "at_user": 6,
                    "dm": 2,
                    "thread_v2": 14,
                    "message_reaction": 0
                },
                "ims": [
                    {
                        "id": "D083XVDGBJ8",
                        "is_im": true,
                        "updated": "1778339234.000100",
                        "mention_count": 1,
                        "has_unreads": true
                    }
                ]
            }"#,
    )
    .unwrap();

    assert_eq!(page.ims[0].updated, Some(1_778_339_234));
    assert!(page.ims[0].has_unreads);
    assert_eq!(page.ims[0].mention_count, Some(1));
    assert_eq!(page.activity_unread_count(), Some(22));
}

#[test]
fn deserialize_edge_users_list() {
    let page: EdgeResults<User> = serde_json::from_str(
        r#"{"ok":true,"results":[{
                "id":"U01234567","name":"alice","real_name":"Alice Anderson",
                "profile":{"display_name":"alice","real_name":"Alice Anderson"}
            }],"failed_ids":["U_MISSING"]}"#,
    )
    .unwrap();
    assert!(page.ok);
    assert_eq!(page.results.len(), 1);
    assert_eq!(
        page.results[0]
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("alice")
    );
    assert_eq!(page.failed_ids, vec!["U_MISSING".to_owned()]);
}

#[test]
fn deserialize_users_search_results() {
    let page: EdgeResults<User> = serde_json::from_str(
        r#"{"ok":true,"results":[
                {"id":"U0ALICE","name":"alice","deleted":false,"color":"9f69e7",
                 "real_name":"Alice A","tz":"America/New_York","is_admin":true,
                 "is_bot":false,"is_app_user":false,"updated":1783000000,
                 "enterprise_user":{"id":"W1"},"enterprise_id":"E09V59WQY1E",
                 "profile":{"display_name":"alice","real_name":"Alice A"}},
                {"id":"U0BOT","name":"helperbot","is_bot":true}
            ]}"#,
    )
    .unwrap();
    assert!(page.ok);
    assert_eq!(page.results.len(), 2);
    assert_eq!(page.results[0].id, "U0ALICE");
    assert_eq!(
        page.results[0]
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("alice")
    );
    assert!(page.results[1].is_bot);
}

#[test]
fn deserialize_conversations_open() {
    let opened: OpenedConversation = serde_json::from_str(
        r#"{"ok":true,"no_op":false,"already_open":true,"channel":{"id":"D0ALICE"}}"#,
    )
    .unwrap();
    assert_eq!(opened.channel.id, "D0ALICE");
}

#[test]
fn deserialize_search_messages_page() {
    let page: SearchMessagesPage = serde_json::from_str(
            r#"{
                "ok": true,
                "module": "messages",
                "query": "hello",
                "items": [
                    {
                        "iid": "21c94942-e73c-4cb6-a605-44994d411930",
                        "team": "T0266FRGM",
                        "channel": {"id":"C07TM4C0AQ5","name":"help","is_channel":true,"is_private":false},
                        "messages": [{
                            "type":"message",
                            "text":"Hello, can anyone help me?",
                            "ts":"1783431840.570159",
                            "thread_ts":"1783431840.570159",
                            "user":"U0BF7PL6RQF",
                            "username":"lolfero095",
                            "reply_count":5,
                            "reactions":[{"name":"white_check_mark","count":1,"users":["U0ASE1R05FW"]}],
                            "permalink":"https://hackclub.slack.com/archives/C07TM4C0AQ5/p1783431840570159"
                        }]
                    }
                ],
                "pagination": {"first":1,"last":5,"page":1,"page_count":23551,"per_page":5,"total_count":117752}
            }"#,
        )
        .unwrap();

    assert_eq!(page.module.as_deref(), Some("messages"));
    assert_eq!(page.items.len(), 1);
    let item = &page.items[0];
    assert_eq!(item.channel.as_ref().unwrap().name.as_deref(), Some("help"));
    let msg = &item.messages[0];
    assert_eq!(msg.text.as_deref(), Some("Hello, can anyone help me?"));
    assert_eq!(msg.ts.as_deref(), Some("1783431840.570159"));
    assert_eq!(msg.reply_count, Some(5));
    assert_eq!(msg.reactions[0].name, "white_check_mark");
    assert_eq!(
        msg.extra["permalink"].as_str(),
        Some("https://hackclub.slack.com/archives/C07TM4C0AQ5/p1783431840570159")
    );
    let pagination = page.pagination.unwrap();
    assert_eq!(pagination.page, Some(1));
    assert_eq!(pagination.page_count, Some(23551));
    assert_eq!(pagination.total_count, Some(117752));
}

#[test]
fn deserialize_search_inline_page() {
    let page: SearchInlinePage = serde_json::from_str(
            r#"{
                "items": [
                    {
                        "channel_id": "C0BBMA16677",
                        "iid": "fac7c614-5c33-4702-91bd-06122777ee4f",
                        "permalink": "https://hackclub.enterprise.slack.com/archives/C0BBMA16677/p1783308502631879",
                        "ts": "1783308502.631879",
                        "user": "U0AEY1PUMPX"
                    }
                ],
                "ok": true,
                "pagination": { "first": 1, "last": 2, "page": 1, "page_count": 1, "per_page": 20, "total_count": 2 },
                "query": "deploy"
            }"#,
        )
        .unwrap();

    assert_eq!(page.items.len(), 1);
    let item = &page.items[0];
    assert_eq!(item.channel_id.as_deref(), Some("C0BBMA16677"));
    assert_eq!(item.ts.as_deref(), Some("1783308502.631879"));
    assert_eq!(item.user.as_deref(), Some("U0AEY1PUMPX"));
    assert_eq!(
        item.permalink.as_deref(),
        Some("https://hackclub.enterprise.slack.com/archives/C0BBMA16677/p1783308502631879")
    );

    let pagination = page.pagination.unwrap();
    assert_eq!(pagination.total_count, Some(2));
    assert!(pagination.extra.contains_key("first"));
}

#[test]
fn deserialize_message_attachments() {
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "ts":"1783328401.000100",
                "text":"check this",
                "attachments":[{
                    "id":1,
                    "service_name":"the Guardian",
                    "service_icon":"https://www.theguardian.com/favicon.ico",
                    "title":"Some headline",
                    "title_link":"https://www.theguardian.com/football/x",
                    "text":"A short description of the article.",
                    "image_url":"https://i.guim.co.uk/img/media/x/master/2961.jpg",
                    "from_url":"https://www.theguardian.com/football/x",
                    "fields":[{"title":"Score","value":"1-0","short":true}]
                }]
            }]}"#,
    )
    .unwrap();
    let att = &page.messages[0].attachments[0];
    assert_eq!(att.service_name.as_deref(), Some("the Guardian"));
    assert_eq!(att.title.as_deref(), Some("Some headline"));
    assert_eq!(
        att.title_link.as_deref(),
        Some("https://www.theguardian.com/football/x")
    );
    assert!(att.image_url.as_deref().unwrap().ends_with("2961.jpg"));
    assert_eq!(att.fields[0].title.as_deref(), Some("Score"));
    assert_eq!(att.fields[0].value.as_deref(), Some("1-0"));
    assert!(att.fields[0].short);
}

#[test]
fn deserialize_message_files() {
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "ts":"1783372600.000100",
                "files":[{
                    "id":"F123",
                    "name":"design.png",
                    "title":"Design mock",
                    "mimetype":"image/png",
                    "filetype":"png",
                    "pretty_type":"PNG",
                    "url_private":"https://files.slack.com/files-pri/T-F/design.png",
                    "thumb_160":"https://files.slack.com/files-tmb/T-F/design_160.png",
                    "size":2048,
                    "mode":"hosted"
                }]
            }]}"#,
    )
    .unwrap();
    let file = &page.messages[0].files[0];
    assert_eq!(file.id.as_deref(), Some("F123"));
    assert_eq!(file.title.as_deref(), Some("Design mock"));
    assert_eq!(file.pretty_type.as_deref(), Some("PNG"));
    assert_eq!(file.size, Some(2048));
    assert_eq!(file.extra["mode"].as_str(), Some("hosted"));
}

#[test]
fn deserialize_message_replied_envelope() {
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "subtype":"message_replied",
                "channel":"C1",
                "ts":"1783372700.000100",
                "message":{
                    "type":"message",
                    "user":"U1",
                    "text":"actual root",
                    "ts":"1783372600.000100",
                    "reply_count":1
                }
            }]}"#,
    )
    .unwrap();
    let envelope = &page.messages[0];
    let nested = envelope.message.as_ref().unwrap();
    assert_eq!(envelope.subtype.as_deref(), Some("message_replied"));
    assert_eq!(nested.user.as_deref(), Some("U1"));
    assert_eq!(nested.text.as_deref(), Some("actual root"));
}

#[test]
fn deserialize_bot_profile_message() {
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "bot_id":"B123",
                "username":"helper",
                "text":"hello",
                "ts":"1783372600.000100",
                "bot_profile":{
                    "id":"B123",
                    "name":"Helper Bot",
                    "user_id":"U_APP",
                    "icons":{
                        "image_36":"https://example.test/36.png",
                        "image_48":"https://example.test/48.png",
                        "image_72":"https://example.test/72.png"
                    }
                }
            }]}"#,
    )
    .unwrap();
    let msg = &page.messages[0];
    assert_eq!(msg.username.as_deref(), Some("helper"));
    let profile = msg.bot_profile.as_ref().unwrap();
    assert_eq!(profile.name.as_deref(), Some("Helper Bot"));
    assert_eq!(
        profile.icons.as_ref().unwrap().image_48.as_deref(),
        Some("https://example.test/48.png")
    );
}

#[test]
fn deserialize_message_unfurl_attachment() {
    // Trimmed from a real #out-of-context bot post.
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "ts":"1787180246.285849",
                "user":"U0BKU1GSAGJ",
                "bot_id":"B0BKVUPP2F3",
                "text":"user pfp <@U0A55A4B21K>",
                "blocks":[{
                    "type":"context",
                    "block_id":"8122cf2a",
                    "elements":[
                        {"type":"image","image_url":"https://cachet.example/u/r","alt_text":"user pfp"},
                        {"type":"mrkdwn","text":"<@U0A55A4B21K>","verbatim":false}
                    ]
                }],
                "attachments":[{
                    "id":1,
                    "author_icon":"https://avatars.slack-edge.com/x_48.png",
                    "author_id":"U09AFPZ852L",
                    "author_name":"kc",
                    "author_subname":"kc",
                    "channel_id":"C09M3V4E7MM",
                    "channel_team":"T0266FRGM",
                    "color":"D0D0D0",
                    "footer":"Thread in Slack Conversation",
                    "from_url":"https://example.slack.com/archives/C09M3V4E7MM/p1787153585078499",
                    "is_msg_unfurl":true,
                    "is_reply_unfurl":true,
                    "is_share":true,
                    "mrkdwn_in":["text"],
                    "text":"aarav gets TOUCHED by slack icl",
                    "ts":"1787153585.078499",
                    "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
                        "elements":[{"type":"text","text":"aarav gets TOUCHED by slack icl"}]}]}]
                }]
            }]}"#,
    )
    .unwrap();
    let attachment = &page.messages[0].attachments[0];
    assert_eq!(attachment.author_id.as_deref(), Some("U09AFPZ852L"));
    assert_eq!(attachment.channel_id.as_deref(), Some("C09M3V4E7MM"));
    assert_eq!(attachment.ts.as_deref(), Some("1787153585.078499"));
    assert!(attachment.is_msg_unfurl && attachment.is_reply_unfurl && attachment.is_share);
    assert_eq!(attachment.mrkdwn_in, vec!["text".to_owned()]);
    assert_eq!(attachment.blocks.len(), 1);
    assert_eq!(page.messages[0].blocks.len(), 1);
}

#[test]
fn deserialize_link_unfurl_and_numeric_attachment_ts() {
    // Legacy bot attachments send `ts` as a bare epoch number, not a string.
    // Rejecting those broke warm boot from the on-disk cache.
    let page: HistoryPage = serde_json::from_str(
        r#"{"ok":true,"messages":[{
                "type":"message",
                "ts":"1787000000.000100",
                "attachments":[{
                    "id":1,
                    "service_name":"Stardance",
                    "service_icon":"https://example.test/icon.png",
                    "title":"UrStudyBuddy",
                    "title_link":"https://example.test/projects/22862",
                    "text":"Here is a buddy for u.",
                    "image_url":"https://example.test/og.png",
                    "image_width":1200,
                    "image_height":630,
                    "footer":"Stardance",
                    "footer_icon":"https://example.test/footer.png",
                    "ts":1786923846
                }]
            }]}"#,
    )
    .unwrap();
    let attachment = &page.messages[0].attachments[0];
    assert_eq!(attachment.ts.as_deref(), Some("1786923846"));
    assert_eq!(attachment.image_width, Some(1200));
    assert_eq!(attachment.image_height, Some(630));
    assert_eq!(attachment.service_name.as_deref(), Some("Stardance"));
    assert!(!attachment.is_msg_unfurl);
}
