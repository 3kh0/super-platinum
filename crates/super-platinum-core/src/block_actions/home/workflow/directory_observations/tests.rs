use super::*;
use serde_json::json;

fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}
fn entity(key: &str) -> Entity {
    match key {
        "member" => Entity::Member,
        "channel" => Entity::Channel,
        "mpim" => Entity::Mpim,
        "ims" => Entity::Im,
        _ => panic!("unknown oracle kind"),
    }
}

#[test]
fn captured_model_conversion_matches_actual_o_r09() {
    for case in oracle()["models"].as_array().unwrap() {
        let model = decode_model(&case["input"]).unwrap();
        if case["expected"].is_null() {
            assert!(model.is_none());
            continue;
        }
        let model = model.unwrap();
        for (key, expected) in case["expected"].as_object().unwrap() {
            // READ_ weights are added to their base; use an unrelated write source
            // when inspecting a literal key so each observation stays isolated.
            let score = model.score(&[(key.as_str(), 1.0)], "share-observation");
            assert_eq!(score, expected.as_f64().unwrap(), "{case} / {key}");
        }
    }
}

#[test]
fn captured_offline_defaults_indexed_features_and_metadata_match_zb_k1() {
    for case in oracle()["offline"].as_array().unwrap() {
        let offline = Offline::decode(&case["input"]).unwrap();
        let expected = &case["expected"];
        for kind in ["member", "channel", "mpim", "ims"] {
            let entity = entity(kind);
            assert_eq!(
                offline.entity_ctr(entity),
                expected["ctr"][kind].as_f64().unwrap()
            );
            for (id, value) in expected["features"][kind].as_object().unwrap() {
                let found = offline.features(id, entity);
                if value["missing"] == true {
                    assert!(found.is_none());
                } else {
                    let found = found.unwrap();
                    assert_eq!(
                        found.team_ctr_90dd,
                        value["value"]["team_ctr_90dd"].as_f64().unwrap()
                    );
                    assert_eq!(
                        found.user_ctr_30dd,
                        value["value"]["user_ctr_30dd"].as_f64().unwrap()
                    );
                }
            }
        }
        for key in ["dm", "mpdm"] {
            for (id, value) in expected[key].as_object().unwrap() {
                let found = if key == "dm" {
                    offline.dm(id)
                } else {
                    offline.mpdm(id)
                };
                if value["missing"] == true {
                    assert!(found.is_none());
                } else {
                    assert_eq!(found, Some(&value["value"]));
                }
            }
        }
        for (id, value) in expected["affinity"].as_object().unwrap() {
            assert_eq!(offline.affinity(id), value.as_f64().unwrap());
        }
        for (id, value) in expected["priority"].as_object().unwrap() {
            assert_eq!(offline.priority(id), value.as_f64().unwrap());
        }
    }
}

#[test]
fn malformed_model_is_refused_instead_of_becoming_an_absent_model() {
    for raw in [
        json!({"A":"NaN"}),
        json!({"A":"Infinity"}),
        json!({"A":{}}),
        json!({"A":[1,2]}),
        json!({"A":"\u{0085}1"}),
        json!({"A":"+0x10"}),
        json!({"a":1,"A":2}),
        json!({"\u{0131}":1}),
        json!(true),
        json!([]),
    ] {
        assert_eq!(
            decode_model(&json!({"ok":true,"model":raw})).unwrap_err(),
            Error::Model
        );
    }
    assert_eq!(
        decode_model(&json!({"ok":false})).unwrap_err(),
        Error::Response
    );
    assert_eq!(decode_model(&Value::Null).unwrap_err(), Error::Response);
}

#[test]
fn malformed_offline_fields_do_not_silently_become_cold_defaults() {
    for (key, value) in [
        ("channel_ctr", json!(".5")),
        ("channel_features", json!(false)),
        ("top_channels", json!([])),
        ("users_has_dmed", json!([])),
        ("user_affinity", json!({"U1":"5"})),
        ("top_channels", json!({"C1":true})),
    ] {
        let mut raw = json!({"ok":true});
        raw[key] = value;
        assert_eq!(Offline::decode(&raw).unwrap_err(), Error::Offline);
    }
    assert_eq!(
        Offline::decode(&json!({"ok":false})).unwrap_err(),
        Error::Response
    );
}

#[test]
fn cold_and_observed_empty_map_have_distinct_metadata_absence() {
    let cold = Offline::default();
    assert_eq!(cold.dm("U1"), Some(&Value::Null));
    let empty = Offline::decode(&json!({"ok":true,"users_has_dmed":{}})).unwrap();
    assert_eq!(empty.dm("U1"), None);
    assert_eq!(cold.entity_ctr(Entity::Im), 0.179);
    assert_eq!(cold.features("D1", Entity::Im), None);
}

#[test]
fn requests_are_closed_and_use_existing_workspace_transport_shape() {
    let client = SlackClient::default();
    let mut workspace = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com".into(),
        token: "synthetic-token".into(),
    };
    for enterprise in [false, true] {
        workspace.enterprise_id = enterprise.then(|| "E00000001".into());
        for (request, endpoint, reason) in [
            (
                model_request(&client, &workspace),
                "search.autocomplete.model",
                "AutocompleteModelCache",
            ),
            (
                offline_request(&client, &workspace),
                "search.autocomplete.offlineFeatures",
                "search-offline-features",
            ),
        ] {
            assert_eq!(request.method, "POST");
            let host = if enterprise {
                "synthetic.enterprise.slack.com"
            } else {
                "synthetic.slack.com"
            };
            assert_eq!(
                request.url.split('?').next().unwrap(),
                format!("https://{host}/api/{endpoint}")
            );
            let crate::slack::client::RequestBody::Form(form) = request.body else {
                panic!("expected form")
            };
            assert_eq!(
                form,
                vec![
                    ("token".into(), "synthetic-token".into()),
                    ("_x_reason".into(), reason.into())
                ]
            );
        }
    }
}

#[test]
fn contradictory_success_envelopes_are_never_model_or_offline_observations() {
    for error in [
        json!("ratelimited"),
        json!(false),
        json!(true),
        json!({}),
        json!([]),
        json!(""),
        json!(0),
    ] {
        let raw = json!({"ok":true,"error":error,"model":{"FUZZY_SCORE":2},"channel_ctr":0.4});
        assert_eq!(decode_model(&raw).unwrap_err(), Error::Response);
        assert_eq!(Offline::decode(&raw).unwrap_err(), Error::Response);
    }
    let raw = json!({"ok":true,"error":null,"model":{"FUZZY_SCORE":2},"channel_ctr":0.4});
    assert_eq!(
        decode_model(&raw)
            .unwrap()
            .unwrap()
            .score(&[("FUZZY_SCORE", 1.0)], ""),
        2.0
    );
    assert_eq!(
        Offline::decode(&raw).unwrap().entity_ctr(Entity::Channel),
        0.4
    );
}
