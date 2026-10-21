//! Closed List file responses keep the original byte order until production decode.
use super::*;
use super_platinum_core::slack::{
    SlackClient, api::custom::texty::files as wire, client::RequestBody,
};

pub(crate) const CANVAS: &str = "FLISTFILE1";
pub(crate) const LIST: &str = "FLISTFILE2";
pub(crate) const HOSTED: &str = "FLISTFILE3";
pub(crate) const CANVAS_LABEL: &str = "Alex Canvas";
pub(crate) const LIST_LABEL: &str = "Alex List";
pub(crate) const RESPONSE: &str = include_str!("files-response.json");

pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
        Ok("file-completion"
            | "file-completion-custody"
            | "file-completion-off"
            | "file-navigation"
            | "file-navigation-custody")
    )
}
pub(crate) fn off() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
        == Ok("file-completion-off")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell)
}
pub(super) fn assignments() -> Value {
    let mut raw = json!({"ok":true,"assignments":{"friendly_channel_names":{"group":"off"}}});
    if named() {
        raw["assignments"]["texty_autocomplete_files"] = json!({"group":if off(){"off"}else{"on"}});
    }
    raw
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    if owner.team_id != "T1" {
        return Err(refused());
    }
    let bytes = match mode {
        Mode::Offline => {
            return Err(Error::Offline(
                "Synthetic List file search unavailable".into(),
            ));
        }
        Mode::Malformed => r#"{"ok":true,"files":false}"#,
        Mode::Empty => r#"{"ok":true,"files":[]}"#,
        Mode::Success | Mode::Held => RESPONSE,
    };
    Ok((Value::String(bytes.into()), None))
}
pub(super) fn plan(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<(PreparedRequest, Value), Error> {
    let RequestBody::Form(body) = &request.body else {
        return Err(refused());
    };
    let query = body
        .iter()
        .find(|(key, _)| key == "query")
        .map(|(_, value)| value.as_str())
        .ok_or_else(refused)?;
    let expected = wire::request(client, owner, query);
    if !protocol::same_request(request, &expected) {
        return Err(refused());
    }
    Ok((expected, reply(owner, Mode::Success)?.0))
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<wire::OrderedResponse, Error> {
    {
        let shell = state.peek();
        if !enabled(&shell)
            || off()
            || url::Url::parse(&request.url).map_err(|_| refused())?.path()
                != "/api/search.autocomplete.files"
        {
            return Err(refused());
        }
    }
    let (raw, _) = super::request(state, request).await?;
    wire::OrderedResponse::decode(raw.as_str().ok_or_else(refused)?.as_bytes())
}
/// Closed exercise of the existing local-metadata invalidation boundary. This
/// neither synthesizes a server event nor replaces any original response row.
pub(crate) fn invalidate(shell: &mut ShellState, id: &str) -> Result<Value, Error> {
    if !enabled(shell) || off() || !matches!(id, CANVAS | LIST) {
        return Err(refused());
    }
    let model = handle(shell)?;
    let before =
        crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(shell)?;
    if !before["rows"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["id"] == id))
    {
        return Err(refused());
    }
    let event_before = crate::file_events::snapshot_revision(shell);
    crate::file_events::invalidate(shell, &model.session.team_id, id);
    let after =
        crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(shell)?;
    Ok(json!({"id":id,"before":before,"after":after,
        "eventRevisionBefore":event_before,"eventRevisionAfter":crate::file_events::snapshot_revision(shell)}))
}

pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if !enabled(shell) {
        return json!({"enabled":false});
    }
    let source =
        crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(shell)
            .unwrap_or_else(|_| json!({"unavailable":true}));
    json!({"enabled":true,"gate":if off(){"off"}else{"on"},"originalResponse":RESPONSE,"source":source})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            user_id: SELF.into(),
            ..super::super::super::session()
        }
    }
    #[test]
    fn exact_file_factory_rejects_added_duplicate_or_changed_fields() {
        let client = SlackClient::default();
        let owner = owner();
        let request = wire::request(&client, &owner, "alex");
        let (_, bytes) = plan(&client, &owner, &request).unwrap();
        assert_eq!(bytes.as_str(), Some(RESPONSE));
        for change in 0..4 {
            let mut bad = request.clone();
            let RequestBody::Form(body) = &mut bad.body else {
                panic!("form");
            };
            match change {
                0 => body.push(("count".into(), "25".into())),
                1 => body.push(("query".into(), "alex".into())),
                2 => {
                    body.iter_mut()
                        .find(|(k, _)| k == "include_shares")
                        .unwrap()
                        .1 = "false".into()
                }
                _ => body.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 = "other".into(),
            }
            assert!(plan(&client, &owner, &bad).is_err());
        }
        let mut foreign = request;
        // Use an unconditionally foreign URL rather than assuming the factory host.
        foreign.url = "https://foreign.invalid/api/search.autocomplete.files".into();
        assert!(plan(&client, &owner, &foreign).is_err());
    }
    #[test]
    fn original_bytes_preserve_map_order_and_real_category_exclusion() {
        use super_platinum_core::slack::api::custom::texty::file_title_source::Store;
        let response = wire::OrderedResponse::decode(RESPONSE.as_bytes()).unwrap();
        let mut store = Store::new();
        store
            .ingest_batch_ordered(
                response.raw(),
                response.rows(),
                &owner(),
                response.channel_map_order(),
            )
            .unwrap();
        let canvas = store.candidate(CANVAS).unwrap();
        let list = store.candidate(LIST).unwrap();
        assert_eq!(
            canvas.channel_map_keys().unwrap(),
            ["z-last", "a-first", "m-foreign"]
        );
        assert_eq!(
            list.channel_map_keys().unwrap(),
            ["y-last", "b-first", "x-foreign"]
        );
        assert_eq!(
            canvas.effective()["channels"],
            json!(["CZMAP001", "CAMAP001", "CTAIL001"])
        );
        assert_eq!(
            list.effective()["channels"],
            json!(["CYMAP002", "CBMAP002"])
        );
        assert_eq!(canvas.effective()["title"], CANVAS_LABEL);
        assert_eq!(list.effective()["title"], LIST_LABEL);
        assert_eq!(canvas.source(), &response.rows()[0]);
        assert_eq!(list.source(), &response.rows()[1]);
        assert_eq!(
            store.candidates().excluded_ids().collect::<Vec<_>>(),
            [HOSTED]
        );
        assert!(store.candidates().unavailable_ids().is_empty());
    }
    #[test]
    fn modes_count_only_file_dispatch_without_changing_existing_targets() {
        for method in [
            "users/search",
            "usergroups/search",
            "channels/search",
            "users/list",
        ] {
            assert!(!protocol::mode_target(method, true, true));
        }
        assert!(protocol::mode_target(
            "search.autocomplete.files",
            true,
            true
        ));
        assert!(!protocol::mode_target(
            "search.autocomplete.files",
            false,
            true
        ));
        assert!(protocol::mode_target("usergroups/search", false, true));
        assert!(!protocol::mode_target("users/search", false, true));
        assert!(protocol::mode_target("users/search", false, false));
        assert!(protocol::mode_target("channels/search", false, false));
    }
    #[test]
    fn file_modes_retain_exact_bytes_or_explicit_failure() {
        assert_eq!(
            reply(&owner(), Mode::Held).unwrap().0.as_str(),
            Some(RESPONSE)
        );
        let empty = reply(&owner(), Mode::Empty).unwrap().0;
        assert!(
            wire::OrderedResponse::decode(empty.as_str().unwrap().as_bytes())
                .unwrap()
                .rows()
                .is_empty()
        );
        let malformed = reply(&owner(), Mode::Malformed).unwrap().0;
        assert!(wire::OrderedResponse::decode(malformed.as_str().unwrap().as_bytes()).is_err());
        assert!(matches!(
            reply(&owner(), Mode::Offline),
            Err(Error::Offline(_))
        ));
        let mut other = owner();
        other.team_id = "TOTHER".into();
        assert!(reply(&other, Mode::Success).is_err());
    }
}
