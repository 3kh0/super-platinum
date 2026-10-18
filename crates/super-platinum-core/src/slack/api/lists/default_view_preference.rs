use super::*;
use crate::slack_lists::ListDefaultViewPreferencePermit;
pub fn list_default_view_preference(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ListDefaultViewPreferencePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "users.prefs.set",
        vec![
            ("name", "lists_default_view".into()),
            ("value", intent.expected().to_string()),
            ("_x_reason", "prefs-api/setUserPrefByApi".into()),
        ],
    )
}
/// Nu4c reads named prefs through users.prefs.get. Local reason names this
/// separate current-state read; it grants no metadata or preference write.
pub fn list_default_view_preference_read(
    client: &SlackClient,
    session: &WorkspaceSession,
) -> PreparedRequest {
    client.rest_form(
        session,
        "users.prefs.get",
        vec![
            ("prefs", "lists_default_view".into()),
            ("_x_reason", "native-list-default-view-read".into()),
        ],
    )
}
