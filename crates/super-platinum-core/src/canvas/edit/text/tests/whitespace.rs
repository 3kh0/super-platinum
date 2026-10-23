use super::*;

#[test]
fn decoded_leading_feff_is_content_and_roundtrips_without_normalization() {
    for raw in [
        "\u{feff}",
        "\u{feff} ",
        "\u{feff}\u{feff}",
        " \u{feff}\u{a0}",
        "<br>\u{feff}",
        "\u{feff}<br>",
        " \u{feff}",
        "\u{feff}<b>x</b>",
        "<b>\u{feff}</b>",
        "\u{2028}<br>\u{2029}",
    ] {
        let d = TextDraft::from_rtml(raw).unwrap();
        assert_eq!(d.rtml().unwrap(), raw, "raw={raw:?}");
        let expected = raw.replace("<b>", "").replace("</b>", "");
        assert_eq!(d.model().unwrap().text, expected, "raw={raw:?}");
        assert!(TextDraft::from_rtml(&d.rtml().unwrap()).unwrap() == d);
    }
    // Existing entity decoding remains equivalent content, without making its
    // distinct raw spelling canonical for source-bound clearing.
    assert_eq!(
        TextDraft::from_rtml("&#65279;").unwrap().rtml().unwrap(),
        "\u{feff}"
    );
    assert_eq!(
        TextDraft::from_rtml("\u{feff}\t").unwrap().rtml().unwrap(),
        "\u{feff} "
    );
    for raw in [
        "\u{feff}\0",
        "\u{feff}<unknown>x</unknown>",
        "\u{feff}<br x='y'>",
    ] {
        assert!(TextDraft::from_rtml(raw).is_err(), "raw={raw:?}");
    }
    let limit = format!("\u{feff}{}", "x".repeat(draft::MAX_TEXT_BYTES - 3));
    assert_eq!(TextDraft::from_rtml(&limit).unwrap().rtml().unwrap(), limit);
    assert!(TextDraft::from_rtml(&(limit + "x")).is_err());
}
