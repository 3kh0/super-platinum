//! Closed group responses use the same acquisition ledger as member completion.
use super::*;
use super_platinum_core::slack::api::custom::texty::groups as wire;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_GROUP_COMPLETION_CHECK";
pub(crate) const RESTRICTED: &str = "SUPER_PLATINUM_PROFILE_GROUP_COMPLETION_RESTRICTED";
pub(crate) const FIRST: &str = "SCOMPLETE1";
pub(crate) const SECOND: &str = "SCOMPLETE2";
pub(crate) const DELETED: &str = "SCOMPLETE3";
pub(crate) const LABEL: &str = "@Alex Harbor";
pub(crate) const SECOND_LABEL: &str = "@@Alex Café";
pub(crate) const REOPENED_LABEL: &str = "@Alex Cafe\u{301}";
pub(crate) fn named() -> bool {
    super::named() && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell)
}
pub(crate) fn restricted() -> bool {
    named() && std::env::var(RESTRICTED).as_deref() == Ok("1")
}
pub(crate) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    [(FIRST,"Alex Harbor","Alex Harbor",0),(SECOND,"Alex Café","@Alex Cafe\u{301}",0),(DELETED,"Alex Deleted Group","Alex Deleted Group",123)]
        .into_iter().map(|(id,name,handle,date_delete)|json!({"id":id,"team_id":owner.team_id,"name":name,"handle":handle,"date_update":101,"date_delete":date_delete,"users":[],"user_count":"0","is_external":false,"fixture_opaque":{"preserve":id,"ordered":["raw","effective"]}})).collect()
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Failure => {
                return Err(Error::Offline("Synthetic group search unavailable".into()));
            }
            Mode::Malformed => json!({"ok":true,"results":false}),
            Mode::Empty => json!({"ok":true,"results":[]}),
            Mode::Success | Mode::Held => {
                json!({"ok":true,"results":if named(){rows(owner)}else{vec![]}})
            }
        },
        None,
    ))
}
pub(super) fn plan(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<(PreparedRequest, Value), Error> {
    let RequestBody::Json(body) = &request.body else {
        return Err(refused());
    };
    // Source OrdinaryClientV2 policy is true, independently of body.
    let query = wire::Query::capture(owner, body["query"].as_str().ok_or_else(refused)?, true)?;
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(owner, &discovery(owner), now)?;
    Ok((
        query.request(client, owner, &endpoint, now, None)?,
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
    ledger.mode_method = Some("usergroups/search");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TINIT0001".into(),
            user_id: "UINIT0001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.test".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn actual_group_factory_and_opaque_rows_stay_distinct() {
        let owner = owner();
        let rows = rows(&owner);
        assert_eq!(rows.len(), 3);
        assert_ne!(rows[0]["id"], rows[1]["id"]);
        assert_eq!(rows[1]["handle"], "@Alex Cafe\u{301}");
        assert_eq!(rows[2]["date_delete"], 123);
        for row in rows {
            assert_eq!(row["fixture_opaque"]["preserve"], row["id"]);
            assert_eq!(row["team_id"], owner.team_id);
        }
    }
    #[test]
    fn failed_and_empty_responses_do_not_fabricate_unknown_groups() {
        let owner = owner();
        assert!(matches!(
            reply(&owner, Mode::Failure),
            Err(Error::Offline(_))
        ));
        assert_eq!(
            reply(&owner, Mode::Empty).unwrap().0,
            json!({"ok":true,"results":[]})
        );
        assert_eq!(
            reply(&owner, Mode::Malformed).unwrap().0,
            json!({"ok":true,"results":false})
        );
    }
}
