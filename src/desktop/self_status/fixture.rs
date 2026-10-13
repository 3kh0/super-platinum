//! Exact-gated disconnected status requests: prepare real payloads, never send.
use super::*;
use super_platinum_core::{config::WorkspaceSession, slack::client::RequestBody};

#[derive(Clone, Default)]
pub(crate) struct Fixture {
    pub attempts: Vec<Value>,
    pub prepared_valid: bool,
    pending: Option<Attempt>,
    pub checks: usize,
    pub next_uncertain: bool,
}

pub(crate) fn enabled(shell: &ShellState) -> bool {
    std::env::var("SUPER_PLATINUM_SELF_STATUS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("self-status-editor" | "self-status-editor-narrow")
        )
        && shell.core.transport.is_none()
        && shell.core.session.is_none()
        && shell.self_status.fixture.is_some()
}

pub(crate) fn install(shell: &mut ShellState) {
    shell.core.transport = None;
    shell.core.session = None;
    shell.self_status = State::default();
    shell.self_status.fixture = Some(Fixture {
        prepared_valid: true,
        ..Default::default()
    });
    shell.toast = None;
    let scope = Scope::capture(shell).expect("fixture self account");
    scope.merge(
        shell,
        &Status {
            text: "Original status".into(),
            emoji: ":eyes:".into(),
            expiration: 0,
        },
    );
    open_shell(shell, now());
}

pub(crate) fn save(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let attempt = match begin(shell, now()) {
        Ok(attempt) => attempt,
        Err(error) => {
            set_error(shell, error);
            return;
        }
    };
    let session = WorkspaceSession {
        team_id: shell.core.active_team.clone().expect("fixture team"),
        enterprise_id: None,
        user_id: shell.self_account.user_id.clone(),
        name: "Disconnected status fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = prepare(&attempt, &SlackClient::default(), &session);
    let mut valid = request.method == "POST"
        && !request.retry_safe()
        && request
            .url
            .split('?')
            .next()
            .is_some_and(|url| url.ends_with("/api/users.profile.set"));
    if let RequestBody::Form(fields) = &request.body {
        let sent = fields
            .iter()
            .find(|(key, _)| key == "profile")
            .and_then(|(_, value)| serde_json::from_str::<Value>(value).ok());
        valid &= sent.as_ref() == Some(&attempt.profile);
    } else {
        valid = false;
    }
    let fixture = shell.self_status.fixture.as_mut().expect("fixture present");
    fixture.prepared_valid &= valid;
    fixture.attempts.push(attempt.profile.clone());
    fixture.pending = Some(attempt);
}

pub(crate) fn complete_pending(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.self_status.fixture.as_mut().expect("fixture present");
    let Some(attempt) = fixture.pending.take() else {
        return;
    };
    let result = if std::mem::take(&mut fixture.next_uncertain) {
        Err(Error::Api("internal_error".into()))
    } else if fixture.attempts.len() == 1 {
        Err(Error::Api("invalid_profile".into()))
    } else {
        Ok(serde_json::from_value(attempt.profile.clone()).expect("fixture status profile"))
    };
    finish(shell, &attempt, result);
}

pub(crate) fn check_status(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let Some(attempt) = begin_check(shell) else {
        return;
    };
    shell
        .self_status
        .fixture
        .as_mut()
        .expect("fixture present")
        .checks += 1;
    let profile =
        serde_json::from_value(attempt.profile.clone()).expect("fixture readback profile");
    finish_check(shell, &attempt, Ok(profile));
}

pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let fixture = shell.self_status.fixture.as_ref().expect("fixture present");
    let status = Scope::capture(shell)
        .map(|scope| scope.status(shell))
        .unwrap_or_default();
    json!({"attempts": fixture.attempts,"prepared_valid":fixture.prepared_valid,
        "checks":fixture.checks,"pending":fixture.pending.is_some(),
        "noTransport":shell.core.transport.is_none()&&shell.core.session.is_none(),
        "status":{"text":status.text,"emoji":status.emoji,"expiration":status.expiration}})
}
