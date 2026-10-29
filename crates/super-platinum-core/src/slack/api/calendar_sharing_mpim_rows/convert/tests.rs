use super::*;
#[test]
fn known_open_cached_kind_controls_source_conversion_without_typed_defaults_or_outer_name() {
    let cached = json!({"id":"GMPIM0001","is_mpim":true,"is_open":true,"name":"Old Team",
        "members":["UINIT0001"],"updated":7,"opaque":{"old":1},"is_archived":true,
        "properties":{"has_custom_mpdm_name":true}});
    let wire = json!({"id":"GMPIM0001","name":"Ignored outer name","members":["UINIT0001","UOTHER001"],
        "details":{"name":"New Team","is_open":false,"is_mpim":false,"is_im":true,"is_member":false,
        "is_archived":true,"updated":7,"opaque":{"new":2},"purpose":{"value":"ignored"},
        "properties":{"has_custom_mpdm_name":true}}});
    let before = cached.clone();
    let converted = convert_calendar_known_open_mpim(&cached, &wire)
        .unwrap()
        .unwrap();
    assert_eq!(cached, before);
    assert_eq!(converted["name"], "New Team");
    assert_eq!(converted["updated"], 7);
    assert_eq!(converted["members"], json!(["UINIT0001", "UOTHER001"]));
    assert_eq!(converted["opaque"], json!({"new":2}));
    for key in [
        "is_open",
        "is_mpim",
        "is_channel",
        "is_member",
        "is_private",
        "is_global_shared",
        "is_org_shared",
    ] {
        assert_eq!(converted[key], true, "{key}");
    }
    for key in [
        "is_archived",
        "is_im",
        "is_group",
        "is_general",
        "is_org_default",
        "is_org_mandatory",
        "is_pending_ext_shared",
    ] {
        assert_eq!(converted[key], false, "{key}");
    }
    assert_eq!(converted["purpose"], json!({}));
    assert_eq!(converted["internal_team_ids"], json!([]));
    assert_eq!(converted["connected_limited_team_ids"], json!([]));
    assert!(converted["parent_conversation"].is_null());
    for (key, value) in [
        ("is_mpim", json!(false)),
        ("is_open", json!(false)),
        ("isUnknown", json!(true)),
        ("isNonExistent", Value::Null),
    ] {
        let mut wrong = cached.clone();
        wrong[key] = value;
        assert!(
            convert_calendar_known_open_mpim(&wrong, &wire)
                .unwrap()
                .is_none()
        );
    }
    let missing_kind =
        json!({"id":"GMPIM0001","members":["UINIT0001"],"details":{"name":"New Team"}});
    assert_eq!(
        convert_calendar_known_open_mpim(&cached, &missing_kind)
            .unwrap()
            .unwrap()["name"],
        "New Team"
    );
    for wrong in [
        json!({"id":"GMPIM0001","members":[],"details":{"id":"GOTHER001"}}),
        json!({"id":"GMPIM0001","members":[],"details":null}),
        json!({"id":"GMPIM0001","members":["U1"]}),
    ] {
        assert!(convert_calendar_known_open_mpim(&cached, &wrong).is_err());
    }
    assert!(
        convert_calendar_known_open_mpim(
            &cached,
            &json!({"id":"GMPIM0001","members":vec!["UINIT0001";10]})
        )
        .unwrap()
        .is_none()
    );
}
