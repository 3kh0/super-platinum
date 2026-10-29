use super::*;
use std::time::Duration;

#[test]
fn final_sdk_recipe_adds_only_true_and_preserves_legacy_owner_endpoint_and_marker_contracts() {
    for enterprise in [false, true] {
        let w = WorkspaceSession {
            team_id: if enterprise { "E00000001" } else { "T00000001" }.into(),
            enterprise_id: enterprise.then(|| "E00000001".into()),
            user_id: "U00000001".into(),
            name: "Fixture".into(),
            url: "https://fixture.test".into(),
            token: "synthetic".into(),
        };
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(
            &w,
            &json!({"ok":true,"ttl_seconds":60,
            "url":format!("https://edgeapi.slack.com/cache/{}",w.team_id)}),
            now,
        )
        .unwrap();
        let query = Query::capture(
            &w,
            " @Cafe\u{301}\0\n ",
            RequestFacts {
                top_users: Some(json!(["U2", "U1", "U2"])),
                default_workspace: Some("TOTHER001".into()),
            },
        )
        .unwrap();
        let client = SlackClient::default();
        for marker in [None, Some("next")] {
            let old = query.request(&client, &w, &endpoint, now, marker).unwrap();
            let new = query
                .request_for_profile_long_text(&client, &w, &endpoint, now, marker)
                .unwrap();
            assert_eq!(old.method, new.method);
            assert_eq!(old.headers, new.headers);
            assert_eq!(
                url::Url::parse(&old.url).unwrap().path(),
                url::Url::parse(&new.url).unwrap().path()
            );
            let (RequestBody::Json(old), RequestBody::Json(mut new)) = (old.body, new.body) else {
                panic!("JSON")
            };
            assert!(old.get("include_profile_only_users").is_none());
            assert_eq!(
                new.as_object_mut()
                    .unwrap()
                    .remove("include_profile_only_users"),
                Some(Value::Bool(true))
            );
            assert_eq!(old, new);
            assert_eq!(new["count"], 25);
            assert_eq!(new["query"], query.raw());
            assert_eq!(new["top_users"], json!(["U2", "U1", "U2"]));
        }
        for change in 0..6 {
            let mut other = w.clone();
            match change {
                0 => other.token.push('x'),
                1 => other.user_id.push('x'),
                2 => other.team_id.push('x'),
                3 => other.url.push('x'),
                4 => other.name.push('x'),
                _ => other.enterprise_id = Some("EREPLACED".into()),
            }
            assert!(
                query
                    .request_for_profile_long_text(&client, &other, &endpoint, now, None)
                    .is_err()
            );
        }
        assert!(
            query
                .request_for_profile_long_text(
                    &client,
                    &w,
                    &endpoint,
                    now + Duration::from_secs(61),
                    None
                )
                .is_err()
        );
        for marker in ["", "bad\n", &"x".repeat(4097)] {
            assert!(
                query
                    .request_for_profile_long_text(&client, &w, &endpoint, now, Some(marker))
                    .is_err()
            );
        }
    }
}
