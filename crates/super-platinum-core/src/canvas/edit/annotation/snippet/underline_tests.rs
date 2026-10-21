use super::*;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../fixtures/underline.json")).unwrap()
}
fn section(source: &str, style: u64) -> Section {
    let snapshot = super::super::tests::fixture();
    let original = snapshot.section("item").unwrap();
    let text = RawMessage::from_fields(&[F::bytes(1, source.as_bytes()).unwrap()]).unwrap();
    let content = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
    Section::from_raw(
        original
            .raw()
            .replace_fields(10, &[F::uint(10, style).unwrap()])
            .unwrap()
            .replace_fields(12, &[F::message(12, &content).unwrap()])
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn original_paragraph_heading_vectors_preserve_rich_underline_and_exact_slack_markdown() {
    for vector in vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "externalSnippet")
    {
        let section = section(
            vector["canonical"].as_str().unwrap(),
            vector["style"].as_u64().unwrap(),
        );
        let before = section.raw().clone();
        let draft = super::super::super::text::canonical_draft(&section).unwrap();
        let nodes = styled::nodes(&section, &draft).unwrap();
        assert_eq!(
            markdown_nodes(nodes.clone()).unwrap(),
            vector["markdown"].as_str().unwrap()
        );
        // The original oracle runs the leaf collector, not block normalization.
        let mut leaves = Vec::new();
        for node in &nodes {
            rich_node(node, &serde_json::Map::new(), &mut leaves).unwrap();
        }
        assert_eq!(
            json!({"type":"rich_text_section","elements":leaves}),
            vector["rich"]
        );
        assert_eq!(section.raw(), &before);
    }
}

#[test]
fn original_typed_link_vectors_keep_outer_but_not_inner_underline() {
    for vector in vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "typedSlackLink")
    {
        let section = section(vector["html"].as_str().unwrap(), 0);
        let draft = super::super::super::text::canonical_draft(&section).unwrap();
        let rich = rich_nodes(&section_nodes(&section, &draft).unwrap()).unwrap();
        assert_eq!(rich["elements"][0], vector["rich"]);
    }
}

#[test]
fn ordinary_link_styled_runs_preserve_underlined_and_plain_segments() {
    let section = section(
        "<a href=\"https://example.test\">plain<u>under<b>both</b></u>end</a>",
        0,
    );
    let draft = super::super::super::text::canonical_draft(&section).unwrap();
    let rich = rich_nodes(&section_nodes(&section, &draft).unwrap()).unwrap();
    assert_eq!(
        rich["elements"][0]["elements"],
        json!([
            {"type":"link","text":"plain","url":"https://example.test"},
            {"type":"link","text":"under","url":"https://example.test","style":{"underline":true}},
            {"type":"link","text":"both","url":"https://example.test","style":{"underline":true,"bold":true}},
            {"type":"link","text":"end","url":"https://example.test"}
        ])
    );
}

#[test]
fn inline_subtree_preserves_inner_underline_without_copying_outer_formatting() {
    const ID: &str = "temp:C:ABC0123456789abcdef012345678";
    let section = section("body", 0);
    let source =
        format!("<annotation id=\"{ID}\"><u><b>\u{4e2d}\u{1f98a}&amp;</b></u></annotation>");
    let snippet = inline::capture(&section, ID, "Q1", "U1", &source).unwrap();
    assert_eq!(snippet["mrkdwn"], "*\u{4e2d}\u{1f98a}&*");
    let rich: Value = serde_json::from_str(snippet["richText"].as_str().unwrap()).unwrap();
    assert_eq!(
        rich["blocks"][0]["elements"][0]["elements"][0]["style"],
        json!({"bold":true,"underline":true})
    );
    let source = format!("<u><annotation id=\"{ID}\">inner</annotation></u>");
    let snippet = inline::capture(&section, ID, "Q1", "U1", &source).unwrap();
    let rich: Value = serde_json::from_str(snippet["richText"].as_str().unwrap()).unwrap();
    assert!(
        rich["blocks"][0]["elements"][0]["elements"][0]
            .get("style")
            .is_none()
    );
    for body in [
        "<u class='x'>bad</u>",
        "<u><control>x</control></u>",
        "<u style='x'>bad</u>",
    ] {
        let source = format!("<annotation id=\"{ID}\">{body}</annotation>");
        assert!(inline::capture(&section, ID, "Q1", "U1", &source).is_err());
    }
}

#[test]
fn external_body_keeps_text_codepoints_and_legacy_nonunderline_path() {
    for source in [
        "a\u{a0}b\u{200b}\u{feff}",
        "<u>a\u{a0}b\u{200b}\u{feff}</u>",
    ] {
        let section = section(source, 0);
        let draft = super::super::super::text::canonical_draft(&section).unwrap();
        let nodes = section_nodes(&section, &draft).unwrap();
        assert_eq!(markdown_nodes(nodes).unwrap(), "a\u{a0}b\u{200b}\u{feff}\n");
    }
}
