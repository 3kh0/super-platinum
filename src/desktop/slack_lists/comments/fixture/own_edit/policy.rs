use super::*;
use crate::slack_lists::comments::{
    Owner,
    own_edit_policy::{self, Observation},
};
use super_platinum_core::block_actions::name_policy;

fn response(
    client: &super_platinum_core::slack::SlackClient,
    session: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let requests = [
        (
            api::conversations_generic_info(
                client,
                session,
                &BTreeMap::from([("CCOMMENT01".into(), 0)]),
            ),
            json!({"ok":true,"channels":[{"id":"CCOMMENT01","is_channel":true,"is_file":true,"is_read_only":false,"is_thread_only":false,"context_team_id":"T1"}]}),
        ),
        (
            name_policy::request::user(client, session),
            json!({"ok":true,"prefs":{"plain_text_mode":false}}),
        ),
        (
            api::list_experiments(client, session),
            json!({"ok":true,"assignments":{"portable_content_composer":{"group":"control"}}}),
        ),
        (
            api::own_profile_team_preferences_request(client, session),
            json!({"ok":true,"team_prefs":{"toplevel_prefs":{"msg_edit_window_mins":-1},"workspace_prefs":{}}}),
        ),
    ];
    requests
        .into_iter()
        .find(|(expected, _)| same(request, expected))
        .map(|(_, raw)| raw)
        .ok_or_else(refused)
}
pub(crate) async fn policy(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
) -> Result<Observation, Error> {
    model(&state.peek())?;
    own_edit_policy::acquire_with(state, owner, serial, move |request| async move {
        let shell = state.peek();
        let model = model(&shell)?;
        let session = session(&shell)?;
        let raw = response(&shell.core.client, &session, &request)?;
        let form = wire(&request)?;
        let mut l = model.0.lock().map_err(|_| refused())?;
        l.own_edit
            .as_mut()
            .ok_or_else(refused)?
            .policy
            .push(json!({"request":form,"response":raw}));
        Ok(raw)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_reader_accepts_only_exact_factories() {
        let client = super_platinum_core::slack::SlackClient::default();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: completion::SELF.into(),
            ..super::super::super::session()
        };
        let request = name_policy::request::user(&client, &session);
        assert_eq!(
            response(&client, &session, &request).unwrap()["prefs"]["plain_text_mode"],
            false
        );
        let mut extra = request.clone();
        if let RequestBody::Form(fields) = &mut extra.body {
            fields.push(("channel".into(), "OTHER".into()));
        }
        assert!(response(&client, &session, &extra).is_err());
        extra = request;
        extra.url.push_str("&arbitrary=true");
        assert!(response(&client, &session, &extra).is_err());
    }
}
