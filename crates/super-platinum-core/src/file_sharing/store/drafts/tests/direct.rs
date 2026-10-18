use super::*;

#[test]
fn conversation_destinations_reopen_and_keep_exact_attempt_custody() {
    for id in ["C12345678", "G12345678", "D12345678"] {
        for accepted in [false, true] {
            let t = Temp::new();
            let mut j = t.open();
            let initial = claim(&mut j, 0);
            let mut content = initial.content.clone();
            content.note = note();
            content.destination = Some(Destination {
                id: id.into(),
                name: "Exact conversation label".into(),
                private: id.starts_with('G'),
            });
            let saved = applied(j.save_draft(&initial, content.clone()).unwrap());
            drop(j);
            let mut j = t.open();
            let reopened = j.draft(&saved.scope).unwrap().unwrap();
            assert!(reopened == saved);
            assert!(reopened.content == content);
            assert!(
                j.unresolved(&saved.scope.team, &saved.scope.user)
                    .unwrap()
                    .is_empty()
            );
            let receipt = begin(&mut j, &reopened, 10);
            assert_eq!(receipt.scope.destination, id);
            assert_eq!(receipt.intent.note(), Some(note().as_slice()));
            assert_eq!(receipt.destination_name, "Exact conversation label");
            assert_eq!(links(&j), 1);
            drop(j);
            let mut j = t.open();
            assert!(j.get(&receipt.scope).unwrap().as_ref() == Some(&receipt));
            assert!(
                matches!(j.begin_from_draft(&saved, 11).unwrap(), Begin::Blocked(ref old) if old == &receipt)
            );
            assert!(
                matches!(j.save_draft(&saved, saved.content.clone()).unwrap(), Mutation::Blocked(ref old) if old == &receipt)
            );
            let response = if accepted {
                Ok(json!({"ok":true,"file_msg_ts":"1719800400.000001"}))
            } else {
                Err(crate::slack::Error::Transport(
                    "synthetic response lost".into(),
                ))
            };
            let settled = j
                .finish_result(&receipt.scope, &receipt.attempt_id, response, 20)
                .unwrap()
                .unwrap();
            assert_eq!(settled.attempt_id, receipt.attempt_id);
            assert!(settled.intent == receipt.intent);
            drop(j);
            let mut j = t.open();
            assert!(j.get(&receipt.scope).unwrap().as_ref() == Some(&settled));
            if accepted {
                assert!(!settled.unresolved());
                assert!(j.draft(&saved.scope).unwrap().is_none());
                assert_eq!(links(&j), 0);
            } else {
                assert!(settled.unresolved());
                assert!(j.draft(&saved.scope).unwrap().as_ref() == Some(&saved));
                assert_eq!(links(&j), 1);
                assert!(
                    matches!(j.begin_from_draft(&saved, 9000).unwrap(), Begin::Blocked(ref old) if old == &settled)
                );
            }
        }
    }
}

#[test]
fn invalid_conversation_destination_never_replaces_a_saved_direct_note() {
    let t = Temp::new();
    let mut j = t.open();
    let initial = claim(&mut j, 0);
    let mut content = initial.content.clone();
    content.note = note();
    content.destination = Some(Destination {
        id: "D12345678".into(),
        name: "Alice".into(),
        private: false,
    });
    let saved = applied(j.save_draft(&initial, content).unwrap());
    for id in [
        "",
        "D1234567",
        "D1234567a",
        "D1234567/",
        " D12345678",
        "D12345678 ",
        "U12345678",
        "F12345678",
        &format!("D{}", "1".repeat(64)),
    ] {
        let mut invalid = saved.content.clone();
        invalid.destination.as_mut().unwrap().id = id.into();
        assert!(
            j.save_draft(&saved, invalid).is_err(),
            "accepted invalid destination {id:?}"
        );
        assert!(j.draft(&saved.scope).unwrap().as_ref() == Some(&saved));
    }
    drop(j);
    assert!(t.open().draft(&saved.scope).unwrap().as_ref() == Some(&saved));
}
