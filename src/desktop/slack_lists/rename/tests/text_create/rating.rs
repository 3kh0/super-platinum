use super::*;
use super_platinum_core::slack_lists::RatingOptions;
#[test]
fn rating_intent_uses_shared_uncertain_check_and_actual_selected_table_observation() {
    for max in 1..=5 {
        let (mut s, _cleanup) = text_shell();
        let serial = open(&mut s);
        let visit = generation(&s).unwrap();
        let selected = super::super::super::selection(&s).unwrap();
        assert!(text_create::kind_shell(
            &s,
            serial,
            visit,
            selected,
            text_create::choice(&s).unwrap(),
            "rating"
        ));
        if max != 5 {
            assert!(text_create::rating::quantity_shell(
                &s,
                serial,
                visit,
                selected,
                text_create::choice(&s).unwrap(),
                &max.to_string()
            ));
        }
        assert!(name(&s, serial, "Release notes"));
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let ordered = order(&s);
        let (g, i) = prepare(&mut s, serial).unwrap();
        let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        let RequestBody::Form(form) = permit.request(&s.core.client, &fixture::session()).body
        else {
            panic!()
        };
        assert_eq!(form.len(), 7);
        assert_eq!(form.iter().find(|(k, _)| k == "type").unwrap().1, "rating");
        assert_eq!(
            form.iter().find(|(k, _)| k == "options").unwrap().1,
            RatingOptions::capture(":star:", max, None)
                .unwrap()
                .wire()
                .to_string()
        );
        completed(
            &mut s,
            &g,
            Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "fixture lost reply".into(),
            ))),
        );
        assert!(prepare(&mut s, serial).is_none());
        let mut fresh = addition(&s, true);
        fresh.document.raw["list_metadata"]["schema"][1]["type"] = json!("rating");
        fresh.document.raw["list_metadata"]["schema"][1]["options"] =
            RatingOptions::capture(":star:", max, None).unwrap().wire();
        fresh.document = Document::parse(fresh.document.raw).unwrap();
        let check = checking(&s);
        assert!(checked(&mut s, &check, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
        assert!(presentation(&s).unwrap().message.contains("Rating"));
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
