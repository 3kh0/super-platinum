//! Real section request preparation against disconnected synthetic responses.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};
#[derive(Clone)]
pub(crate) struct Fixture {
    pub raw: Value,
    pub attempts: Vec<Value>,
    pub valid: bool,
    pub reads: usize,
    pub uncertain: bool,
    pending: Option<Attempt>,
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    (cfg!(test) || std::env::var("SUPER_PLATINUM_PROFILE_TITLE_CHECK").as_deref() == Ok("1"))
        && shell.profile_title.fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000078".into());
    let owner = Owner::capture(shell).unwrap();
    let profile = shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .users
        .get_mut(&owner.user)
        .unwrap()
        .profile
        .get_or_insert_with(Default::default);
    profile.title = Some("Original role".into());
    profile.status_text = Some("Reading".into());
    profile.pronouns = Some("they/them".into());
    profile
        .extra
        .insert("title_fixture".into(), json!({"keep":true}));
    shell.profile_title = State::default();
    shell.profile_title.fixture = Some(Fixture {
        raw: json!({"ok":true,"result":{"data":{"user":{"id":owner.user,"profileSections":[{"sectionId":"PsHeader","type":"HEADER","profileElements":[
        {"elementId":"PeTitle","elementKey":"title","__typename":"ProfileTextElement","text":"Original role","permissions":{"ui":true}},
        {"elementId":"PeName","elementKey":"display_name","__typename":"ProfileTextElement","text":"Preserved name","permissions":{"ui":false}}
    ]},{"sectionId":"PsOther","type":"OTHER","unknown":{"keep":true},"profileElements":[]}]}}}}),
        attempts: vec![],
        valid: true,
        reads: 0,
        uncertain: false,
        pending: None,
    });
    shell.toast = None;
    if let Some((token, visit)) = open_shell(shell) {
        let s = snapshot(shell, &token.owner);
        finish_load(shell, &token, visit, Ok(s));
    }
}
pub(super) fn session(owner: &Owner) -> WorkspaceSession {
    WorkspaceSession {
        team_id: owner.team.clone(),
        enterprise_id: None,
        user_id: owner.user.clone(),
        name: "Title fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-token".into(),
    }
}
pub(super) fn snapshot(shell: &ShellState, owner: &Owner) -> api::OwnProfileSections {
    api::OwnProfileSections::from_response(
        &session(owner),
        &shell.profile_title.fixture.as_ref().unwrap().raw,
    )
    .unwrap()
}
pub(super) fn load(shell: &mut ShellState, token: &Token, visit: u64) {
    if enabled(shell) {
        let s = snapshot(shell, &token.owner);
        finish_load(shell, token, visit, Ok(s));
    }
}
pub(super) fn save(shell: &mut ShellState, attempt: Attempt) {
    if !enabled(shell) {
        return;
    }
    let fresh = snapshot(shell, &attempt.token.owner);
    if !preflight(shell, &attempt, &fresh) {
        return;
    }
    let request = api::own_profile_title_request(
        &SlackClient::default(),
        &session(&attempt.token.owner),
        &attempt.intent,
        &fresh,
    );
    let f = shell.profile_title.fixture.as_mut().unwrap();
    let mut elements = Value::Null;
    let valid = request.is_ok_and(|r| {
        r.method == "POST"
            && !r.retry_safe()
            && r.url.contains("/api/users.profile.setSections")
            && match r.body {
                RequestBody::Form(fields) => {
                    elements = fields
                        .iter()
                        .find(|(k, _)| k == "elements")
                        .and_then(|(_, v)| serde_json::from_str(v).ok())
                        .unwrap_or(Value::Null);
                    fields
                        .iter()
                        .any(|(k, v)| k == "user" && v == &attempt.token.owner.user)
                        && fields
                            .iter()
                            .any(|(k, v)| k == "section" && v == "PsHeader")
                        && fields
                            .iter()
                            .any(|(k, v)| k == "_x_reason" && v == "edit-profile-form-pane")
                        && elements.as_array().is_some_and(|es| {
                            es.len() == 2
                                && es.iter().any(|e| {
                                    e["element_id"] == "PeTitle"
                                        && e["text"]["text"] == attempt.intent.desired_title()
                                })
                                && es.iter().any(|e| {
                                    e["element_id"] == "PeName"
                                        && e["text"]["text"] == "Preserved name"
                                })
                        })
                }
                _ => false,
            }
    });
    f.valid &= valid;
    f.attempts.push(elements);
    f.pending = Some(attempt);
}
pub(crate) fn complete(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let f = shell.profile_title.fixture.as_mut().unwrap();
    let Some(a) = f.pending.take() else {
        return;
    };
    f.raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][0]["text"] =
        json!(a.intent.desired_title());
    let unknown = f.uncertain;
    if finish_write(
        shell,
        &a,
        if unknown {
            Err(Error::Transport("synthetic ambiguous reply".into()))
        } else {
            Ok(())
        },
    ) {
        let visit = shell.profile_title.draft.as_ref().map(|d| d.visit);
        if let Some((t, id, r)) = visit.and_then(|v| begin_check(shell, v)) {
            check(shell, t, id, r);
        }
    }
}
pub(super) fn check(shell: &mut ShellState, token: Token, id: u64, read: u64) {
    if enabled(shell) {
        shell.profile_title.fixture.as_mut().unwrap().reads += 1;
        let s = snapshot(shell, &token.owner);
        finish_check(shell, &token, id, read, Ok(s));
    }
}
