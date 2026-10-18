//! Existing workflow executor boundary; protected fixtures have no HTTP client.
use super::*;
use super_platinum_core::slack::PreparedRequest;

pub(super) async fn read(
    state: Signal<ShellState>,
    origin: &Origin,
    request: PreparedRequest,
) -> Result<Value, Error> {
    #[cfg(feature = "protected-test-support")]
    if crate::fixture::is_fixture() {
        return fixture::execute(state, origin, request, false).await;
    }
    let _ = state;
    origin.owner.ctx.transport.execute(request).await
}
pub(super) async fn once(
    state: Signal<ShellState>,
    origin: &Origin,
    request: PreparedRequest,
) -> Result<Value, Error> {
    #[cfg(feature = "protected-test-support")]
    if crate::fixture::is_fixture() {
        return fixture::execute(state, origin, request, true).await;
    }
    let _ = state;
    origin
        .owner
        .ctx
        .transport
        .execute_block_interaction(request)
        .await
}
