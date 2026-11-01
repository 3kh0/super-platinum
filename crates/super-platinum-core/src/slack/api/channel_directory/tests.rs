use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

fn query() -> Query {
    Query {
        text: String::new(),
        scope: Scope::Joined,
        kind: Kind::Active,
        sort: Sort::Name,
        page: 1,
        cursor: None,
    }
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Offline fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn raw() -> serde_json::Value {
    json!({"ok":true,"items":[{"id":"C1","name":"release-notes","purpose":"Release planning","member_count":42,"is_member":true}],"pagination":{"page":1,"page_count":3,"total_count":120,"next_cursor":"next-page"}})
}

#[test]
fn ordinary_channel_recipe_keeps_joined_scope_and_distinguishes_browse_from_text() {
    let client = SlackClient::default();
    let mut q = query();
    let prepared = request(&client, &workspace(), &q).unwrap();
    assert!(prepared.url.contains("/api/search.modules.channels?"));
    let RequestBody::Form(fields) = prepared.body else {
        panic!("ordinary native form")
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    for (key, value) in [
        ("module", "channels"),
        ("count", "50"),
        ("search_only_my_channels", "true"),
        ("channel_type", "exclude_archived"),
        ("browse", "standard"),
        ("highlight", "0"),
        ("extracts", "0"),
        ("extra_message_data", "0"),
        ("sort", "name"),
        ("sort_dir", "asc"),
    ] {
        assert_eq!(fields.get(key).map(String::as_str), Some(value), "{key}");
    }
    assert!(!fields.contains_key("cursor") && !fields.contains_key("max_extract_len"));
    assert!(!fields.contains_key("recommend_source"));
    q.text = "  release plans  ".into();
    q.page = 2;
    q.cursor = Some("exact-cursor".into());
    q.sort = Sort::MostMembers;
    let RequestBody::Form(fields) = request(&client, &workspace(), &q).unwrap().body else {
        panic!()
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fields["query"], "release plans");
    assert_eq!(fields["cursor"], "exact-cursor");
    assert_eq!(fields["page"], "2");
    assert_eq!(fields["highlight"], "1");
    assert_eq!(fields["extracts"], "1");
    assert_eq!(fields["max_extract_len"], "200");
    assert_eq!(fields["sort"], "member_count");
    assert!(!fields.contains_key("browse"));
    q.sort = Sort::Relevant;
    let RequestBody::Form(fields) = request(&client, &workspace(), &q).unwrap().body else {
        panic!()
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fields["sort"], "relevant");
    assert_eq!(fields["sort_dir"], "desc");
    q.page = 0;
    assert!(request(&client, &workspace(), &q).is_err());
    q.page = 2;
    q.cursor = Some("bad\ncursor".into());
    assert!(request(&client, &workspace(), &q).is_err());
}

#[test]
fn metadata_page_does_not_invent_membership_or_accept_ambiguous_rows() {
    let q = query();
    let page = decode(raw(), &q).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name.as_deref(), Some("release-notes"));
    assert_eq!(page.items[0].purpose.as_deref(), Some("Release planning"));
    assert_eq!(page.items[0].member_count, Some(42));
    assert_eq!(page.next_cursor.as_deref(), Some("next-page"));
    assert_eq!((page.total, page.pages), (120, 3));
    let mut zero_total = raw();
    zero_total["pagination"]["total_count"] = 0.into();
    assert_eq!(decode(zero_total, &q).unwrap().total, 1);
    for patch in [
        json!({"is_member":false}),
        json!({"is_member":null}),
        json!({"is_member":"true"}),
        json!({"id":""}),
        json!({"member_count":-1}),
    ] {
        let mut value = raw();
        value["items"][0]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(decode(value, &q).is_err());
    }
    let mut value = raw();
    let duplicate = value["items"][0].clone();
    value["items"].as_array_mut().unwrap().push(duplicate);
    assert!(decode(value, &q).is_err());
    let mut value = raw();
    value["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("is_member");
    let mut accessible = q.clone();
    accessible.scope = Scope::Accessible;
    assert_eq!(decode(value, &accessible).unwrap().items[0].is_member, None);
    for value in [
        json!({"ok":false,"error":"not_allowed"}),
        json!({"ok":true,"error":"opaque-private-error","items":[]}),
        json!({"ok":true,"items":{}}),
    ] {
        assert!(decode(value, &q).is_err());
    }
}

#[test]
fn pagination_matches_request_rejects_cursor_replay_and_stops_on_empty_items() {
    let mut q = query();
    q.page = 2;
    q.cursor = Some("previous-page".into());
    assert!(
        decode(raw(), &q).is_err(),
        "wrong page is not this request's result"
    );
    let mut value = raw();
    value["pagination"]["page"] = 2.into();
    assert!(decode(value.clone(), &q).is_ok());
    value["pagination"]["next_cursor"] = "previous-page".into();
    assert!(
        decode(value.clone(), &q).is_err(),
        "replayed cursor cannot create an unbounded loop"
    );
    value["items"] = json!([]);
    assert!(
        decode(value, &q).unwrap().next_cursor.is_none(),
        "Source empty-page cursor termination is retained"
    );
    let mut value = raw();
    value["pagination"]["next_cursor"] = 7.into();
    assert!(decode(value, &query()).is_err());
}

#[test]
fn explicit_channel_types_forward_original_literals_without_leaving_joined_scope() {
    for (kind, literal) in [
        (Kind::Active, "exclude_archived"),
        (Kind::Public, "private_exclude"),
        (Kind::Private, "private"),
        (Kind::Archived, "archived"),
    ] {
        for text in ["", " release plans "] {
            let mut q = query();
            q.kind = kind;
            q.text = text.into();
            q.page = 2;
            q.cursor = Some("original-kind-page".into());
            let RequestBody::Form(fields) = request(&SlackClient::default(), &workspace(), &q)
                .unwrap()
                .body
            else {
                panic!()
            };
            let fields = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(fields["channel_type"], literal);
            assert_eq!(fields["search_only_my_channels"], "true");
            assert_eq!(fields["page"], "2");
            assert_eq!(fields["cursor"], "original-kind-page");
            let mut value = raw();
            value["pagination"]["page"] = 2.into();
            assert!(decode(value.clone(), &q).is_ok());
            value["items"][0]["is_member"] = false.into();
            assert!(
                decode(value, &q).is_err(),
                "a type filter cannot relax the joined-row requirement"
            );
        }
    }
}

#[test]
fn shared_channels_and_standard_creation_member_sorts_keep_explicit_joined_recipe() {
    for (sort, field, direction) in [
        (Sort::FewestMembers, "member_count", "asc"),
        (Sort::CreatedNewest, "created", "desc"),
        (Sort::CreatedOldest, "created", "asc"),
    ] {
        for text in ["", " release "] {
            let mut q = query();
            q.kind = Kind::Shared;
            q.sort = sort;
            q.text = text.into();
            q.page = 2;
            q.cursor = Some("shared-original-page".into());
            let RequestBody::Form(fields) = request(&SlackClient::default(), &workspace(), &q)
                .unwrap()
                .body
            else {
                panic!()
            };
            let fields = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(fields["channel_type"], "external_shared");
            assert_eq!(fields["sort"], field);
            assert_eq!(fields["sort_dir"], direction);
            assert_eq!(fields["search_only_my_channels"], "true");
            assert_eq!(fields["page"], "2");
            assert_eq!(fields["cursor"], "shared-original-page");
            assert_eq!(fields["count"], "50");
            assert!(
                !fields.contains_key("external_shared_team_ids")
                    && !fields.contains_key("recommend_source")
            );
            let mut value = raw();
            value["pagination"]["page"] = 2.into();
            assert!(decode(value.clone(), &q).is_ok());
            value["items"][0]["is_member"] = false.into();
            assert!(
                decode(value, &q).is_err(),
                "shared metadata never relaxes native joined membership"
            );
        }
    }
}
