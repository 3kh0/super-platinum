use super::*;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
const CHANNEL: &str = "CLIST0001";
const A: &str = "ULIST0001";
const B: &str = "ULIST0002";
fn read() -> Read {
    Read::membership(&owner(), CHANNEL, vec![A.into(), B.into()]).unwrap()
}
fn response(read: &Read) -> Membership {
    Membership::decode(
        read,
        &owner(),
        json!({"members":[A],"non_members":[B],"opaque":{"keep":true}}),
    )
    .unwrap()
}
#[test]
fn source_first_thirty_and_membership_bodies_are_exact() {
    let first = Read::first_members(&owner(), CHANNEL).unwrap();
    assert_eq!(first.method(), "users/list");
    assert_eq!(
        first.body(),
        json!({"channels":[CHANNEL],"present_first":true,
        "filter":"everyone AND NOT bots AND NOT apps","count":30})
    );
    let membership = read();
    assert_eq!(membership.method(), "channels/membership");
    assert_eq!(
        membership.body(),
        json!({"channel":CHANNEL,"users":[A,B],"as_admin":false})
    );
}
#[test]
fn original_first_rows_order_duplicates_and_raw_are_retained() {
    let read = Read::first_members(&owner(), CHANNEL).unwrap();
    let raw =
        json!({"results":[{"id":B,"opaque":1},{"id":A},{"id":B,"opaque":2}],"next_marker":"more"});
    let rows = FirstMembers::decode(&read, &owner(), raw.clone(), None).unwrap();
    assert_eq!(rows.raw(), &raw);
    assert_eq!(rows.rows(), raw["results"].as_array().unwrap());
    assert_eq!(rows.ids(), [B, A]);
    assert_eq!(rows.channel(), CHANNEL);
}
#[test]
fn empty_first_page_never_claims_known_empty_membership() {
    let first = Read::first_members(&owner(), CHANNEL).unwrap();
    let rows = FirstMembers::decode(&first, &owner(), json!({"results":[]}), None).unwrap();
    assert!(rows.ids().is_empty());
    assert!(Read::membership(&owner(), CHANNEL, vec![]).is_err());
    assert_eq!(
        Cache::capture(&owner(), CHANNEL).unwrap().entry(A),
        Entry::Unknown
    );
}
#[test]
fn first_members_refuses_foreign_read_invalid_rows_and_unordered_objects() {
    let first = Read::first_members(&owner(), CHANNEL).unwrap();
    for raw in [
        json!({"results":[{"id":"FNOTUSER1"}]}),
        json!({"results":vec![json!({"id":A});31]}),
        json!({"results":{"one":{"id":A}}}),
        json!({"ok":false,"results":[]}),
    ] {
        assert!(FirstMembers::decode(&first, &owner(), raw, None).is_err());
    }
    assert!(FirstMembers::decode(&read(), &owner(), json!({"results":[]}), None).is_err());
    let keys = vec!["two".into(), "one".into()];
    let page = FirstMembers::decode(
        &first,
        &owner(),
        json!({"results":{"one":{"id":A},"two":{"id":B}}}),
        Some(&keys),
    )
    .unwrap();
    assert_eq!(page.ids(), [B, A]);
}
#[test]
fn membership_exact_partition_rejects_foreign_missing_overlap_and_error() {
    for raw in [
        json!({"members":[A]}),
        json!({"members":[A,B,"UOTHER001"]}),
        json!({"members":[A,B],"non_members":[B]}),
        json!({"members":[A,B],"channel":"COTHER001"}),
        json!({"members":[A,B],"ok":false}),
        json!({"members":[A,B],"error":"denied"}),
        json!({"members":{A:true,B:false}}),
        json!({"members":[A, false]}),
    ] {
        assert!(Membership::decode(&read(), &owner(), raw).is_err());
    }
    let original = response(&read());
    assert_eq!(
        original.values(),
        &BTreeMap::from([(A.into(), true), (B.into(), false)])
    );
    assert_eq!(original.raw()["opaque"], json!({"keep":true}));
}
#[test]
fn unknown_pending_negative_positive_remain_distinct() {
    let read = read();
    let mut cache = Cache::capture(&owner(), CHANNEL).unwrap();
    assert_eq!(cache.entry(A), Entry::Unknown);
    cache.begin(&read).unwrap();
    assert_eq!(cache.entry(A), Entry::Unknown);
    cache.accept(&response(&read)).unwrap();
    assert_eq!(cache.entry(A), Entry::Known(true));
    assert_eq!(cache.entry(B), Entry::Known(false));
    assert!(cache.bonus(A));
    assert!(!cache.bonus(B));
    assert!(!cache.bonus("UUNKNOWN1"));
}
#[test]
fn clear_then_late_response_does_not_resurrect_or_assign_false() {
    let read = read();
    let mut cache = Cache::capture(&owner(), CHANNEL).unwrap();
    cache.begin(&read).unwrap();
    cache.clear(&read).unwrap();
    cache.accept(&response(&read)).unwrap();
    assert_eq!(cache.entry(A), Entry::Unknown);
    assert_eq!(cache.entry(B), Entry::Unknown);
    cache.accept(&response(&read)).unwrap();
    assert!(cache.entries.is_empty());
}
#[test]
fn owner_and_channel_replacement_refuse_without_mutating_cache() {
    let read = read();
    for field in 0..6 {
        let mut other = owner();
        match field {
            0 => other.team_id = "TOTHER001".into(),
            1 => other.user_id = "UOTHER001".into(),
            2 => other.token = "other".into(),
            3 => other.url = "https://other.test".into(),
            4 => other.enterprise_id = Some("EOTHER001".into()),
            _ => other.name = "Other".into(),
        }
        assert!(!read.current(&other));
        assert!(Membership::decode(&read, &other, json!({"members":[A,B]})).is_err());
        let mut cache = Cache::capture(&other, CHANNEL).unwrap();
        assert!(cache.begin(&read).is_err());
        assert!(cache.accept(&response(&read)).is_err());
        assert!(cache.entries.is_empty());
    }
    let mut cache = Cache::capture(&owner(), "COTHER001").unwrap();
    assert!(cache.begin(&read).is_err());
    assert!(cache.clear(&read).is_err());
}
#[test]
fn request_bounds_and_member_ids_are_defensive_not_partial_requests() {
    for ids in [
        vec![A.into(), A.into()],
        vec!["FNOTUSER1".into()],
        vec![A.into(); 101],
    ] {
        assert!(Read::membership(&owner(), CHANNEL, ids).is_err());
    }
    assert!(Read::first_members(&owner(), "FLIST0001").is_err());
    assert!(Read::first_members(&owner(), "").is_err());
}

