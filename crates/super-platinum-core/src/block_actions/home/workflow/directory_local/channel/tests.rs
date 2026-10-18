use super::*;
use serde_json::json;

#[test]
fn retained_channel_vectors_preserve_scores_and_metadata() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    assert_eq!(cases.len(), 26);
    for case in cases {
        let mut options = Options::workflow(
            case["raw_query"].as_str().unwrap(),
            case["normalized_query"].as_str().unwrap(),
            case["matcher_query"].as_str().unwrap(),
            false,
        );
        options.allow_empty_query = case["allow_empty"].as_bool().unwrap();
        options.prefer_exact_match = case["prefer_exact"].as_bool().unwrap();
        options.passthrough_remote_fuzzy_filtering = case["passthrough"].as_bool().unwrap();
        let remote = case["remote"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let rows = filter_channels(
            case["items"].as_array().unwrap(),
            &options,
            case["search_previous"].as_bool().unwrap(),
            &remote,
            |item| !case["deny"].as_array().unwrap().contains(&item["id"]),
            |item| {
                case["display"][item["id"].as_str().unwrap()]
                    .as_str()
                    .unwrap()
                    .to_owned()
            },
            |name| case["previous"][name].as_str().unwrap().to_owned(),
        );
        let actual: Vec<Value> = rows.iter().map(|row| json!({
            "id": row.id,
            "score": row.score,
            "exact": row.is_exact_match,
            "previous": row.previous_name_scores.as_ref().map(|scores| scores.iter().map(|score| json!({"name":score.name,"score":score.score})).collect::<Vec<_>>()),
            "best": row.best_previous_name_match,
            "passthrough": row.is_flannel_fuzzy_passthrough,
        })).collect();
        assert_eq!(json!(actual), case["expected"], "{}", case["label"]);
        for row in rows {
            assert!(
                case["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| std::ptr::eq(item, row.item))
            );
        }
    }
}

#[test]
fn name_input_keeps_unresolved_producer_requirements_explicit() {
    let record = json!({"name":"mc:123:raw", "properties":{"record_channel":{"record_id":"R"},"meetings_channel":{"event_link_id":"E"}}});
    assert_eq!(
        channel_name_input(&record),
        Some(ChannelNameInput::Record {
            raw_name_or_ellipsis: "mc:123:raw"
        })
    );
    let missing_record = json!({"properties":{"record_channel":{"record_id":"R"}}});
    assert_eq!(
        channel_name_input(&missing_record),
        Some(ChannelNameInput::Record {
            raw_name_or_ellipsis: "..."
        })
    );
    let meeting =
        json!({"name":"mc:123:raw", "properties":{"meetings_channel":{"event_link_id":"E"}}});
    assert_eq!(
        channel_name_input(&meeting),
        Some(ChannelNameInput::Meeting {
            raw_name: Some("mc:123:raw"),
            event_link_id: Some("E")
        })
    );
    let unnamed = json!({"id":"C"});
    assert_eq!(
        channel_name_input(&unnamed),
        Some(ChannelNameInput::Ordinary { raw_name: None })
    );
    assert_eq!(channel_name_input(&json!({"isUnknown":true})), None);
    assert_eq!(channel_name_input(&json!({"is_mpim":true})), None);
}
