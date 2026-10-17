use super::*;
fn decimals(s: &ShellState, serial: u64, value: &str) -> bool {
    text_create::number::choose_shell(
        s,
        serial,
        generation(s).unwrap(),
        super::super::super::super::selection(s).unwrap(),
        text_create::choice(s).unwrap(),
        value,
    )
}
#[test]
fn precision_epoch_retains_name_same_type_and_refuses_aba_and_stale_callbacks() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    assert!(kind(&s, serial, "number"));
    let initial = text_create::choice(&s).unwrap();
    let visit = generation(&s).unwrap();
    let selected = super::super::super::super::selection(&s).unwrap();
    assert!(text_create::input_shell(
        &s,
        serial,
        visit,
        selected,
        initial,
        "A".into()
    ));
    assert!(text_create::input_shell(
        &s,
        serial,
        visit,
        selected,
        initial,
        "Amount".into()
    ));
    assert!(decimals(&s, serial, "3"));
    let configured = text_create::choice(&s).unwrap();
    assert_eq!(configured.number_precision.value(), 3);
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selected,
        initial,
        "stale".into()
    ));
    assert!(!text_create::number::choose_shell(
        &s, serial, visit, selected, initial, "2"
    ));
    assert!(!kind(&s, serial, "number"));
    assert_eq!(text_create::choice(&s), Some(configured));
    for bad in ["", "6", "-1", "2.0", "+2", "02", " 2", "null"] {
        assert!(!decimals(&s, serial, bad));
        assert_eq!(text_create::choice(&s), Some(configured));
    }
    assert!(decimals(&s, serial, "0"));
    assert_ne!(text_create::choice(&s).unwrap().revision, initial.revision);
    assert!(decimals(&s, serial, "5"));
    assert!(kind(&s, serial, "text"));
    assert!(kind(&s, serial, "number"));
    assert_eq!(text_create::choice(&s).unwrap().number_precision.value(), 0);
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Amount".into())
    );
    assert!(close_entry(&s, serial));
    assert!(entry(&s).is_none());
    assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
}
#[test]
fn all_precision_intents_lock_and_observe_only_their_canonical_raw_options() {
    for n in 0..=5 {
        let (mut s, _cleanup) = text_shell();
        let serial = open(&mut s);
        assert!(kind(&s, serial, "number"));
        assert!(name(&s, serial, "Release notes"));
        if n != 0 {
            assert!(decimals(&s, serial, &n.to_string()));
        }
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let ordered = order(&s);
        let (g, i) = prepare(&mut s, serial).unwrap();
        let Intent::NewTextField(intent) = &i else {
            panic!()
        };
        assert_eq!(intent.options(), Some(json!({"precision":n})));
        let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        let RequestBody::Form(fields) = permit.request(&s.core.client, &fixture::session()).body
        else {
            panic!()
        };
        assert_eq!(fields.len(), 7);
        assert_eq!(fields[5].1, json!({"precision":n}).to_string());
        assert!(!decimals(&s, serial, &((n + 1) % 6).to_string()));
        assert!(!name(&s, serial, "changed"));
        assert!(!kind(&s, serial, "text"));
        completed(
            &mut s,
            &g,
            Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "lost".into(),
            ))),
        );
        assert!(prepare(&mut s, serial).is_none());
        let mut fresh = number_addition(&s);
        fresh.document.raw["list_metadata"]["schema"][1]["options"] = json!({"precision":n});
        fresh.document = Document::parse(fresh.document.raw).unwrap();
        let check = checking(&s);
        assert!(checked(&mut s, &check, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            before.records
        );
        assert_eq!(order(&s), ordered);
        assert_eq!(
            s.slack_lists.view.as_ref().unwrap().layout.find.applied,
            "Item 0"
        );
        assert!(close_entry(&s, serial));
    }
}

#[test]
fn changed_loaded_population_and_retired_visit_refuse_precision_without_rebase() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    assert!(kind(&s, serial, "number"));
    assert!(decimals(&s, serial, "2"));
    let current = text_create::choice(&s).unwrap();
    let visit = generation(&s).unwrap();
    let selected = super::super::super::super::selection(&s).unwrap();
    assert!(!text_create::number::choose_shell(
        &s,
        serial,
        visit.wrapping_add(1),
        selected,
        current,
        "4"
    ));
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.records.raw.clone();
    raw["response_metadata"]["next_cursor"] = json!("Changed");
    loaded.records = RecordsPage::parse(raw).unwrap();
    assert!(!decimals(&s, serial, "4"));
    assert_eq!(text_create::choice(&s), Some(current));
    assert!(prepare(&mut s, serial).is_none());
    assert!(close_entry(&s, serial));
}
