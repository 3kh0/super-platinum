use super::*;
use serde_json::json;

fn shell() -> ShellState {
    ShellState::fixture(crate::media::MediaRegistry::default())
}
fn text(nodes: &[Inline]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            Inline::File { kind, label, .. } => {
                label
                    .as_ref()
                    .map(|v| v.text.as_str())
                    .unwrap_or(match kind {
                        files::Kind::Canvas => "Canvas unavailable",
                        files::Kind::List => "List unavailable",
                    })
            }
            Inline::Text(text)
            | Inline::Emoji { text, .. }
            | Inline::User { label: text, .. }
            | Inline::Channel { label: text, .. } => text.as_str(),
        })
        .collect()
}

#[test]
fn trailing_scanner_matches_actual_retained_hozi_oracle() {
    // Generated from actual retained HOzI/pH4l module execution, not this scanner.
    let cases: Vec<(String, Vec<(String, String)>)> =
        serde_json::from_str(include_str!("tokens-oracle.json")).unwrap();
    for (input, expected) in cases {
        let actual = tokens::split(&input)
            .into_iter()
            .map(|(emoji, text)| {
                (
                    if emoji { "emoji" } else { "text" }.to_owned(),
                    text.to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{input}");
    }
}

#[test]
fn explicit_component_is_one_value_and_preserves_unknown_or_unsupported_input() {
    let s = shell();
    for value in [
        "",
        "unknown_name",
        ":UNKNOWN_NAME:",
        ":wave: :smile:",
        " wave ",
        ":wave::skin-tone-3:",
    ] {
        assert_eq!(single(&s, "T1", value, false), Inline::Text(value.into()));
    }
    for value in ["wave", ":wave:", ":WAVE:"] {
        assert_eq!(
            single(&s, "T1", value, false),
            Inline::Emoji {
                text: "👋".into(),
                title: None,
                image: None,
            }
        );
    }
}

#[test]
fn prefix_is_plain_but_trailing_emoji_survives_literal_angle_entities() {
    let s = shell();
    let raw = json!({"template":"Prefix :wave: {e} <@U1> <b>x</b> &amp; :wave:",
        "variables":{"e":{"type":"EMOJI","value":":wave:"}}});
    let nodes = project(&s, "T1", 0, &raw);
    assert_eq!(text(&nodes), "Prefix :wave: 👋 <@U1> <b>x</b> &amp; 👋");
    let emojis: Vec<_> = nodes
        .iter()
        .filter_map(|node| match node {
            Inline::Emoji { title, .. } => Some(title.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(emojis, vec![None, Some(":wave:".into())]);
}

#[test]
fn custom_media_is_registered_without_readiness_or_network_and_standard_wins() {
    let mut s = shell();
    let w = s.core.workspaces.get_mut("T1").unwrap();
    for (name, value) in [
        ("wave", "https://fixture.invalid/override.png"),
        ("équipe", "https://fixture.invalid/team.png"),
        ("team_alias", "alias:équipe"),
    ] {
        w.custom_emoji.insert(
            name.into(),
            serde_json::from_value(json!({"name":name,"value":value})).unwrap(),
        );
    }
    assert!(
        matches!(single(&s,"T1",":wave:",true), Inline::Emoji { image:None, text, .. } if text=="👋")
    );
    for raw in [":équipe:", ":team_alias:"] {
        let Inline::Emoji {
            text,
            title,
            image: Some(id),
        } = single(&s, "T1", raw, true)
        else {
            panic!("missing custom media")
        };
        assert_eq!(text, raw);
        assert_eq!(title.as_deref(), Some(raw));
        assert!(!s.media.is_ready(&id));
    }
}

#[test]
fn unknown_falsy_and_duplicate_parts_keep_original_order_without_actions() {
    let s = shell();
    let nodes = project(
        &s,
        "T1",
        0,
        &json!({"template":"{e}{e}{x}{no}{missing} :unknown:",
        "variables":{"e":{"type":"EMOJI","value":":wave:"},"x":{"type":"OTHER","value":"discarded"},"no":false}}),
    );
    assert_eq!(text(&nodes), "👋👋replacement.value{no}{missing} :unknown:");
}

#[test]
fn ordinary_channel_name_preserves_bytes_and_occurrences_with_separate_icon() {
    let mut s = shell();
    let channel = serde_json::from_value(
        json!({"id":"CLABEL", "name":"Équipe & <literal>", "is_channel":true}),
    )
    .unwrap();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("CLABEL".into(), channel);
    let raw = json!({"template":"Open {c} then {c}", "variables":{"c":{"type":"CHANNEL","value":"CLABEL"}}});
    let nodes = project(&s, "T1", 0, &raw);
    let channels: Vec<_> = nodes
        .iter()
        .filter_map(|n| match n {
            Inline::Channel {
                id,
                label,
                icon,
                part,
            } => Some((id.as_str(), label.as_str(), *icon, *part)),
            _ => None,
        })
        .collect();
    assert_eq!(
        channels,
        [
            ("CLABEL", "Équipe & <literal>", Some(Icon::Tag), 1),
            ("CLABEL", "Équipe & <literal>", Some(Icon::Tag), 3)
        ]
    );
    assert_eq!(
        text(&nodes),
        "Open Équipe & <literal> then Équipe & <literal>"
    );
    assert!(
        matches!(project(&s,"OTHER",0,&raw).get(1),Some(Inline::Channel {label,icon:None,..}) if label=="Channel unavailable")
    );
}

#[test]
fn local_icon_does_not_claim_special_or_incomplete_channel_metadata() {
    let base = json!({"id":"CLABEL","name":"ordinary","is_channel":true});
    for patch in [
        json!({"is_channel":false}),
        json!({"is_im":true}),
        json!({"is_mpim":true}),
        json!({"is_ext_shared":true}),
        json!({"is_shared":true}),
        json!({"is_org_shared":true}),
        json!({"is_file":true}),
        json!({"isUnknown":true}),
        json!({"isNonExistent":true}),
        json!({"is_frozen":true}),
        json!({"is_frozen":"false"}),
        json!({"name":"mc:event:meeting"}),
        json!({"name":""}),
        json!({"name":null}),
        json!({"properties":false}),
        json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
        json!({"properties":{"meetings_channel":{"is_meeting_channel":true}}}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"properties":{"meetings_channel":false}}),
        json!({"is_shared":"false"}),
    ] {
        let mut raw = base.clone();
        raw.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let channel = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(local_channel_icon(&channel), None, "{raw}");
    }
    let ordinary = serde_json::from_value(json!({"id":"CLABEL","name":" raw ","is_channel":true,
        "is_shared":false,"properties":{"record_channel":{"record_id":""},"huddles":{"is_huddle_channel":false}}})).unwrap();
    assert_eq!(local_channel_icon(&ordinary), Some(Icon::Tag));
}

#[test]
fn local_private_and_archived_labels_keep_original_names_and_occurrences() {
    let mut s = shell();
    let name = " \u{00c9}quipe & <literal> \u{4e2d}\u{1f98a} ";
    let raw = json!({"template":"Open {c} then {c}","variables":{"c":{"type":"CHANNEL","value":"CLABEL"}}});
    for (patch, icon, archived) in [
        (json!({"is_channel":true}), Icon::Tag, false),
        (
            json!({"is_channel":true,"is_private":true}),
            Icon::LockFilled,
            false,
        ),
        (json!({"is_group":true}), Icon::LockFilled, false),
        (
            json!({"is_channel":true,"is_archived":true}),
            Icon::Archive,
            true,
        ),
        (
            json!({"is_channel":true,"is_private":true,"is_archived":true}),
            Icon::ArchiveFilled,
            true,
        ),
        (
            json!({"is_group":true,"is_archived":true}),
            Icon::ArchiveFilled,
            true,
        ),
    ] {
        let mut channel = json!({"id":"CLABEL","name":name,"is_frozen":false});
        channel
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let channel = serde_json::from_value(channel).unwrap();
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .channels
            .insert("CLABEL".into(), channel);
        let original = s.core.workspaces["T1"].channels["CLABEL"].clone();
        let label = if archived {
            format!("{name} (archived)")
        } else {
            name.to_owned()
        };
        let nodes = project(&s, "T1", 0, &raw);
        for part in [1, 3] {
            assert!(
                matches!(&nodes[part],Inline::Channel{id,label:actual,icon:Some(actual_icon),part:ordinal}
                    if id=="CLABEL" && actual==&label && *actual_icon==icon && *ordinal==part)
            );
        }
        assert_eq!(
            serde_json::to_value(&s.core.workspaces["T1"].channels["CLABEL"]).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }
}

#[test]
fn unsupported_private_archived_metadata_keeps_existing_fallback() {
    let mut s = shell();
    let raw = json!({"template":"{c}","variables":{"c":{"type":"CHANNEL","value":"CLABEL"}}});
    for patch in [
        json!({"is_ext_shared":true}),
        json!({"is_shared":true}),
        json!({"is_frozen":true,"unlinked":1,"grace_period_end":9}),
        json!({"is_mpim":true}),
        json!({"is_file":true}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
    ] {
        let mut channel =
            json!({"id":"CLABEL","name":" raw ","is_private":true,"is_archived":true});
        channel
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .channels
            .insert("CLABEL".into(), serde_json::from_value(channel).unwrap());
        assert!(
            matches!(&project(&s,"T1",0,&raw)[0],Inline::Channel{label,icon:None,..} if label=="# raw ")
        );
    }
}

fn name_policy(force: i32) -> super_platinum_core::block_actions::name_policy::Snapshot {
    use super_platinum_core::block_actions::name_policy::*;
    Snapshot::new(
        OrdinaryClientV2,
        Membership::observe(
            None,
            &["T1".into()],
            &std::collections::BTreeMap::from([("T1".into(), json!({"id":"T1"}))]),
        )
        .unwrap(),
        UserPrefs::decode(&json!({"ok":true,"prefs":{"display_real_names_override":force}}))
            .unwrap(),
        None,
    )
    .unwrap()
}

#[test]
fn user_name_obeys_observed_preference_and_exact_real_name_fallback() {
    let mut s = shell();
    let raw = json!({"id":"UNAME","name":"username-not-a-name","real_name":"Top real",
        "profile":{"real_name":" Profile <real> ","display_name":"Display & literal","real_name_normalized":"normalized"}});
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("UNAME".into(), serde_json::from_value(raw).unwrap());
    let real = name_policy(1);
    let display = name_policy(-1);
    assert_eq!(
        user_label(&s, "T1", "UNAME", Some(&real)),
        " Profile <real> "
    );
    assert_eq!(
        user_label(&s, "T1", "UNAME", Some(&display)),
        "Display & literal"
    );
    let member = s
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("UNAME")
        .unwrap();
    member.profile.as_mut().unwrap().display_name = Some(String::new());
    member.profile.as_mut().unwrap().real_name = Some(String::new());
    assert_eq!(user_label(&s, "T1", "UNAME", Some(&display)), "Top real");
    assert_eq!(user_label(&s, "T1", "UNAME", Some(&real)), "Top real");
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("UNAME")
        .unwrap()
        .real_name = None;
    assert_eq!(user_label(&s, "T1", "UNAME", Some(&real)), "");
    assert_eq!(user_label(&s, "T1", "UNAME", None), "Person unavailable");
    assert_eq!(
        user_label(&s, "OTHER", "UNAME", Some(&real)),
        "Person unavailable"
    );
}

#[test]
fn user_projection_does_not_guess_policy_and_preserves_duplicate_occurrences() {
    let s = shell();
    let raw = json!({"template":"{u} and {u}","variables":{"u":{"type":"USER","value":"U1"}}});
    assert_eq!(
        project(&s, "T1", 0, &raw),
        vec![
            Inline::User {
                id: "U1".into(),
                label: "Person unavailable".into(),
                deactivated: false,
                part: 0
            },
            Inline::Text(" and ".into()),
            Inline::User {
                id: "U1".into(),
                label: "Person unavailable".into(),
                deactivated: false,
                part: 2
            },
        ]
    );
}

#[test]
fn deactivated_member_retains_exact_policy_name_and_duplicate_occurrences() {
    let mut s = shell();
    s.core.workspaces.get_mut("T1").unwrap().users.insert(
        "UDELETED".into(),
        serde_json::from_value(json!({"id":"UDELETED","deleted":true,
            "real_name":"Top real","profile":{"real_name":" Real <name> ",
            "display_name":"Display & literal"}}))
        .unwrap(),
    );
    for (force, expected) in [(1, " Real <name> "), (-1, "Display & literal")] {
        assert_eq!(
            user_label(&s, "T1", "UDELETED", Some(&name_policy(force))),
            expected
        );
    }
    let raw =
        json!({"template":"{u} then {u}","variables":{"u":{"type":"USER","value":"UDELETED"}}});
    let nodes = project(&s, "T1", 0, &raw);
    for (index, part) in [(0, 0), (2, 2)] {
        assert_eq!(
            nodes[index],
            Inline::User {
                id: "UDELETED".into(),
                label: "Person unavailable".into(),
                deactivated: true,
                part
            }
        );
    }
    // A known deletion remains visible while policy is being refreshed; names
    // are not guessed and the raw label never absorbs the separate suffix.
    assert_eq!(user_label(&s, "T1", "UDELETED", None), "Person unavailable");
    assert!(matches!(
        project(&s, "OTHER", 0, &raw).first(),
        Some(Inline::User {
            deactivated: false,
            ..
        })
    ));
}

#[test]
fn deactivation_does_not_infer_deleted_from_other_member_statuses() {
    let mut s = shell();
    let raw = json!({"template":"{u}","variables":{"u":{"type":"USER","value":"USTATUS"}}});
    for patch in [
        json!({}),
        json!({"deleted":false}),
        json!({"suspended":true}),
        json!({"is_inactive":true}),
        json!({"is_org_user_deleted":true}),
        json!({"is_bot":true}),
    ] {
        let mut member = json!({"id":"USTATUS"});
        member
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .insert("USTATUS".into(), serde_json::from_value(member).unwrap());
        assert!(
            matches!(
                project(&s, "T1", 0, &raw).first(),
                Some(Inline::User {
                    deactivated: false,
                    ..
                })
            ),
            "{patch}"
        );
    }
}
