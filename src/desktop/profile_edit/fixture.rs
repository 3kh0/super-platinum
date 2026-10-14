//! Disconnected profile writes: real prepared requests, synthetic canonical reads.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};
#[derive(Clone)]
pub(crate) struct Fixture {
    pub attempts: Vec<Value>,
    pub valid: bool,
    pub(super) pending: Option<Attempt>,
    pub reads: usize,
    pub profile: UserProfile,
    pub allow_name: bool,
    pub allow_display: bool,
    pub allow_pronouns: bool,
    pub uncertain: bool,
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    (cfg!(test) || std::env::var("SUPER_PLATINUM_PROFILE_EDIT_CHECK").as_deref() == Ok("1"))
        && shell.profile_edit.fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000077".into());
    let profile:UserProfile=serde_json::from_value(json!({"real_name":"Original Person","display_name":"Original","pronouns":"they/them",
        "title":"Preserved title","status_text":"Reading","status_emoji":":books:","status_expiration":0,"unknown_fixture":{"keep":true}})).unwrap();
    let owner = Owner::capture(shell).expect("fixture self owner");
    shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .users
        .get_mut(&owner.user)
        .expect("fixture self user")
        .profile = Some(profile.clone());
    shell.profile_edit = State::default();
    shell.profile_edit.fixture = Some(Fixture {
        attempts: vec![],
        valid: true,
        pending: None,
        reads: 0,
        profile,
        allow_name: true,
        allow_display: true,
        allow_pronouns: true,
        uncertain: false,
    });
    shell.toast = None;
    if let Some((token, visit)) = open_shell(shell) {
        // The named screenshot fixture must render even when the interaction
        // checker flag is off; this pure installation performs no I/O.
        let cap = capabilities(shell, &token.owner);
        finish_load(shell, &token, visit, Ok(cap));
    }
}
pub(super) fn session(owner: &Owner) -> WorkspaceSession {
    WorkspaceSession {
        team_id: owner.team.clone(),
        enterprise_id: None,
        user_id: owner.user.clone(),
        name: "Profile fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-token".into(),
    }
}
pub(super) fn capabilities(shell: &ShellState, owner: &Owner) -> api::FreshOwnProfileCapabilities {
    let fixture = shell.profile_edit.fixture.as_ref().unwrap();
    let allowed = [
        (fixture.allow_name, "UPDATE_NAME"),
        (fixture.allow_display, "UPDATE_DISPLAY_NAME"),
    ]
    .into_iter()
    .filter(|(yes, _)| *yes)
    .map(|(_, id)| id)
    .collect::<Vec<_>>();
    api::FreshOwnProfileCapabilities::from_responses(&session(owner),
        &json!({"ok":true,"self":{"id":owner.user},"team":{"id":owner.team,"prefs":{"auth_mode":"normal","sso_choose_username":true,"display_pronouns":fixture.allow_pronouns}},"workspaces":[]}),
        &json!({"ok":true,"allowed":allowed}),&json!({"ok":true,"profile":fixture.profile}),
        &json!({"ok":true,"team_prefs":{"toplevel_prefs":{"auth_mode":"normal","sso_choose_username":true,"display_pronouns":fixture.allow_pronouns},"workspace_prefs":{}}}),None).unwrap()
}
pub(super) fn load(shell: &mut ShellState, token: &Token, visit: u64) {
    if !enabled(shell) {
        return;
    }
    let cap = capabilities(shell, &token.owner);
    finish_load(shell, token, visit, Ok(cap));
}
pub(super) fn save(shell: &mut ShellState, attempt: Attempt) {
    if !enabled(shell) {
        return;
    }
    let fresh = capabilities(shell, &attempt.token.owner);
    if !preflight(shell, &attempt, &fresh) {
        return;
    }
    let request = api::own_profile_text_request(
        &SlackClient::default(),
        &session(&attempt.token.owner),
        attempt.patch.clone(),
    );
    let fixture = shell.profile_edit.fixture.as_mut().unwrap();
    let valid = match request {
        Ok(request) => {
            request.method == "POST"
                && !request.retry_safe()
                && request.url.contains("/api/users.profile.set")
                && match request.body {
                    RequestBody::Form(fields) => {
                        fields
                            .iter()
                            .find(|(k, _)| k == "profile")
                            .and_then(|(_, v)| serde_json::from_str::<Value>(v).ok())
                            .as_ref()
                            == Some(&attempt.patch)
                            && fields
                                .iter()
                                .any(|(k, v)| k == "reason" && v == "EditProfileFormPane")
                            && !fields.iter().any(|(k, _)| k == "user")
                    }
                    _ => false,
                }
        }
        Err(_) => false,
    };
    fixture.valid &= valid;
    fixture.attempts.push(attempt.patch.clone());
    fixture.pending = Some(attempt);
}
pub(crate) fn complete(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.profile_edit.fixture.as_mut().unwrap();
    let Some(attempt) = fixture.pending.take() else {
        return;
    };
    for field in FIELDS {
        let Some(value) = attempt
            .patch
            .get(field.key())
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        match field {
            Field::FullName => fixture.profile.real_name = Some(value),
            Field::DisplayName => fixture.profile.display_name = Some(value),
            Field::Pronouns => fixture.profile.pronouns = Some(value),
        }
    }
    let uncertain = fixture.uncertain;
    let accepted = finish_write(
        shell,
        &attempt,
        if uncertain {
            Err(Error::Transport("synthetic unknown response".into()))
        } else {
            Ok(())
        },
    );
    if accepted {
        let visit = shell.profile_edit.draft.as_ref().map(|d| d.visit);
        if let Some((token, id, read)) = visit.and_then(|v| begin_check(shell, v)) {
            check(shell, token, id, read);
        }
    }
}
pub(super) fn check(shell: &mut ShellState, token: Token, id: u64, read: u64) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.profile_edit.fixture.as_mut().unwrap();
    fixture.reads += 1;
    let profile = fixture.profile.clone();
    finish_check(shell, &token, id, read, Ok(profile));
}
