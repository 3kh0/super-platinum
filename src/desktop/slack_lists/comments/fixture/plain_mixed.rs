//! Exact closed mixed same-label ambiguity, using an actual captured visit clock.
use super::*;
use std::{
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_MIXED_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain-mixed-tie" | "own-edit-plain-mixed-group")
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default()
}
fn group_wins() -> bool {
    case() == "own-edit-plain-mixed-group"
}
fn captured_ms() -> u64 {
    static TIME: OnceLock<u64> = OnceLock::new();
    *TIME.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Offline fixture clock predates epoch")
            .as_millis()
            .try_into()
            .expect("Offline fixture clock exceeds u64")
    })
}
fn history() -> String {
    // This is an authored ordered STRING, not serialization of an unordered map.
    // History::weight consumes milliseconds; latest() alone exposes seconds.
    let time = captured_ms();
    let count = if group_wins() { 10 } else { 1 };
    format!(
        "{{\"harbor\":[{{\"id\":\"ULISTMEM1\",\"count\":1,\"visits\":[{time}]}},{{\"id\":\"SLISTGRP1\",\"count\":{count},\"visits\":[{time}]}}]}}"
    )
}
pub(crate) fn apply_preferences(raw: &mut Value) {
    if named() {
        raw["prefs"]["frecency"] = json!(history());
    }
}
pub(crate) fn snapshot() -> Value {
    if !named() {
        return json!({"enabled":false});
    }
    json!({"enabled":true,"case":case(),"capturedAtMillis":captured_ms(),"historyWire":history(),
        "clock":"SystemTime::now captured once by the offline fixture","noLiveNetwork":true})
}
pub(crate) fn future() -> &'static str {
    if group_wins() {
        include_str!("plain/mixed-group-future.json")
    } else {
        include_str!("plain/mixed-tie-future.json")
    }
}
