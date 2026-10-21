use super::*;
use serde_json::json;

fn loaded() -> (ShellState, Source, Value) {
    let mut shell = super::super::tests::shell();
    let (owner, serial, target, cursor) = controller::begin(&mut shell, false).unwrap();
    let raw = json!({"type":"canvas","file_id":"FLISTFILE1",
        "url":"https://fixture.invalid/files/FLISTFILE1","opaque":{"keep":true},"style":{"bold":true}});
    let page = Page::parse(&target, json!({"ok":true,"messages":[{
        "ts":"1700000001.000001","thread_ts":target.root_ts(),"user":"U1",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw.clone(),raw.clone()]}]}]
    }]})).unwrap();
    let projection = Projection::observed(json!({"ok":true,"assignments":{}})).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded { page, projection }),
    );
    let row = shell.slack_lists.comments.entry.as_ref().unwrap().rows[0].clone();
    let source = Source::capture(&shell, owner.serial, &row).unwrap();
    (shell, source, raw)
}

#[test]
fn original_duplicate_occurrences_keep_order_and_exact_raw_authority() {
    let (shell, source, raw) = loaded();
    assert!(source.current(&shell));
    assert_eq!(source.occurrences.len(), 2);
    for index in 0..2 {
        assert!(source.matches_occurrence(&source.owner.scope.team, &raw, index));
    }
    assert!(!source.matches_occurrence(&source.owner.scope.team, &raw, 2));
    assert!(!source.matches_occurrence("FOREIGN", &raw, 0));
    let mut changed = raw.clone();
    changed["opaque"]["keep"] = json!(false);
    assert!(!source.matches_occurrence(&source.owner.scope.team, &changed, 0));
    // A visible token is not enough: absent canonical file authority refuses.
    assert!(
        source
            .occurrence(&shell, &source.owner.scope.team, &raw, 0)
            .is_none()
    );
}

#[test]
fn source_retires_on_original_body_root_account_socket_and_deletion() {
    let (mut changed, source, _) = loaded();
    Arc::make_mut(&mut changed.slack_lists.comments.entry.as_mut().unwrap().rows)[0]
        .body
        .push(RichNode::Text("new body".into()));
    assert!(!source.current(&changed));
    let (mut changed, source, _) = loaded();
    changed.core.account_epoch = changed.core.account_epoch.wrapping_add(1);
    assert!(!source.current(&changed));
    let (mut changed, source, _) = loaded();
    changed
        .core
        .workspaces
        .get_mut(&source.owner.scope.team)
        .unwrap()
        .rt_generation += 1;
    assert!(!source.current(&changed));
    let (mut changed, source, _) = loaded();
    changed
        .slack_lists
        .comments
        .entry
        .as_mut()
        .unwrap()
        .deleted
        .insert(source.row.ts.clone());
    assert!(!source.current(&changed));
    let (mut changed, source, _) = loaded();
    let mut record = source.target.record_raw().clone();
    record["thread_ts"] = json!("1800000000.000001");
    changed.slack_lists.comments.entry.as_mut().unwrap().target = source
        .target
        .refresh(
            &json!({"ok":true,"file":source.target.document_raw()}),
            &json!({"ok":true,"record":record}),
            false,
        )
        .unwrap();
    assert!(!source.current(&changed));
    let (mut changed, source, _) = loaded();
    controller::close_shell(&mut changed);
    source.refused(&mut changed);
    assert!(
        changed
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );
    assert!(!source.current(&changed));
}

#[test]
fn source_tolerates_unrelated_reaction_projection_but_not_duplicate_rows() {
    let (mut shell, source, _) = loaded();
    Arc::make_mut(&mut shell.slack_lists.comments.entry.as_mut().unwrap().rows)[0].author =
        "Renamed author".into();
    assert!(source.current(&shell));
    let rows = &mut shell.slack_lists.comments.entry.as_mut().unwrap().rows;
    let duplicate = rows[0].clone();
    Arc::make_mut(rows).push(duplicate);
    assert!(!source.current(&shell));
}

#[test]
fn nested_body_traversal_counts_only_file_occurrences_in_render_order() {
    let file = |id: &str| RichNode::FileMention {
        team: "T1".into(),
        raw: json!({"type":"file","file_id":id}),
        fallback: id.into(),
    };
    let nodes = vec![
        RichNode::Section {
            text: vec![file("first")],
            fields: vec![vec![RichNode::Quote(vec![file("second")])]],
            accessory: Some(Box::new(file("third"))),
        },
        RichNode::List {
            ordered: false,
            indent: 0,
            offset: 0,
            items: vec![vec![file("fourth")]],
        },
    ];
    let mut result = Vec::new();
    collect(&nodes, &mut result);
    assert_eq!(
        result
            .iter()
            .map(|(_, v)| v["file_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["first", "second", "third", "fourth"]
    );
}
