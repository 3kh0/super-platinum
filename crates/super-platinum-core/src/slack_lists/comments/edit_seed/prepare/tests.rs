use super::*;

#[test]
fn stripped_replacements_use_insert_attributes_without_changing_raw_capture() {
    let original = vec![
        json!({"insert":"example.com","attributes":{"link":"http://example.com"}}),
        json!({"insert":" "}),
        json!({"insert":"@Ada","attributes":{"slackmention":{"id":"U1","label":"Ada"}}}),
        json!({"insert":"\n","attributes":{"code-block":true}}),
    ];
    let before = original.clone();
    let mut document = Document::capture(&original).unwrap();
    strip::formatting(&mut document, &SyntaxObservations::default()).unwrap();
    assert_eq!(
        document.ops().unwrap(),
        vec![
            json!({"insert":"example.com @Ada"}),
            json!({"insert":"\n","attributes":{"code-block":true}}),
        ]
    );
    assert_eq!(original, before);

    let raw_empty = vec![json!({"insert":"retained","attributes":{}})];
    assert_eq!(
        Document::capture(&raw_empty).unwrap().ops().unwrap(),
        raw_empty
    );
    let inline = vec![json!({"insert":"example.com","attributes":{
        "code":true,"link":"http://example.com","opaque":{"raw":[false,7]}
    }})];
    let mut document = Document::capture(&inline).unwrap();
    strip::formatting(&mut document, &SyntaxObservations::default()).unwrap();
    assert_eq!(
        document.ops().unwrap(),
        vec![json!({"insert":"example.com","attributes":{
            "code":true,"opaque":{"raw":[false,7]}
        }})]
    );
}

#[test]
fn actual_plain_source_phase_vectors_match() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let observations = SyntaxObservations::default();
    for (index, vector) in vectors["results"].as_array().unwrap().iter().enumerate() {
        let input = vector["input"].as_array().unwrap();
        let mut document = Document::capture(input).unwrap();
        links::linkify(&mut document, false).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            vector["first"],
            "first {index}"
        );
        embeds::repair(&mut document).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            vector["repair"],
            "repair {index}"
        );
        if vectors["unsupportedIndices"]
            .as_array()
            .unwrap()
            .contains(&json!(index))
        {
            assert!(
                markdown(&mut document, &observations).is_err(),
                "non-document {index}"
            );
            assert!(
                PlainSyntax::capture(input, &observations).is_err(),
                "unsupported {index}"
            );
            continue;
        }
        markdown(&mut document, &observations).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            vector["markdown"],
            "markdown {index}"
        );
        let syntax = PlainSyntax::capture(input, &observations).unwrap();
        assert_eq!(syntax.original_ops(), input);
        assert_eq!(
            json!(syntax.delta_ops()),
            vector["markdown"],
            "public {index}"
        );
        links::linkify(&mut document, true).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            vector["final"],
            "final-link-only {index}"
        );
        let mut document = Document::capture(input).unwrap();
        strip::formatting(&mut document, &observations).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            vector["strip"],
            "strip {index}"
        );
    }
}

#[test]
fn unknown_code_workflow_requires_exact_positive_macro_observation() {
    let token = json!({"type":"slack#/types/channel_id","id":"var1","name":"Channel"});
    let input = vec![json!({"insert":{"workflowtoken":token},"attributes":{"code":true}})];
    assert!(PlainSyntax::capture(&input, &SyntaxObservations::default()).is_err());
    let keep = SyntaxObservations {
        workflow_text: vec![(token.clone(), true)],
    };
    assert_eq!(
        PlainSyntax::capture(&input, &keep).unwrap().delta_ops(),
        input
    );
    let remove = SyntaxObservations {
        workflow_text: vec![(token.clone(), false)],
    };
    assert!(
        PlainSyntax::capture(&input, &remove)
            .unwrap()
            .delta_ops()
            .is_empty()
    );
    let duplicate = SyntaxObservations {
        workflow_text: vec![(token.clone(), true), (token, false)],
    };
    assert!(PlainSyntax::capture(&input, &duplicate).is_err());
    let unrelated = SyntaxObservations {
        workflow_text: vec![(json!({"type":"other"}), true)],
    };
    assert!(PlainSyntax::capture(&input, &unrelated).is_err());
}

