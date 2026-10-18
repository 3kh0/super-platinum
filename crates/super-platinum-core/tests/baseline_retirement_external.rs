//! External facade visibility; no fixture construction, network or worker launch.
#![cfg(feature = "protected-test-support")]
use super_platinum_core::drafts::store::protected::{self as p, owned_store::*};
#[allow(dead_code)]
fn signatures(
    reserved: &BaselineReserved,
    completion: &BaselineCompletion,
    finish: &StorageJob<BaselineFinishInput, p::Review>,
) {
    if let Ok((value, ack)) = reserved.observe_cooldown() {
        let _ = (value.key(), value.submission(), value.until_ms());
        let _ = ack.acknowledge();
    }
    if let Ok((value, history, ack)) = completion.observe_repaired_success() {
        let _ = (
            history.failed_submissions(),
            history.successful_submission(),
            ack.matches(&value),
        );
        let _ = ack.acknowledge();
    }
    if let Ok((value, history, ack)) = finish.observe_repaired_success() {
        let _ = (
            history.failed_submissions(),
            history.successful_submission(),
            ack.matches(&value),
        );
        let _ = ack.acknowledge();
    }
}
#[test]
fn public_retirement_signatures_are_available_without_private_storage_access() {
    let _: fn(&BaselineReserved, &BaselineCompletion, &StorageJob<BaselineFinishInput, p::Review>) =
        signatures;
}
