use super::*;
use crate::message_buttons::{external_fixture as fixture, source::Bag};
use serde_json::{Value, json};
use super_platinum_core::{
    block_actions::external::ExternalSuggestions,
    slack::{SlackClient, client::RequestBody},
};
fn setup() -> (ShellState, Source, BlockElementMetadata, display::Mounted) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let w = &s.core.workspaces["T1"];
    let message = &w.messages["C2"].messages[0];
    let row = crate::message_vm::message_vm(w, message, &s.media);
    let source = Source::capture(&s, &row, "C2", Bag::Timeline).unwrap();
    let entry = super::super::candidate(&s, &source).unwrap();
    s.message_buttons
        .inputs
        .insert(ContainerKey::new(&source), entry);
    let metadata = source.external_metadata(&s, "choice", "pick").unwrap();
    let mounted = display::Mounted::new();
    assert_eq!(
        presentation(&s, &source, &metadata, &mounted).1,
        "Installed release"
    );
    (s, source, metadata, mounted)
}
fn picker(
    s: &mut ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> Owner {
    let key = field_key(metadata).unwrap();
    let route = route(s, source, &key).unwrap();
    let context = crate::scheduled::Context::capture(s).map(Arc::new);
    let ticket = Ticket {
        source: source.clone(),
        key: key.clone(),
        revision: next_id(),
        input_revision: revision(s, source).unwrap(),
        display: mounted.capture(&ContainerKey::new(source), &key).unwrap(),
        context: context.clone(),
        route: route.clone(),
    };
    let entry = s
        .message_buttons
        .inputs
        .get_mut(&ContainerKey::new(source))
        .unwrap();
    let field = &entry.model.fields[&key];
    let mut proofs = shared::Proofs::default();
    proofs.remember(&ExternalOption::parse(&field.raw["initial_option"]).unwrap());
    entry.external_proofs.entry(key).or_insert(Catalog {
        context,
        route,
        raw: field.raw.clone(),
        proofs,
    });
    s.message_buttons.external_picker = Some(Picker {
        multiple: None,
        ticket: ticket.clone(),
        query: Query::new(3),
    });
    s.overlay = Some(Overlay::MessageExternalSelect);
    Owner::Message(ticket)
}
fn values(s: &mut ShellState, captured: &Captured) -> Value {
    let request = super::super::prepare(
        s,
        captured,
        &SlackClient::default(),
        &fixture::workspace(),
        "fixture",
    )
    .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    serde_json::from_str(&fields.iter().find(|(k, _)| k == "state").unwrap().1).unwrap()
}
fn option() -> ExternalOption {
    ExternalOption::parse(&fixture::option("Release Alpha", "alpha,desktop")).unwrap()
}
#[test]
fn accepted_raw_option_survives_empty_query_confirmation_cancel_and_full_sibling_commit() {
    let (mut s, source, metadata, mounted) = setup();
    let owner = picker(&mut s, &source, &metadata, &mounted);
    let q = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![option()]))
    ));
    let Owner::Message(ticket) = owner else {
        unreachable!()
    };
    let proof = Proof {
        context: None,
        route: ticket.route.clone(),
    };
    let mut captured = source
        .capture_external(&s, "choice", "pick", Some(&option()), proof.clone())
        .unwrap();
    captured.set_display(ticket.display.clone());
    assert!(captured.confirmation().is_some());
    drop(captured);
    assert!(
        s.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .saved_external(&ticket.key)
            .is_none()
    );
    let mut captured = source
        .capture_external(&s, "choice", "pick", Some(&option()), proof)
        .unwrap();
    captured.set_display(ticket.display.clone());
    s.message_buttons.external_picker = None;
    s.overlay = None;
    let wire = values(&mut s, &captured);
    assert_eq!(
        wire["values"]["choice"]["pick"]["selected_option"],
        *option().raw()
    );
    assert_eq!(
        wire["values"]["review"]["environment"]["selected_option"]["value"],
        "prod"
    );
    assert_eq!(
        presentation(&s, &source, &metadata, &mounted).1,
        "Release Alpha"
    );
    let owner = picker(&mut s, &source, &metadata, &mounted);
    let query = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        query,
        Ok(ExternalSuggestions::Options(vec![]))
    ));
    s.message_buttons.external_picker = None;
    s.overlay = None;
    let button = source.capture_button(&s, "review", "submit").unwrap();
    assert_eq!(values(&mut s, &button), wire);
}
#[test]
fn obsolete_query_account_and_field_replacement_cannot_change_catalog() {
    let (mut s, source, metadata, mounted) = setup();
    let owner = picker(&mut s, &source, &metadata, &mounted);
    let q = owner.query(&s).unwrap().revision;
    s.message_buttons
        .external_picker
        .as_mut()
        .unwrap()
        .query
        .revision = next_id();
    assert!(!shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![option()]))
    ));
    assert!(!known(
        &s,
        &source,
        &field_key(&metadata).unwrap(),
        &option()
    ));
    let q = owner.query(&s).unwrap().revision;
    s.core.account_epoch += 1;
    assert!(!shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![option()]))
    ));
    s.core.account_epoch -= 1;
    let field = s
        .message_buttons
        .inputs
        .get_mut(&ContainerKey::new(&source))
        .unwrap()
        .model
        .fields
        .get_mut(&field_key(&metadata).unwrap())
        .unwrap();
    field.raw["min_query_length"] = json!(4);
    assert!(!known(
        &s,
        &source,
        &field_key(&metadata).unwrap(),
        &option()
    ));
}
#[test]
fn value_match_alone_or_changed_raw_metadata_cannot_authorize_confirmation() {
    let (mut s, source, metadata, mounted) = setup();
    let owner = picker(&mut s, &source, &metadata, &mounted);
    let q = owner.query(&s).unwrap().revision;
    let Owner::Message(ticket) = owner.clone() else {
        unreachable!()
    };
    let proof = Proof {
        context: None,
        route: ticket.route.clone(),
    };
    let captured = source
        .capture_external(&s, "choice", "pick", Some(&option()), proof.clone())
        .unwrap();
    assert!(captured.revalidate(&s).is_err());
    assert!(shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![option()]))
    ));
    assert!(captured.revalidate(&s).is_ok());
    let changed =
        ExternalOption::parse(&fixture::option("Different label", "alpha,desktop")).unwrap();
    assert!(shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![changed]))
    ));
    assert!(captured.revalidate(&s).is_err());
    assert_eq!(
        presentation(&s, &source, &metadata, &mounted).1,
        "Installed release"
    );
}

