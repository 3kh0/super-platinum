use super::*;
#[test]
fn permissions_reject_every_unowned_entry_and_cold_overlap() {
    let good = |protected, fixture, base, flag, name, case, cold| {
        permitted(protected, fixture, base, flag, name, case, cold)
    };
    for name in [
        "canvas-canonical-file-custody",
        "canvas-canonical-file-custody-narrow",
    ] {
        for case in ["success", "retirement"] {
            assert!(good(
                true,
                true,
                Some("1"),
                Some("1"),
                Some(name),
                Some(case),
                None
            ));
            assert!(!good(
                false,
                true,
                Some("1"),
                Some("1"),
                Some(name),
                Some(case),
                None
            ));
            assert!(!good(
                true,
                false,
                Some("1"),
                Some("1"),
                Some(name),
                Some(case),
                None
            ));
            assert!(!good(
                true,
                true,
                Some("1"),
                Some("1"),
                Some(name),
                Some(case),
                Some("1")
            ));
        }
    }
    for name in [
        None,
        Some("canvas-canonical-native-list"),
        Some("canvas-canonical-file-custody-extra"),
    ] {
        assert!(!good(
            true,
            true,
            Some("1"),
            Some("1"),
            name,
            Some("success"),
            None
        ));
    }
    for value in [None, Some(""), Some("true"), Some("0")] {
        assert!(!good(
            true,
            true,
            value,
            Some("1"),
            Some("canvas-canonical-file-custody"),
            Some("success"),
            None
        ));
        assert!(!good(
            true,
            true,
            Some("1"),
            value,
            Some("canvas-canonical-file-custody"),
            Some("success"),
            None
        ));
        assert!(!good(
            true,
            true,
            Some("1"),
            Some("1"),
            Some("canvas-canonical-file-custody"),
            value,
            None
        ));
    }
}
#[test]
fn identity_has_no_live_or_foreign_capability() {
    assert!(identity(
        Some(ACCOUNT),
        Some("T1"),
        "U0",
        false,
        false,
        Some("Ffixture")
    ));
    for (account, team, user, session, transport, canvas) in [
        (None, Some("T1"), "U0", false, false, Some("Ffixture")),
        (
            Some("foreign"),
            Some("T1"),
            "U0",
            false,
            false,
            Some("Ffixture"),
        ),
        (
            Some(ACCOUNT),
            Some("T2"),
            "U0",
            false,
            false,
            Some("Ffixture"),
        ),
        (
            Some(ACCOUNT),
            Some("T1"),
            "other",
            false,
            false,
            Some("Ffixture"),
        ),
        (
            Some(ACCOUNT),
            Some("T1"),
            "U0",
            true,
            false,
            Some("Ffixture"),
        ),
        (
            Some(ACCOUNT),
            Some("T1"),
            "U0",
            false,
            true,
            Some("Ffixture"),
        ),
        (
            Some(ACCOUNT),
            Some("T1"),
            "U0",
            false,
            false,
            Some("foreign"),
        ),
    ] {
        assert!(!identity(account, team, user, session, transport, canvas));
    }
}
#[test]
fn exact_known_factory_rejects_absent_and_navigation_requests() {
    let client = SlackClient::default();
    let session = session();
    let req =
        super::super::super::super::file_acquisition::known_request(&client, &session, HOSTED);
    assert_eq!(
        protocol::validate(&client, &session, &req, HOSTED).unwrap(),
        json!({"file":HOSTED,"page":"1","count":"500","truncate":"true","_x_reason":"files-store.ConditionalFetchManager.fetch"})
    );
    for req in [
        super::super::super::super::file_acquisition::request(&client, &session, HOSTED),
        super_platinum_core::slack::api::file_info(&client, &session, HOSTED),
    ] {
        assert!(protocol::validate(&client, &session, &req, HOSTED).is_err());
    }
    for extra in ["skip_shares", "file", "extra"] {
        let mut bad = req.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!("form")
        };
        fields.push((extra.into(), "true".into()));
        assert!(protocol::validate(&client, &session, &bad, HOSTED).is_err());
    }
}
#[test]
fn original_and_successor_raw_responses_are_distinct_without_typed_defaults() {
    let first = response(Mode::Held, HOSTED, 1);
    let second = response(Mode::Success, HOSTED, 2);
    assert_eq!(first["file"]["id"], HOSTED);
    assert_eq!(second["file"]["id"], HOSTED);
    assert_ne!(first["file"]["name"], second["file"]["name"]);
    assert_ne!(first["file"]["permalink"], second["file"]["permalink"]);
    assert_eq!(first["file"]["updated"], 201);
    assert_eq!(second["file"]["updated"], 202);
    for raw in [&first, &second] {
        assert_eq!(
            raw["file"]["fixture_opaque"],
            file(HOSTED)["fixture_opaque"]
        );
        assert!(raw["file"].get("channel_map").is_none());
    }
    assert_eq!(
        response(Mode::Malformed, HOSTED, 1),
        json!({"ok":true,"file":false})
    );
    assert_eq!(
        response(Mode::Error, HOSTED, 1),
        json!({"ok":false,"error":"file_not_found"})
    );
}
#[tokio::test]
async fn dropped_original_receiver_cannot_deliver_or_consume_successor() {
    let (a, ar) = oneshot::channel();
    let (b, br) = oneshot::channel();
    drop(ar);
    assert!(a.is_closed());
    assert!(a.send(response(Mode::Held, HOSTED, 1)).is_err());
    assert!(!b.is_closed());
    assert!(b.send(response(Mode::Held, HOSTED, 2)).is_ok());
    assert_eq!(br.await.unwrap()["file"]["updated"], 202);
}
