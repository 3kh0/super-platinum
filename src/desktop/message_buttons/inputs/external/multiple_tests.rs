use super::*;
use crate::message_buttons::{external_fixture as fixture, source::Bag};
use serde_json::{Value, json};
use super_platinum_core::{
    block_actions::external::ExternalSuggestions,
    slack::{SlackClient, client::RequestBody},
};

fn option(label: &str, value: &str) -> ExternalOption {
    ExternalOption::parse(&fixture::option(label, value)).unwrap()
}
fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    setup_required(false)
}
fn setup_required(required: bool) -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    let raw = &mut shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"];
    raw["type"] = json!("multi_external_select");
    let initial = raw
        .as_object_mut()
        .unwrap()
        .remove("initial_option")
        .unwrap();
    raw["initial_options"] = json!([initial]);
    raw["max_selected_items"] = json!(2);
    raw["required"] = json!(required);
    let w = &shell.core.workspaces["T1"];
    let row = crate::message_vm::message_vm(w, &w.messages["C2"].messages[0], &shell.media);
    let source = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = super::super::candidate(&shell, &source).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source
        .external_options_metadata(&shell, "choice", "pick")
        .unwrap();
    let mount = display::Mounted::new();
    assert_eq!(
        presentation(&shell, &source, &metadata, &mount).1,
        "1 selected"
    );
    (shell, source, metadata, mount)
}
fn open(
    shell: &mut ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mount: &display::Mounted,
) -> Owner {
    open_local(shell, source.clone(), metadata.clone(), mount.clone()).unwrap()
}
fn prepared(shell: &mut ShellState, captured: &Captured) -> Value {
    let req = super::super::prepare(
        shell,
        captured,
        &SlackClient::default(),
        &fixture::workspace(),
        "test",
    )
    .unwrap();
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap()
}
#[test]
fn staging_confirmation_cancel_rollback_and_ordered_wire_are_separate() {
    let (mut shell, source, metadata, mount) = setup();
    let owner = open(&mut shell, &source, &metadata, &mount);
    let revision = owner.query(&shell).unwrap().revision;
    let alpha = option("Alpha", "alpha,desktop");
    let beta = option("Beta", "beta");
    assert!(shared::complete(
        &mut shell,
        &owner,
        revision,
        Ok(ExternalSuggestions::Options(vec![
            alpha.clone(),
            beta.clone()
        ]))
    ));
    let Owner::Message(ticket) = owner.clone() else {
        unreachable!()
    };
    assert!(multiple::stage(&mut shell, &ticket, alpha.clone()));
    assert_eq!(
        presentation(&shell, &source, &metadata, &mount).1,
        "2 selected"
    );
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .saved_external_options(&ticket.key)
            .is_none()
    );
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    assert!(multiple::picker(&shell).unwrap().confirming);
    multiple::confirmation_cancel_local(&mut shell, ticket.id());
    assert!(!multiple::picker(&shell).unwrap().confirming);
    assert_eq!(ticket.selected(&shell).unwrap().len(), 2);
    assert!(multiple::remove_local(
        &mut shell,
        owner.clone(),
        "installed"
    ));
    assert!(multiple::stage(&mut shell, &ticket, beta.clone()));
    assert!(shared::complete(
        &mut shell,
        &owner,
        revision,
        Ok(ExternalSuggestions::Options(vec![]))
    ));
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    let captured = multiple::apply_local(&mut shell, ticket.id()).unwrap();
    let wire = prepared(&mut shell, &captured);
    assert_eq!(
        wire["values"]["choice"]["pick"]["selected_options"],
        json!([alpha.raw(), beta.raw()])
    );
    assert_eq!(
        wire["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    let owner = open(&mut shell, &source, &metadata, &mount);
    assert!(multiple::remove_local(
        &mut shell,
        owner.clone(),
        alpha.value()
    ));
    cancel_local(&mut shell);
    assert_eq!(
        presentation(&shell, &source, &metadata, &mount).1,
        "2 selected"
    );
    assert_eq!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .values()
            .unwrap()
            .unwrap(),
        wire["values"]
    );
}
#[test]
fn maximum_conflicting_metadata_and_stale_query_cannot_commit() {
    let (mut shell, source, metadata, mount) = setup();
    let owner = open(&mut shell, &source, &metadata, &mount);
    let query = owner.query(&shell).unwrap().revision;
    let a = option("Alpha", "a");
    let b = option("Beta", "b");
    assert!(shared::complete(
        &mut shell,
        &owner,
        query,
        Ok(ExternalSuggestions::Options(vec![a.clone(), b.clone()]))
    ));
    let Owner::Message(ticket) = owner.clone() else {
        unreachable!()
    };
    assert!(multiple::stage(&mut shell, &ticket, a.clone()));
    assert!(multiple::stage(&mut shell, &ticket, b));
    assert!(!multiple::picker(&shell).unwrap().enabled);
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    assert!(multiple::remove_local(
        &mut shell,
        owner.clone(),
        "installed"
    ));
    assert!(multiple::picker(&shell).unwrap().enabled);
    assert!(shared::complete(
        &mut shell,
        &owner,
        query,
        Ok(ExternalSuggestions::Options(vec![option(
            "Forged label",
            "a"
        )]))
    ));
    assert!(!multiple::picker(&shell).unwrap().enabled);
    assert!(multiple::remove_local(&mut shell, owner.clone(), "a"));
    assert!(multiple::picker(&shell).unwrap().enabled);
    shell.core.account_epoch += 1;
    assert!(!shared::complete(
        &mut shell,
        &owner,
        query,
        Ok(ExternalSuggestions::Options(vec![a]))
    ));
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
}
#[test]
fn saved_empty_wins_opening_but_not_fresh_outer_preview_and_clear_stays_array() {
    let (mut shell, source, metadata, mount) = setup();
    let key = field_key(&metadata).unwrap();
    shell
        .message_buttons
        .inputs
        .get_mut(&ContainerKey::new(&source))
        .unwrap()
        .model
        .commit_external_options(&key, &[])
        .unwrap();
    assert_eq!(
        presentation(&shell, &source, &metadata, &mount).1,
        "1 selected"
    );
    let owner = open(&mut shell, &source, &metadata, &mount);
    let Owner::Message(ticket) = owner else {
        unreachable!()
    };
    assert!(ticket.selected(&shell).unwrap().is_empty());
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    let captured = multiple::apply_local(&mut shell, ticket.id()).unwrap();
    let wire = prepared(&mut shell, &captured);
    assert_eq!(
        wire["values"]["choice"]["pick"]["selected_options"],
        json!([])
    );
}

