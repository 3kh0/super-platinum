use super::*;
use serde_json::json;

#[test]
fn sharing_discovery_distinguishes_absent_boolean_and_malformed_profile_observations() {
    for (flag, allowed) in [
        (None, true),
        (Some(json!(false)), true),
        (Some(json!(true)), false),
        (Some(Value::Null), false),
        (Some(json!(0)), false),
        (Some(json!(1)), false),
        (Some(json!(-1)), false),
        (Some(json!("")), false),
        (Some(json!("false")), false),
        (Some(json!([])), false),
        (Some(json!({})), false),
    ] {
        let mut raw = json!({"id":"UCALPROFILE","name":"Directory person"});
        if let Some(flag) = &flag {
            raw["is_profile_only_user"] = flag.clone();
        }
        let user = serde_json::from_value(raw).unwrap();
        assert_eq!(sharing_profile_allowed(&user), allowed, "flag={flag:?}");
    }
}
