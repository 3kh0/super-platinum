use super::*;
use serde_json::json;

#[test]
fn static_table_supplied_grid_literals_format_bounds_and_refusal_contract() {
    let raw = fixture::block();
    let table = parse(&raw).unwrap();
    assert_eq!(table.rows().len(), 13);
    assert_eq!(table.rows()[0].len(), 3);
    assert_eq!(table.rows()[0][0].plain_text(), "Project");
    assert_eq!(table.rows()[12][0].plain_text(), "Project 12");
    assert_eq!(table.rows()[1][2].plain_text(), "$1,000.00");
    assert_eq!(
        table.column(0),
        Column {
            wrapped: true,
            ..Default::default()
        }
    );
    assert_eq!(table.column(1), Column::default());
    assert_eq!(table.column(2).align, Align::Right);
    assert_eq!(table.column(19), Column::default());
    let Part::Paragraph(runs) = &table.rows()[1][1].0[0] else {
        panic!()
    };
    assert_eq!(runs[0].text, "Ready <literal>");
    assert_eq!(
        runs[0].style,
        Style {
            bold: true,
            italic: true,
            underline: true,
            strike: true,
            code: true
        }
    );
    assert_eq!(runs[1].text, "Release notes");
    assert_eq!(runs[2].text, "👍");
    assert!(table.plain_text().contains("3. Ship"));
    assert!(table.plain_text().contains("a < b\nsecond line"));
    let one = |cell: Value| json!({"type":"table","rows":[[cell]]});
    let rich = |leaf: Value| json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[leaf]}]});
    for value in [Value::Null, json!(false), json!(0), json!(-0.0), json!("")] {
        assert!(parse(&one(value)).unwrap().rows()[0][0].0.is_empty());
    }
    let literal =
        one(json!({"type":"raw_text","text":"\n<https://example.invalid|label> &amp; *raw*\n\n"}));
    assert_eq!(
        parse(&literal).unwrap().rows()[0][0].plain_text(),
        "\n<https://example.invalid|label> &amp; *raw*\n\n"
    );
    let ragged = json!({"type":"table","rows":[[],[{"type":"raw_text","text":"only"}]],"column_settings":[null,{"align":"center"}]});
    let got = parse(&ragged).unwrap();
    assert!(got.rows()[0].is_empty());
    assert_eq!(got.rows()[1].len(), 1);
    assert_eq!(got.column(1).align, Align::Center);
    for label in [json!("supplied"), json!(null)] {
        for url in [json!(null), json!(false), json!(""), json!(" \u{feff}\t")] {
            let leaf = json!({"type":"link","url":url,"text":label});
            assert!(parse(&one(rich(leaf.clone()))).is_none(), "{leaf}");
            let mut nested = leaf;
            nested["elements"] = json!([{"type":"text","text":"nested label"}]);
            assert!(parse(&one(rich(nested))).is_none());
        }
    }
    for leaf in [
        json!({"type":"link","text":"missing URL"}),
        json!({"type":"link","elements":[{"type":"text","text":"missing URL"}]}),
        json!({"type":"file","file_id":"F1"}),
        json!({"type":"button","text":"not passive"}),
        json!({"type":"user","userId":"U1"}),
        json!({"type":"channel","channel_id":"bad"}),
    ] {
        assert!(parse(&one(rich(leaf))).is_none());
    }
    for (leaf, expected) in [
        (
            json!({"type":"link","url":" \u{feff}https://example.invalid/path\t"}),
            "https://example.invalid/path",
        ),
        (
            json!({"type":"link","url":"https://example.invalid","text":"shown"}),
            "shown",
        ),
        (
            json!({"type":"link","url":"https://example.invalid","elements":[{"type":"text","text":"nested"}]}),
            "nested",
        ),
        (json!({"type":"user","user_id":"U123"}), "@U123"),
        (json!({"type":"channel","channel_id":"C123"}), "#C123"),
        (json!({"type":"usergroup","usergroup_id":"S123"}), "@S123"),
        (
            json!({"type":"emoji","name":"workspace_only","skin_tone":3}),
            ":workspace_only::skin-tone-3:",
        ),
        (
            json!({"type":"date","timestamp":1,"fallback":"Supplied date"}),
            "Supplied date",
        ),
    ] {
        assert_eq!(
            parse(&one(rich(leaf))).unwrap().rows()[0][0].plain_text(),
            expected
        );
    }
    let mut huge = one(json!({"type":"raw_text","text":"x".repeat(MAX_BYTES)}));
    assert!(parse(&huge).is_some());
    huge["rows"][0][0]["text"] = json!("x".repeat(MAX_BYTES + 1));
    assert!(parse(&huge).is_none());
    for (rows, ok) in [(100, true), (101, false)] {
        assert_eq!(parse(&json!({"type":"table","rows":vec![vec![json!({"type":"raw_text","text":"x"})];rows]})).is_some(),ok);
    }
    for (columns, ok) in [(20, true), (21, false)] {
        assert_eq!(
            parse(&json!({"type":"table","rows":[vec![json!(null);columns]]})).is_some(),
            ok
        );
    }
    for invalid in [
        json!({"type":"table","rows":[]}),
        json!({"type":"table","rows":[[]]}),
        json!({"type":"data_table","rows":[[null]]}),
        json!({"type":"table","rows":[[null]],"columnSettings":[]}),
        json!({"type":"table","rows":[[null]],"column_settings":[{"isWrapped":true}]}),
        json!({"type":"table","rows":[[null]],"column_settings":[{"is_wrapped":"true"}]}),
        one(json!({"type":"raw_number","value":"1","text":"1"})),
    ] {
        assert!(parse(&invalid).is_none(), "{invalid}");
    }
    let mut deep = json!({"type":"text","text":"deep"});
    for _ in 0..10 {
        deep = json!({"type":"link","url":"https://example.invalid","elements":[deep]});
    }
    assert!(parse(&one(rich(deep))).is_none());
    let empty_runs = json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":vec![json!({"type":"text","text":""});MAX_RUNS+1]}]});
    assert!(parse(&one(empty_runs)).is_none());
    let workspace = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
    let ctx = crate::blocks::BlockCtx::new(&workspace.core.workspaces["T1"], &workspace.media);
    assert!(
        crate::blocks::block_nodes(ctx, &raw).is_empty(),
        "generic Home/modal parser unchanged"
    );
    assert!(matches!(
        &crate::blocks::message_block_nodes(ctx, &raw)[0],
        crate::model::RichNode::Table(_)
    ));
    let inert =
        crate::message_vm::preview::nodes(vec![crate::model::RichNode::Table(table.clone())]);
    assert_eq!(
        inert,
        vec![crate::model::RichNode::Text(table.plain_text())]
    );
}
