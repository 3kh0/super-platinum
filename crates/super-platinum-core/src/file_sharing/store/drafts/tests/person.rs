use super::*;

#[test]
fn person_draft_reopens_but_never_begins_until_exact_cas_resolution() {
    let t = Temp::new();
    let mut j = t.open();
    let initial = claim(&mut j, 0);
    let mut content = initial.content.clone();
    content.note = note();
    content.destination = Some(Destination {
        id: "person:U12345678".into(),
        name: "Alice".into(),
        private: true,
    });
    let person = applied(j.save_draft(&initial, content).unwrap());
    drop(j);
    let mut j = t.open();
    assert!(j.draft(&person.scope).unwrap().as_ref() == Some(&person));
    assert!(j.begin_from_draft(&person, 10).is_err());
    assert!(
        j.unresolved(&person.scope.team, &person.scope.user)
            .unwrap()
            .is_empty()
    );
    let mut resolved = person.content.clone();
    resolved.destination.as_mut().unwrap().id = "D12345678".into();
    let direct = applied(j.save_draft(&person, resolved).unwrap());
    assert!(matches!(
        j.save_draft(&person, person.content.clone()).unwrap(),
        Mutation::Conflict(_)
    ));
    let receipt = begin(&mut j, &direct, 20);
    assert_eq!(receipt.scope.destination, "D12345678");
    assert_eq!(receipt.intent.note(), Some(note().as_slice()));
    assert!(matches!(
        j.begin_from_draft(&direct, 21).unwrap(),
        Begin::Blocked(_)
    ));
    drop(j);
    let j = t.open();
    assert!(j.get(&receipt.scope).unwrap().as_ref() == Some(&receipt));
    assert!(j.draft(&direct.scope).unwrap().as_ref() == Some(&direct));
}

#[test]
fn tagged_person_grammar_does_not_relax_conversation_ids_or_accept_lists() {
    let t = Temp::new();
    let mut j = t.open();
    let initial = claim(&mut j, 0);
    for id in [
        "U12345678",
        "person:D12345678",
        "person:U1234567",
        "person:U1234567a",
        "person:U12345678,U87654321",
        "person: U12345678",
        "person:U12345678\n",
    ] {
        let mut content = initial.content.clone();
        content.destination = Some(Destination {
            id: id.into(),
            name: "Alice".into(),
            private: true,
        });
        assert!(j.save_draft(&initial, content).is_err());
        assert!(j.draft(&initial.scope).unwrap().as_ref() == Some(&initial));
    }
}
