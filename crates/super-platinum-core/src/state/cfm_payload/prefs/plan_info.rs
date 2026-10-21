//! n4D5 shallow merge. Action admission belongs to the original payload owner.
use super::*;

pub(super) fn apply(state: &mut Option<Value>, payload: &Value) -> Result<(), Error> {
    let incoming = payload
        .as_object()
        .ok_or(Error::Malformed("plan info object"))?;
    let mut next = match state {
        Some(raw) => raw
            .as_object()
            .ok_or(Error::Malformed("stored plan info object"))?
            .clone(),
        None => json!({"is_plan_frozen":false,"reconnect_grace_period_start_ts":0})
            .as_object()
            .unwrap()
            .clone(),
    };
    next.extend(incoming.clone());
    *state = Some(Value::Object(next));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(raw: Value) -> Effect {
        Effect {
            kind: Kind::PlanInfo,
            payload: raw,
        }
    }
    #[test]
    fn first_actual_action_initializes_defaults_and_preserves_opaque_values() {
        let mut stores = Stores::default();
        assert_eq!(stores.plan_info, None);
        stores
            .apply(&action(
                json!({"opaque":[0,false,null],"plan_level":"paid"}),
            ))
            .unwrap();
        assert_eq!(
            stores.plan_info,
            Some(json!({"is_plan_frozen":false,
            "reconnect_grace_period_start_ts":0,"opaque":[0,false,null],"plan_level":"paid"}))
        );
    }
    #[test]
    fn shallow_merge_keeps_omitted_fields_and_explicit_null_overwrites() {
        let mut stores = Stores::default();
        stores
            .apply(&action(json!({"reconnect_grace_period_start_ts":42,
            "opaque":{"first":1,"second":2}})))
            .unwrap();
        stores
            .apply(&action(json!({"opaque":{"second":3}})))
            .unwrap();
        assert_eq!(
            stores.plan_info.as_ref().unwrap()["reconnect_grace_period_start_ts"],
            42
        );
        assert_eq!(
            stores.plan_info.as_ref().unwrap()["opaque"],
            json!({"second":3})
        );
        stores
            .apply(&action(json!({"reconnect_grace_period_start_ts":null})))
            .unwrap();
        assert!(stores.plan_info.as_ref().unwrap()["reconnect_grace_period_start_ts"].is_null());
        let before = stores.clone();
        stores.apply(&action(json!({}))).unwrap();
        assert_eq!(stores, before);
    }
    #[test]
    fn malformed_action_is_atomic_and_does_not_manufacture_observation() {
        for malformed in [Value::Null, json!(false), json!(4), json!("bad"), json!([])] {
            let mut stores = Stores::default();
            assert!(stores.apply(&action(malformed.clone())).is_err());
            assert_eq!(stores.plan_info, None);
            stores.apply(&action(json!({"opaque":true}))).unwrap();
            let before = stores.clone();
            assert!(stores.apply(&action(malformed)).is_err());
            assert_eq!(stores, before);
        }
    }
    #[test]
    fn old_cache_remains_unobserved_and_raw_state_round_trips() {
        let mut old = serde_json::to_value(Stores::default()).unwrap();
        old.as_object_mut().unwrap().remove("plan_info");
        let mut stores: Stores = serde_json::from_value(old).unwrap();
        assert_eq!(stores.plan_info, None);
        stores
            .apply(&action(
                json!({"reconnect_grace_period_start_ts":null,"unknown":[1,2]}),
            ))
            .unwrap();
        let restored: Stores =
            serde_json::from_value(serde_json::to_value(&stores).unwrap()).unwrap();
        assert_eq!(restored, stores);
    }
}
