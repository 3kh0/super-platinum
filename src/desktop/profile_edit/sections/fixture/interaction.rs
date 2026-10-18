//! Closed section-save transport for the explicitly named native fixture.
//! Builds the real request; it never installs a session or sends HTTP.
use super::*;
use serde_json::json;
use super_platinum_core::slack::{SlackClient, client::RequestBody};

#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Saved,
    Rejected,
    Uncertain,
}

#[derive(Clone)]
pub(in crate::profile_edit::sections) struct Interaction {
    pub snapshot: api::OwnProfileSections,
    pub attempts: Vec<Value>,
    pub reads: usize,
    pub outcome: Outcome,
    pub(super) mentions: Option<super::mentions::Identity>,
    pub(super) completion: Option<super::completion::Model>,
    pending: Option<(SectionAttempt, Value)>,
}

pub(crate) fn enabled(shell: &ShellState) -> bool {
    ((cfg!(test) || std::env::var("SUPER_PLATINUM_PROFILE_SECTIONS_CHECK").as_deref() == Ok("1"))
        && shell.profile_edit.sections.fixture.is_some()
        && shell.profile_edit.fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none())
        || super::mentions::enabled(shell)
}

pub(crate) fn install_interaction(shell: &mut ShellState) {
    install(shell);
    let snapshot = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .snapshot
        .clone()
        .unwrap();
    shell.profile_edit.sections.fixture = Some(Interaction {
        snapshot,
        attempts: Vec::new(),
        reads: 0,
        outcome: Outcome::Saved,
        mentions: None,
        completion: None,
        pending: None,
    });
}

pub(in crate::profile_edit::sections) fn load(
    shell: &mut ShellState,
    token: &Token,
    visit: u64,
    serial: u64,
) {
    if !enabled(shell) {
        return;
    }
    let snapshot = shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .unwrap()
        .snapshot
        .clone();
    network::finish_load(shell, token, visit, serial, Ok(snapshot));
}

pub(in crate::profile_edit::sections) fn save(shell: &mut ShellState, attempt: SectionAttempt) {
    if !enabled(shell) {
        return;
    }
    let snapshot = shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .unwrap()
        .snapshot
        .clone();
    if !network::preflight(shell, &attempt, &snapshot) {
        return;
    }
    let request = protocol::section_request(
        &SlackClient::default(),
        &super::super::super::fixture::session(&attempt.token.owner),
        &attempt.intent,
        &snapshot,
    );
    let elements = request.ok().and_then(|request| {
        if request.method != "POST"
            || request.retry_safe()
            || !request
                .url
                .split('?')
                .next()
                .is_some_and(|url| url.ends_with("/api/users.profile.setSections"))
        {
            return None;
        }
        let RequestBody::Form(fields) = request.body else {
            return None;
        };
        let field = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, v)| v.as_str())
        };
        if field("user") != Some(attempt.token.owner.user.as_str())
            || field("section") != Some(attempt.intent.section_id())
            || field("_x_reason") != Some("edit-profile-form-pane")
        {
            return None;
        }
        serde_json::from_str::<Value>(field("elements")?)
            .ok()
            .filter(Value::is_array)
    });
    let Some(elements) = elements else {
        network::finish_write(
            shell,
            &attempt,
            Err(Error::Api("invalid_fixture_section_request".into())),
        );
        return;
    };
    let fixture = shell.profile_edit.sections.fixture.as_mut().unwrap();
    fixture.attempts.push(elements.clone());
    fixture.pending = Some((attempt, elements));
}

pub(crate) fn complete(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.profile_edit.sections.fixture.as_mut().unwrap();
    let Some((attempt, elements)) = fixture.pending.take() else {
        return;
    };
    let result = match fixture.outcome {
        Outcome::Rejected => Err(Error::Api("not_allowed".into())),
        Outcome::Uncertain => Err(Error::Transport("synthetic lost section response".into())),
        Outcome::Saved => {
            // Apply only fields represented by the real prepared section request.
            // Managed and unknown siblings retain their original raw shapes.
            let mut raw = fixture.snapshot.raw().clone();
            let fields = raw["result"]["data"]["user"]["profileSections"][0]["profileElements"]
                .as_array_mut()
                .unwrap();
            for field in fields {
                if let Some(wire) = elements
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["element_id"] == field["elementId"])
                {
                    match field["__typename"].as_str() {
                        Some("ProfileTextElement") => {
                            field["text"] = wire["text"]["text"]
                                .as_str()
                                .map_or(json!(""), |v| json!(v));
                        }
                        Some("ProfileLongTextElement") => {
                            field["blocks"] = wire["long_text"]["blocks"]
                                .as_str()
                                .map_or(json!(""), |v| json!(v));
                        }
                        _ => {}
                    }
                }
            }
            fixture.snapshot = api::OwnProfileSections::from_response(
                &super::super::super::fixture::session(&attempt.token.owner),
                &raw,
            )
            .unwrap();
            Ok(())
        }
    };
    if network::finish_write(shell, &attempt, result)
        && let Some(read) = begin_check(shell, attempt.visit, attempt.serial)
    {
        check(shell, read);
    }
}

pub(in crate::profile_edit::sections) fn check(shell: &mut ShellState, attempt: SectionAttempt) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.profile_edit.sections.fixture.as_mut().unwrap();
    fixture.reads += 1;
    let snapshot = fixture.snapshot.clone();
    network::finish_check(shell, &attempt, Ok(snapshot));
}
