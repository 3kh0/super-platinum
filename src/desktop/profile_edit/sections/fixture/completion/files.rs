//! Named, closed file completion responses. Generic hosted rows deliberately
//! reach the real category filter; this fixture never prefilters the response.
use super::*;
use super_platinum_core::slack::api::custom::texty::files as wire;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK";
pub(crate) const OFF: &str = "SUPER_PLATINUM_PROFILE_FILE_COMPLETION_OFF";
pub(crate) const CANVAS: &str = "FCOMPLETE1";
pub(crate) const LIST: &str = "FCOMPLETE2";
pub(crate) const HOSTED: &str = "FCOMPLETE3";
pub(crate) const CANVAS_LABEL: &str = "Alex Canvas";
pub(crate) const LIST_LABEL: &str = "Alex List";
/// Original simulated response bytes. Never serialize a Value to establish
/// channel_map key order: the actual production decoder receives these bytes.
pub(crate) const RESPONSE: &str = include_str!("files-response.json");

pub(crate) fn named() -> bool {
    super::named() && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell)
}
pub(crate) fn off() -> bool {
    named() && std::env::var(OFF).as_deref() == Ok("1")
}
pub(super) fn assignments() -> Value {
    if named() {
        json!({"ok":true,"assignments":{"texty_autocomplete_files":{"group":if off(){"off"}else{"on"}}}})
    } else {
        json!({"ok":true,"assignments":{}})
    }
}
pub(crate) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    if owner.team_id != "TINIT0001" {
        return Vec::new();
    }
    // Convenience for the separately admitted files.info title receipt only;
    // ordered autocomplete uses RESPONSE directly, never these parsed rows.
    serde_json::from_str::<Value>(&title_users::response()).unwrap()["files"]
        .as_array()
        .unwrap()
        .clone()
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    if owner.team_id != "TINIT0001" {
        return Err(refused());
    }
    let bytes = match mode {
        Mode::Failure => {
            return Err(Error::Offline("Synthetic file search unavailable".into()));
        }
        Mode::Malformed => r#"{"ok":true,"files":false}"#,
        Mode::Empty => r#"{"ok":true,"files":[]}"#,
        Mode::Success | Mode::Held => return Ok((Value::String(title_users::response()), None)),
    };
    // The generic closed ledger transports an opaque string for this endpoint.
    // Held/cancel paths preserve this exact allocation's bytes until decode.
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
    Ok((
        wire::request(client, owner, query),
        reply(owner, Mode::Success)?.0,
    ))
}
pub(crate) fn mode(shell: &ShellState, value: Mode) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let model = handle(shell)?;
    let mut ledger = model.0.lock().unwrap();
    ledger.mode = value;
    ledger.mode_method = Some("search.autocomplete.files");
    Ok(())
}

