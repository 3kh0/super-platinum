use super::*;
use crate::slack::client::RequestBody;
#[test]
fn captured_presets_use_exact_hour_and_ceiled_local_tomorrow_minutes() {
    let now = DateTime::parse_from_rfc3339("2026-10-13T12:00:17-04:00").unwrap();
    let empty = BTreeMap::new();
    assert_eq!(
        TemporaryMuteDuration::OneHour
            .expires_at(now, &empty)
            .unwrap(),
        now.timestamp() + 3600
    );
    // Tomorrow 09:00:00 minus today's 12:00:17 is rounded up in minutes,
    // then added to now, preserving the current seconds component.
    assert_eq!(
        TemporaryMuteDuration::UntilTomorrow
            .expires_at(now, &empty)
            .unwrap(),
        now.timestamp() + 21 * 3600
    );
    let custom = serde_json::from_value(json!({"dnd_days":"custom","dnd_enabled_wednesday":"partial","dnd_before_wednesday":"10:30","dnd_end_hour":"08:00"})).unwrap();
    assert_eq!(
        TemporaryMuteDuration::UntilTomorrow
            .expires_at(now, &custom)
            .unwrap(),
        now.timestamp() + 22 * 3600 + 30 * 60
    );
    let fallback = serde_json::from_value(json!({"dnd_days":"custom","dnd_enabled_wednesday":"disabled","dnd_before_wednesday":"10:30","dnd_end_hour":"08:00"})).unwrap();
    assert_eq!(
        TemporaryMuteDuration::UntilTomorrow
            .expires_at(now, &fallback)
            .unwrap(),
        now.timestamp() + 20 * 3600
    );
    let malformed = serde_json::from_value(json!({"dnd_end_hour":"25:00"})).unwrap();
    assert!(
        TemporaryMuteDuration::UntilTomorrow
            .expires_at(now, &malformed)
            .is_err()
    );
    assert!(
        TemporaryMuteDuration::OneHour
            .expires_at_fixed_offset(i64::MAX, 0, &empty)
            .is_err()
    );
}
#[test]
fn temporary_patch_writes_only_the_exact_expiry_string() {
    let workspace = super::super::tests::workspace();
    let request = channel_mute_change_request(
        &SlackClient::default(),
        &workspace,
        "C1",
        ChannelMuteChange::TemporaryUntil(123456),
    )
    .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(fields.len(), 5);
    assert_eq!(fields["global"], "false");
    assert_eq!(fields["sync"], "false");
    assert_eq!(fields["channel_ids"], "C1");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["prefs"]).unwrap(),
        json!([{"name":"temp_mute_expiration_ts","value":"123456"}])
    );
    assert!(
        channel_mute_change_request(
            &SlackClient::default(),
            &workspace,
            "C1",
            ChannelMuteChange::TemporaryUntil(0)
        )
        .is_err()
    );
}
#[test]
fn canonical_expiry_identity_cannot_be_replaced_by_boolean_mute_proof() {
    let change = ChannelMuteChange::TemporaryUntil(500);
    let mut canonical = ChannelMuteState {
        muted: false,
        temp_mute_expiration_ts: Some(501),
    };
    assert!(!change.confirms(canonical, 100));
    canonical.temp_mute_expiration_ts = Some(500);
    assert!(change.confirms(canonical, 100));
    // A later explicit check can prove the saved deadline even after it elapsed.
    assert!(change.confirms(canonical, 501));
    canonical.muted = true;
    assert!(!change.confirms(canonical, 100));
    assert!(!ChannelMuteChange::Permanent(false).confirms(canonical, 501));
    canonical.muted = false;
    assert!(!ChannelMuteChange::Permanent(false).confirms(canonical, 100));
    assert!(ChannelMuteChange::Permanent(false).confirms(canonical, 501));
}