#[test]
fn fresh_sibling_prepare_cannot_validate_against_old_cached_field_before_ensure() {
    let (mut s, source, metadata, mounted) = setup();
    let owner = picker(&mut s, &source, &metadata, &mounted);
    let q = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        q,
        Ok(ExternalSuggestions::Options(vec![option()]))
    ));
    let Owner::Message(ticket) = owner else {
        unreachable!()
    };
    let mut captured = source
        .capture_external(
            &s,
            "choice",
            "pick",
            Some(&option()),
            Proof {
                context: None,
                route: ticket.route,
            },
        )
        .unwrap();
    captured.set_display(ticket.display);
    s.overlay = None;
    s.message_buttons.external_picker = None;
    values(&mut s, &captured);
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"]["min_query_length"] = json!(4);
    let w = &s.core.workspaces["T1"];
    let row = crate::message_vm::message_vm(w, &w.messages["C2"].messages[0], &s.media);
    let fresh = Source::capture(&s, &row, "C2", Bag::Timeline).unwrap();
    let sibling = fresh.capture_button(&s, "review", "submit").unwrap();
    assert!(
        super::super::prepare(
            &mut s,
            &sibling,
            &SlackClient::default(),
            &fixture::workspace(),
            "fixture"
        )
        .is_err()
    );
    assert_eq!(
        s.message_buttons.inputs[&ContainerKey::new(&source)]
            .model
            .saved_external(&field_key(&metadata).unwrap())
            .flatten(),
        Some(option())
    );
}

#[test]
fn removed_external_field_retires_catalog_while_other_controls_remain() {
    let (mut s, source, metadata, mounted) = setup();
    picker(&mut s, &source, &metadata, &mounted);
    assert_eq!(
        s.message_buttons.inputs[&ContainerKey::new(&source)]
            .external_proofs
            .len(),
        1
    );
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]
        .as_object_mut()
        .unwrap()
        .remove("accessory");
    let w = &s.core.workspaces["T1"];
    let row = crate::message_vm::message_vm(w, &w.messages["C2"].messages[0], &s.media);
    let fresh = Source::capture(&s, &row, "C2", Bag::Timeline).unwrap();
    let reconciled = super::super::candidate(&s, &fresh).unwrap();
    assert!(!reconciled.model.fields.is_empty());
    assert!(reconciled.external_proofs.is_empty());
}

#[test]
fn suggestion_fixture_uses_current_user_not_message_author() {
    let (s, source, metadata, _mounted) = setup();
    let workspace = fixture::workspace();
    assert_eq!(workspace.user_id, source.owner().self_user_id);
    let route = route(&s, &source, &field_key(&metadata).unwrap()).unwrap();
    let client = SlackClient::default();
    let request = api::block_suggestions(&client, &workspace, &route, "release").unwrap();
    assert!(!request.retry_safe());
    let mut wrong = workspace;
    wrong.user_id = "U1".into();
    assert!(api::block_suggestions(&client, &wrong, &route, "release").is_err());
}