pub(super) fn info(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<(PreparedRequest, Value), Error> {
    let RequestBody::Form(body) = &request.body else {
        return Err(refused());
    };
    let id = body
        .iter()
        .find(|(key, _)| key == "file")
        .map(|(_, value)| value.as_str())
        .ok_or_else(refused)?;
    let row = rows(owner)
        .into_iter()
        .find(|row| row["id"] == id && id != HOSTED)
        .ok_or_else(refused)?;
    let expected = client.rest_form(
        owner,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    );
    Ok((expected, json!({"ok":true,"file":row})))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> WorkspaceSession {
        serde_json::from_value(json!({"team_id":"TINIT0001","user_id":"UINIT0001","name":"Fixture","url":"https://fixture.invalid","token":"synthetic"})).unwrap()
    }
    #[test]
    fn exact_file_factory_and_raw_categories_are_preserved() {
        let owner = owner();
        let client = SlackClient::default();
        let request = wire::request(&client, &owner, "alex");
        let accepted = validate(&client, &owner, &request).unwrap();
        assert!(accepted.search);
        assert_eq!(accepted.method, "search.autocomplete.files");
        assert_eq!(
            accepted.args,
            json!({"query":"alex","include_shares":"true","_x_reason":"texty-autocomplete"})
        );
        let decoded =
            wire::OrderedResponse::decode(accepted.reply.0.as_str().unwrap().as_bytes()).unwrap();
        let rows = decoded.rows();
        assert_eq!(
            rows.iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [CANVAS, LIST, HOSTED]
        );
        assert_eq!(rows[2]["mode"], "hosted");
        for row in rows {
            assert_eq!(row["fixture_opaque"]["preserve"], row["id"]);
        }
        for mutation in 0..3 {
            let mut bad = wire::request(&client, &owner, "alex");
            let RequestBody::Form(body) = &mut bad.body else {
                panic!()
            };
            match mutation {
                0 => body.push(("count".into(), "25".into())),
                1 => {
                    body.iter_mut()
                        .find(|(key, _)| key == "include_shares")
                        .unwrap()
                        .1 = "false".into()
                }
                _ => {
                    body.iter_mut()
                        .find(|(key, _)| key == "_x_reason")
                        .unwrap()
                        .1 = "other".into()
                }
            }
            assert!(validate(&client, &owner, &bad).is_err());
        }
    }
    #[test]
    fn original_bytes_accept_shared_candidates_without_alphabetizing_channel_maps() {
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
        assert_eq!(canvas.source(), &response.rows()[0]);
        assert_eq!(list.source(), &response.rows()[1]);
        assert!(canvas.current(&store) && list.current(&store));
        assert_eq!(
            store.candidates().excluded_ids().collect::<Vec<_>>(),
            [HOSTED]
        );
        // A Value's sorted map is not equivalent evidence: the legacy intake
        // still refuses to authorize these multi-string-key projections.
        let mut unordered = Store::new();
        unordered
            .ingest_batch(response.raw(), response.rows(), &owner())
            .unwrap();
        assert!(unordered.candidate(CANVAS).is_err());
        assert!(unordered.candidate(LIST).is_err());
    }
    #[tokio::test]
    async fn held_file_reply_preserves_original_byte_string_until_decode() {
        let owner = owner();
        let client = SlackClient::default();
        let model = Model::default();
        {
            let mut ledger = model.0.lock().unwrap();
            ledger.mode = Mode::Held;
            ledger.mode_method = Some("search.autocomplete.files");
        }
        let accepted = validate(&client, &owner, &wire::request(&client, &owner, "alex")).unwrap();
        let Start::Held(receiver) = start(&model, &owner, accepted).unwrap() else {
            panic!()
        };
        assert_eq!(model.release().unwrap()["delivered"], true);
        let (bytes, keys) = wait(model.clone(), receiver).await.unwrap();
        assert!(keys.is_none());
        assert_eq!(bytes.as_str(), Some(RESPONSE));
        assert!(wire::OrderedResponse::decode(bytes.as_str().unwrap().as_bytes()).is_ok());
        assert_eq!(model.snapshot()["completed"], 1);
        assert_eq!(model.snapshot()["cancelled"], 0);
    }
    #[tokio::test]
    async fn held_file_reply_cancellation_is_separate_from_successful_empty_or_error() {
        let owner = owner();
        let client = SlackClient::default();
        let model = Model::default();
        {
            let mut ledger = model.0.lock().unwrap();
            ledger.mode = Mode::Held;
            ledger.mode_method = Some("search.autocomplete.files");
        }
        let accepted = validate(&client, &owner, &wire::request(&client, &owner, "alex")).unwrap();
        let Start::Held(receiver) = start(&model, &owner, accepted).unwrap() else {
            panic!()
        };
        {
            let future = wait(model.clone(), receiver);
            tokio::pin!(future);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(1), &mut future)
                    .await
                    .is_err()
            );
        }
        assert_eq!(model.release().unwrap()["delivered"], false);
        assert_eq!(model.snapshot()["cancelled"], 1);
        // The pinned future is dropped at scope end; direct response outcomes
        // remain distinct and contain no fabricated source rows.
        assert!(reply(&owner, Mode::Failure).is_err());
        assert!(
            wire::OrderedResponse::decode(
                reply(&owner, Mode::Malformed)
                    .unwrap()
                    .0
                    .as_str()
                    .unwrap()
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            wire::OrderedResponse::decode(
                reply(&owner, Mode::Empty)
                    .unwrap()
                    .0
                    .as_str()
                    .unwrap()
                    .as_bytes()
            )
            .unwrap()
            .rows()
            .is_empty()
        );
        assert_eq!(model.snapshot()["fileSearches"], 1);
    }
}
