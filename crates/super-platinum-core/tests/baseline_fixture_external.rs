//! Feature-only external visibility check; constructing/running is a root gate.
#![cfg(feature = "protected-test-support")]
use super_platinum_core::drafts::store::protected::owned_store::{
    AdmissionError, BaselineCompletion, BaselineFixture, BaselineFixtureOutcome,
    BaselineFixtureRead, BaselineFixtureTarget, BaselineRead, OwnedRoute,
};
fn context() -> (OwnedRoute, BaselineFixture) {
    OwnedRoute::fresh_baseline_fixture(
        BaselineFixtureTarget::Thread,
        BaselineFixtureOutcome::Changed,
    )
    .unwrap()
}
fn read(
    context: &BaselineFixture,
    completion: &BaselineCompletion,
) -> Result<BaselineRead, AdmissionError> {
    let _: Vec<BaselineFixtureRead> = context.reads();
    let _ = context.scope();
    context.read(completion, 1000)
}
#[test]
fn external_signatures_do_not_require_storage_or_raw_request_authority() {
    let _: fn() -> (OwnedRoute, BaselineFixture) = context;
    let _: fn(&BaselineFixture, &BaselineCompletion) -> Result<BaselineRead, AdmissionError> = read;
}

#[test]
fn owned_baseline_public_types_are_not_the_legacy_completion() {
    use super_platinum_core::drafts::store::protected::{self as p, owned_store as o};
    fn exported<T>() {}
    exported::<o::BaselineInput>();
    exported::<o::BaselineFinishInput>();
    exported::<o::BaselineReserved>();
    exported::<o::BaselineClaimed>();
    exported::<o::BaselineCompletion>();
    exported::<o::BaselineRead>();
    exported::<o::BaselineState>();
    exported::<o::BaselineOutcome>();
    exported::<o::BaselineAcknowledgment>();
    assert_ne!(
        std::any::TypeId::of::<o::BaselineCompletion>(),
        std::any::TypeId::of::<p::BaselineCompletion>()
    );
    let _: fn(&o::BaselineCompletion) -> Result<o::BaselineState, o::AdmissionError> =
        o::BaselineCompletion::state;
}