#[test]
fn typed_endpoint_preserves_observed_route_owner_and_expiry() {
    use crate::{
        block_actions::directory_endpoint::DirectoryEndpoint,
        slack::{SlackClient, client::RequestBody},
    };
    use std::time::{Duration, Instant};
    let mut owner = owner();
    owner.enterprise_id = Some("E00000001".into());
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &owner,
        &json!({"ok":true,"ttl_seconds":60,
        "url":"https://edgeapi.slack.com/cache/E00000001"}),
        now,
    )
    .unwrap();
    let client = SlackClient::default();
    for read in [
        Read::first_members(&owner, CHANNEL).unwrap(),
        Read::membership(&owner, CHANNEL, vec![A.into()]).unwrap(),
    ] {
        let request = endpoint
            .list_membership(&client, &owner, now, &read)
            .unwrap();
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/cache/E00000001/{}", read.method())
        );
        let RequestBody::Json(mut raw) = request.body else {
            panic!("JSON")
        };
        assert_eq!(
            raw.as_object_mut().unwrap().remove("token"),
            Some(json!("synthetic"))
        );
        assert_eq!(raw, read.body());
        assert!(
            endpoint
                .list_membership(&client, &owner, now + Duration::from_secs(58), &read)
                .is_err()
        );
        let mut other = owner.clone();
        other.token = "replacement".into();
        assert!(
            endpoint
                .list_membership(&client, &other, now, &read)
                .is_err()
        );
        assert!(
            endpoint
                .request(&client, &owner, now, read.method(), read.body())
                .is_err()
        );
    }
}
