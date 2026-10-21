use super::*;
#[test]
fn readback_is_explicit_and_original_received_row_is_immutable() {
    let received = received_message();
    let mut later = received.clone();
    later["text"] = json!("Later canonical body");
    let mut state = Some(State {
        received: received.clone(),
        candidates: vec![later.clone()],
        ..Default::default()
    });
    let original = json!({"ok":true,"messages":[{"ts":ROOT},received.clone()]});
    let mut before = original.clone();
    decorate(&state, &mut before);
    assert_eq!(before, original);
    state.as_mut().unwrap().published = Some(later.clone());
    let mut after = original.clone();
    decorate(&state, &mut after);
    assert_eq!(after["messages"][1], later);
    assert_eq!(state.as_ref().unwrap().received, received);
    assert_eq!(
        original["messages"][1]["text"],
        "Original own comment 中🙂.\nSecond editable line."
    );
}
#[test]
fn update_request_equality_includes_headers_query_and_every_body_field() {
    let client = super_platinum_core::slack::SlackClient::default();
    let session = super::super::session();
    let request = client.rest_form(
        &session,
        "chat.update",
        vec![("channel", "CCOMMENT01".into())],
    );
    assert!(same(&request, &request));
    let independent = client.rest_form(
        &session,
        "chat.update",
        vec![("channel", "CCOMMENT01".into())],
    );
    assert_ne!(request.url, independent.url);
    assert!(same(&request, &independent));
    let mut altered = request.clone();
    altered.headers.clear();
    assert!(!same(&request, &altered));
    altered = request.clone();
    altered.url.push_str("&unexpected=1");
    assert!(!same(&request, &altered));
    altered = request.clone();
    if let RequestBody::Form(fields) = &mut altered.body {
        fields.push(("text".into(), "Unapproved fallback".into()));
    }
    assert!(!same(&request, &altered));
}

#[test]
fn trace_normalization_refuses_duplicates_missing_fields_and_malformed_ids() {
    let client = super_platinum_core::slack::SlackClient::default();
    let request = client.rest_form(&super::super::session(), "chat.update", vec![]);
    for key in ["_x_id", "_x_csid", "fp"] {
        let mut duplicate = request.clone();
        duplicate
            .url
            .push_str(&format!("&{key}=00112233445546778899aabbccddeeff"));
        assert!(!same(&request, &duplicate), "duplicate {key}");
        for invalid in [
            "",
            "wrong",
            "00112233445546778899AABBCCDDEEFF",
            "00112233445516778899aabbccddeeff",
            "00112233445546771899aabbccddeeff",
            "%ZZ",
        ] {
            let mut changed = request.clone();
            let mut url = url::Url::parse(&request.url).unwrap();
            let pairs = url
                .query_pairs()
                .map(|(k, v)| {
                    let v = if k == key {
                        invalid.to_owned()
                    } else {
                        v.into_owned()
                    };
                    (k.into_owned(), v)
                })
                .collect::<Vec<_>>();
            url.set_query(None);
            url.query_pairs_mut().extend_pairs(pairs);
            changed.url = url.into();
            assert!(!same(&request, &changed), "malformed {key}: {invalid}");
        }
        let mut missing = request.clone();
        let mut url = url::Url::parse(&request.url).unwrap();
        let pairs = url
            .query_pairs()
            .filter(|(k, _)| k != key)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        url.set_query(None);
        url.query_pairs_mut().extend_pairs(pairs);
        missing.url = url.into();
        assert!(!same(&request, &missing), "missing {key}");
    }
    for suffix in ["&unexpected=1", "&_x_gantry=true", "#fragment"] {
        let mut changed = request.clone();
        changed.url.push_str(suffix);
        assert!(!same(&request, &changed));
    }
    let mut changed = request.clone();
    changed.url = changed.url.replace("_x_num_retries=0", "_x_num_retries=1");
    assert!(!same(&request, &changed));
}
