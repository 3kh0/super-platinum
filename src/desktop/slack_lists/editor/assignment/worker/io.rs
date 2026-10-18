//! One bounded adapter for the existing worker. Fixture requests have no Transport.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Transport, client::SlackClient},
};

pub(super) struct Context {
    pub client: SlackClient,
    pub session: WorkspaceSession,
    transport: Option<Arc<Transport>>,
}
pub(super) fn capture(shell: &ShellState, guard: &Guard) -> Option<Context> {
    if fixture::dynamic::requested() {
        return fixture::dynamic::owns(shell, guard).then(|| Context {
            client: shell.core.client.clone(),
            session: WorkspaceSession {
                team_id: guard.scope.team.clone(),
                user_id: guard.scope.stable_owner().2,
                enterprise_id: None,
                name: "Disconnected assignment".into(),
                url: "https://example.test".into(),
                token: "fixture-not-a-token".into(),
            },
            transport: None,
        });
    }
    let context = guard.scope.request(shell)?;
    Some(Context {
        client: context.client,
        session: context.session,
        transport: Some(context.transport),
    })
}
pub(super) async fn execute(
    state: Signal<ShellState>,
    guard: &Guard,
    context: &Context,
    request: PreparedRequest,
) -> Result<Value, ListAssigneeFailure> {
    if let Some(transport) = &context.transport {
        transport
            .execute_list_assignee(request, || guard.current(&state.peek()))
            .await
    } else {
        fixture::dynamic::execute(state, guard, request).await
    }
}
