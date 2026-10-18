use super::*;
use crate::slack_lists::ViewRefinementSavePermit;
/// One exact metadata attempt. It does not also set the user's default view.
pub fn list_view_refinement_save(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewRefinementSavePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        intent.method(),
        intent
            .fields()
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect(),
    )
}
