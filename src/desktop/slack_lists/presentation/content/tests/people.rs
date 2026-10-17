use super::*;
#[test]
fn only_explicit_false_ordinary_people_use_avatars_without_losing_text_or_order() {
    let media = MediaRegistry::default();
    let w = workspace();
    let (mut column, record) = values("user", json!({"key":"field","user":["UNKNOWN","U1","U1"]}));
    let raw = record.raw.clone();
    let before = project(Some(&w), &media, &column, &record);
    assert!(matches!(before.1, CellContent::Rich(_)));
    column.raw["options"] = json!({"format":"multi_entity","show_member_name":false});
    let (text, content) = project(Some(&w), &media, &column, &record);
    assert_eq!(text, before.0);
    let CellContent::People(people) = content else {
        panic!()
    };
    assert_eq!(
        people
            .iter()
            .map(|p| p.user_id.as_str())
            .collect::<Vec<_>>(),
        vec!["UNKNOWN", "U1", "U1"]
    );
    assert_eq!(
        people.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
        vec!["UNKNOWN", "@Maya", "@Maya"]
    );
    assert_eq!(
        people.iter().map(|p| p.known).collect::<Vec<_>>(),
        vec![false, true, true]
    );
    assert_eq!(record.raw, raw);
    for flag in [json!(true), json!(null), json!("false"), json!(0)] {
        column.raw["options"]["show_member_name"] = flag;
        assert_eq!(project(Some(&w), &media, &column, &record), before);
    }
}
#[test]
fn assignee_vote_channel_and_unresolved_workspace_keep_existing_content() {
    let media = MediaRegistry::default();
    let w = workspace();
    for kind in ["assignee", "todo_assignee", "vote", "channel"] {
        let (mut column, record) = values(
            kind,
            json!({"key":"field","user":["U1"],"channel":["C1"],"vote":["U1"]}),
        );
        let before = project(Some(&w), &media, &column, &record);
        column.raw["options"] = json!({"show_member_name":false});
        assert_eq!(
            project(Some(&w), &media, &column, &record),
            before,
            "{kind}"
        );
    }
    let (mut c, r) = values("user", json!({"key":"field","user":["U1"]}));
    c.raw["options"] = json!({"show_member_name":false});
    assert!(matches!(
        project(None, &media, &c, &r).1,
        CellContent::Plain
    ));
}