#[test]
fn canonical_initial_and_max_changes_retire_the_open_picker_and_old_confirmation() {
    let (mut shell, source, metadata, mount) = setup();
    let owner = open(&mut shell, &source, &metadata, &mount);
    let Owner::Message(ticket) = owner.clone() else {
        unreachable!()
    };
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    assert!(multiple::picker(&shell).unwrap().confirming);
    let raw = &mut shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"];
    raw["initial_options"] = json!([]);
    raw["max_selected_items"] = json!(1);
    assert!(!ticket.current(&shell));
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    assert!(poll(&mut shell));
    let w = &shell.core.workspaces["T1"];
    let row = crate::message_vm::message_vm(w, &w.messages["C2"].messages[0], &shell.media);
    let fresh = Source::capture(&shell, &row, "C2", Bag::Timeline).unwrap();
    let entry = super::super::candidate(&shell, &fresh).unwrap();
    shell
        .message_buttons
        .inputs
        .insert(ContainerKey::new(&fresh), entry);
    let metadata = fresh
        .external_options_metadata(&shell, "choice", "pick")
        .unwrap();
    assert_eq!(
        presentation(&shell, &fresh, &metadata, &mount).1,
        "Find release"
    );
    let fresh_owner = open(&mut shell, &fresh, &metadata, &mount);
    let q = fresh_owner.query(&shell).unwrap().revision;
    let a = option("A", "a");
    let b = option("B", "b");
    assert!(shared::complete(
        &mut shell,
        &fresh_owner,
        q,
        Ok(ExternalSuggestions::Options(vec![a.clone(), b.clone()]))
    ));
    let Owner::Message(fresh_ticket) = fresh_owner else {
        unreachable!()
    };
    assert!(multiple::stage(&mut shell, &fresh_ticket, a));
    assert!(multiple::stage(&mut shell, &fresh_ticket, b));
    assert!(!multiple::picker(&shell).unwrap().enabled);
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
}

#[test]
fn required_empty_stays_staged_with_disabled_apply_until_corrected() {
    let (mut shell, source, metadata, mount) = setup_required(true);
    let owner = open(&mut shell, &source, &metadata, &mount);
    let Owner::Message(ticket) = owner.clone() else {
        unreachable!()
    };
    assert!(multiple::remove_local(
        &mut shell,
        owner.clone(),
        "installed"
    ));
    let view = multiple::picker(&shell).unwrap();
    assert!(!view.enabled);
    assert_eq!(view.reason.as_deref(), Some("Choose at least one option"));
    assert!(multiple::apply_local(&mut shell, ticket.id()).is_none());
    assert!(!multiple::picker(&shell).unwrap().confirming);
    let query = owner.query(&shell).unwrap().revision;
    let a = option("Alpha", "a");
    assert!(shared::complete(
        &mut shell,
        &owner,
        query,
        Ok(ExternalSuggestions::Options(vec![a.clone()]))
    ));
    assert!(multiple::stage(&mut shell, &ticket, a));
    assert!(multiple::picker(&shell).unwrap().enabled);
    assert!(
        shell.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .saved_external_options(&ticket.key)
            .is_none()
    );
}
