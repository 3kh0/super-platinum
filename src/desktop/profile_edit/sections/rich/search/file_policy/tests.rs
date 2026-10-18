use super::*;
use crate::profile_edit::{
    model::Token,
    sections::{fixture, rich::opening},
};
use serde_json::{Value, json};

fn mounted() -> (ShellState, query::Lease, Arc<crate::scheduled::Context>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install(&mut shell);
    let auth = shell.core.session.clone();
    let transport = shell.core.transport.clone();
    fixture::install_interaction(&mut shell);
    shell.core.session = auth;
    shell.core.transport = transport;
    let token = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = token.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = token;
    let pane = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (pane.visit, pane.serial);
    let opening = opening::begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(opening::finish(&mut shell, opening));
    let lease = query::begin(&mut shell, visit, serial, 1, 1).unwrap();
    let context = lease.context(&shell).unwrap();
    (shell, lease, context)
}
fn prefs(
    shell: &ShellState,
    context: Arc<crate::scheduled::Context>,
    assignments: Value,
) -> Arc<ranking::Preferences> {
    ranking::Preferences::fixture(
        context,
        shell.core.active_account.clone().unwrap(),
        shell.core.session.as_ref().unwrap().d_cookie.clone(),
        crate::app_home::autocomplete::preferences_revision(shell),
        json!({"ok":true,"assignments":assignments}),
        json!({"ok":true,"prefs":{}}),
    )
    .unwrap()
}

#[test]
fn positive_off_is_current_observation_not_missing_policy() {
    for group in [
        None,
        Some("off"),
        Some("control"),
        Some("treatment"),
        Some("on"),
    ] {
        let (shell, lease, context) = mounted();
        let assignments = group.map_or(
            json!({}),
            |group| json!({"texty_autocomplete_files":{"group":group}}),
        );
        let preferences = prefs(&shell, context.clone(), assignments);
        let observed = Observation::bound(&shell, lease, context, preferences).unwrap();
        assert!(observed.owner_current(&shell));
        assert_eq!(observed.policy().enabled(), group == Some("on"));
    }
}

#[test]
fn malformed_target_assignment_does_not_become_disabled_or_enabled() {
    let (shell, lease, context) = mounted();
    let preferences = prefs(
        &shell,
        context.clone(),
        json!({"texty_autocomplete_files":{"group":true}}),
    );
    assert!(Observation::bound(&shell, lease, context, preferences).is_err());
}

#[test]
fn preferences_account_auth_socket_and_query_retire_original_observation() {
    for mutation in 0..8 {
        let (mut shell, lease, context) = mounted();
        let team = lease.team().to_owned();
        let preferences = prefs(
            &shell,
            context.clone(),
            json!({"texty_autocomplete_files":{"group":"on"}}),
        );
        let observed = Observation::bound(&shell, lease, context, preferences).unwrap();
        match mutation {
            0 => {
                let generation = shell.core.workspaces[&team].rt_generation;
                let event = super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                    name: "frecency".into(),
                    value: json!("{}"),
                };
                crate::app_home::realtime(&mut shell, &team, generation, &event);
            }
            1 => shell.core.session.as_mut().unwrap().d_cookie = "changed-cookie".into(),
            2 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "changed-token".into()
            }
            3 => shell.core.active_account = Some("different-account".into()),
            4 => shell.core.account_epoch += 1,
            5 => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            6 => shell.profile_edit.sections.draft.as_mut().unwrap().rich = None,
            _ => {
                let pane = shell.profile_edit.sections.draft.as_ref().unwrap();
                let (visit, serial) = (pane.visit, pane.serial);
                query::begin(&mut shell, visit, serial, 1, 2).unwrap();
            }
        }
        assert!(!observed.owner_current(&shell), "mutation {mutation}");
    }
}