#[test]
fn unformatted_unknown_embeds_and_metadata_remain_exact() {
    let input = vec![
        json!({"insert":{"unknown":{"id":"opaque","nested":[1,false]}},"attributes":{"opaque":null,"custom":["x"]}}),
    ];
    let prepared = PlainSyntax::capture(&input, &SyntaxObservations::default()).unwrap();
    assert_eq!(prepared.original_ops(), input);
    assert_eq!(prepared.delta_ops(), input);
}

#[test]
fn insert_only_and_unicode_boundaries_refuse_lossy_changes() {
    for op in [
        json!({"retain":1}),
        json!({"delete":1}),
        json!({"insert":"x","future":true}),
        json!({"insert":1}),
        json!({"insert":{}}),
        json!({"insert":"x","attributes":false}),
    ] {
        assert!(PlainSyntax::capture(&[op], &SyntaxObservations::default()).is_err());
    }
    let original = Document::capture(&[json!({"insert":"\u{1f600}"})]).unwrap();
    assert_eq!(original.len(), 2);
    assert!(original.slice(0, 1).unwrap().ops().is_err());
    assert_eq!(original.ops().unwrap(), vec![json!({"insert":"\u{1f600}"})]);
    let oversized = vec![json!({"insert":"x".repeat(262_145)})];
    assert!(PlainSyntax::capture(&oversized, &SyntaxObservations::default()).is_err());
}

#[test]
fn source_coercible_nonnumeric_borders_refuse_instead_of_becoming_zero() {
    for border in [json!(true), json!("1"), json!([1]), json!({"value":1})] {
        let input = vec![
            json!({"insert":">>> start\nblock"}),
            json!({"insert":"\n","attributes":{"list":"bullet","border":border}}),
            json!({"insert":"middle\n> last"}),
        ];
        assert!(PlainSyntax::capture(&input, &SyntaxObservations::default()).is_err());
    }
}

#[test]
fn code_slug_primitive_property_access_refuses_source_throwing_shapes() {
    for slug in [json!(true), json!("slug"), json!(1)] {
        let input = vec![json!({"insert":"label","attributes":{"code":true,"slackslug":slug}})];
        assert!(PlainSyntax::capture(&input, &SyntaxObservations::default()).is_err());
    }
    let input = vec![json!({"insert":"label","attributes":{"code":true,"slackslug":{}}})];
    assert_eq!(
        PlainSyntax::capture(&input, &SyntaxObservations::default())
            .unwrap()
            .delta_ops(),
        vec![json!({"insert":"label","attributes":{"code":true}})]
    );
}

#[test]
fn first_op_embed_coercion_refuses_only_on_reached_quote_lookahead() {
    for to_string in [Value::Null, json!("not callable"), json!({})] {
        let embed = json!({"slackopaqueblock":{"block":{"type":"context"}},"toString":to_string});
        let input = vec![
            json!({"insert":">>> start\nblock"}),
            json!({"insert":"\n","attributes":{"list":"bullet","border":1}}),
            json!({"insert":"middle\n"}),
            json!({"insert":embed}),
        ];
        assert!(PlainSyntax::capture(&input, &SyntaxObservations::default()).is_err());
        let inactive = vec![json!({"insert":embed})];
        assert_eq!(
            PlainSyntax::capture(&inactive, &SyntaxObservations::default())
                .unwrap()
                .delta_ops(),
            inactive
        );
        let unrecognized = vec![json!({"insert":{"toString":to_string}})];
        assert!(PlainSyntax::capture(&unrecognized, &SyntaxObservations::default()).is_err());
    }
}
