use super::*;
fn avatars() -> ShellState {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-people-avatars",
    );
    s.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    s
}
#[test]
fn avatar_people_reproject_labels_while_find_retains_joined_user_identity() {
    let mut s = avatars();
    let raw = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    for (id, name) in [
        ("U00000001", "Alice renamed"),
        ("U99999999", "Resolved visitor"),
    ] {
        let before = row(&s, "R_LIST_000");
        let user: User =
            serde_json::from_value(json!({"id":id,"profile":{"display_name":name}})).unwrap();
        let change = prepare(&s, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .apply_user_update(user);
        assert!(change.apply(&mut s));
        let after = row(&s, "R_LIST_000");
        assert!(!Arc::ptr_eq(&before, &after));
        let c = after
            .cells
            .iter()
            .find(|c| c.column_id == "C_EDIT_USERS")
            .unwrap();
        assert!(c.text.contains(name));
        let CellContent::People(people) = &c.content else {
            panic!()
        };
        assert!(
            people
                .iter()
                .any(|p| p.user_id == id && p.known && p.label.contains(name))
        );
        let v = s.slack_lists.view.as_ref().unwrap();
        let w = s.core.workspaces.get("T1").unwrap();
        let query = super::super::super::find::query(v.loaded.as_ref().unwrap(), Some(w), name)
            .unwrap()
            .unwrap();
        // Find deliberately keeps the whole decoded multi-user key, unlike
        // the per-person display dependencies asserted above.
        let names = query.user_names.as_ref().unwrap();
        assert_eq!(
            names
                .get("U00000001|U00000002|W00000003|U99999999")
                .map(String::as_str),
            Some("")
        );
        if id == "U00000001" {
            // The separate single-person Owner field contributes this key.
            assert!(names[id].contains(name));
        } else {
            assert!(!names.contains_key(id));
        }
    }
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .raw,
        raw
    );
}
