//! Original ordinary-own permission read and explicit single-message deletion.
use super::*;
use crate::slack_lists::comments::{OwnDeleteIntent, OwnDeletePermissionQuery};

pub fn list_comment_delete_permissions(
    client: &SlackClient,
    session: &WorkspaceSession,
    query: &OwnDeletePermissionQuery,
) -> Result<PreparedRequest, Error> {
    query.validate().map_err(|e| Error::Api(e.to_string()))?;
    if session.team_id != query.workspace()
        || session.user_id != query.user()
        || session.enterprise_id.as_deref() != query.enterprise_scope()
    {
        return Err(Error::Api("list_comment_delete_owner_changed".into()));
    }
    client
        .edge_json(session, "permissions/info", query.body())
        .map_err(|_| Error::Api("invalid_list_comment_delete_permission".into()))
}

pub fn list_record_comment_delete(
    client: &SlackClient,
    session: &WorkspaceSession,
    intent: &OwnDeleteIntent,
) -> Result<PreparedRequest, Error> {
    let record = intent.record();
    record.validate().map_err(|e| Error::Api(e.to_string()))?;
    if session.team_id != record.workspace() || session.user_id != record.user() {
        return Err(Error::Api("list_comment_delete_owner_changed".into()));
    }
    Ok(client.rest_form(
        session,
        "chat.delete",
        vec![
            ("channel", record.target().channel_id().into()),
            ("ts", record.ts().into()),
            ("_x_reason", "animateAndDeleteMessageApi".into()),
        ],
    ))
}
