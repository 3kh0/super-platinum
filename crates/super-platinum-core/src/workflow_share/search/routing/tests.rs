use super::*;
use crate::workflow_share::search::Kind;

#[test]
fn share_search_routing_empty_and_whitespace_follow_wire_not_matcher() {
    for (raw, expected) in [
        ("", AcquisitionRoute::Local),
        (" ", AcquisitionRoute::Remote),
        ("\t\n", AcquisitionRoute::Remote),
        ("\u{3000}", AcquisitionRoute::Remote),
        (" @", AcquisitionRoute::Remote),
        ("@ ", AcquisitionRoute::Remote),
        ("review", AcquisitionRoute::Remote),
    ] {
        assert_eq!(
            Query::new(raw).unwrap().acquisition_route(true, true),
            expected,
            "{raw:?}"
        );
    }
}

#[test]
fn share_search_routing_prefix_only_keeps_enabled_family() {
    for (raw, kinds) in [
        ("@", vec![Kind::Members]),
        ("\u{ff20}", vec![Kind::Members]),
        ("#", vec![Kind::Channels]),
        ("\u{ff03}", vec![Kind::Channels]),
        ("/", vec![Kind::Members, Kind::Channels]),
        ("\u{ff0f}", vec![Kind::Members, Kind::Channels]),
    ] {
        let q = Query::new(raw).unwrap();
        assert_eq!(q.wire(), "");
        assert_eq!(q.kinds(), kinds);
        assert_eq!(q.acquisition_route(true, true), AcquisitionRoute::Local);
        assert_eq!(q.acquisition_route(true, false), AcquisitionRoute::Remote);
    }
    // Query::wire deliberately does not strip these prefixes.
    for raw in ["+", "\u{ff0b}", ":"] {
        assert_eq!(
            Query::new(raw).unwrap().acquisition_route(true, true),
            AcquisitionRoute::Remote
        );
    }
}

#[test]
fn share_search_routing_email_extraction_precedes_empty_decision() {
    for raw in ["@Ada@example.test", "# label <Ada@example.test>"] {
        let q = Query::new(raw).unwrap();
        assert_eq!(q.wire(), "Ada@example.test");
        assert_eq!(q.kinds(), [Kind::Members, Kind::Channels]);
        assert_eq!(q.acquisition_route(true, true), AcquisitionRoute::Remote);
    }
}

#[test]
fn share_search_routing_offline_dominates_without_changing_query() {
    for raw in ["", " ", "@", "#", "review", "Ada@example.test"] {
        let q = Query::new(raw).unwrap();
        let wire = q.wire().to_owned();
        let kinds = q.kinds().to_vec();
        for client_host in [false, true] {
            assert_eq!(
                q.acquisition_route(false, client_host),
                AcquisitionRoute::Local
            );
        }
        assert_eq!(q.raw(), raw);
        assert_eq!(q.wire(), wire);
        assert_eq!(q.kinds(), kinds);
    }
}

#[test]
fn share_search_routing_requires_explicit_host_for_online_empty() {
    let q = Query::new("").unwrap();
    assert_eq!(q.acquisition_route(true, false), AcquisitionRoute::Remote);
    assert_eq!(q.acquisition_route(true, true), AcquisitionRoute::Local);
}
