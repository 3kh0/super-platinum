use super::*;

#[test]
fn named_table_handles_quotes_case_and_multiple_codepoints() {
    assert_eq!(NAMED.len(), 2125);
    assert_eq!(
        decode_entities("&amp;&AMP;&quot;&apos;&lt;&gt;&nbsp;"),
        "&&\"'<>\u{a0}"
    );
    assert_eq!(
        decode_entities("&NotEqualTilde; &fjlig; &CounterClockwiseContourIntegral;"),
        "\u{2242}\u{338} fj \u{2233}"
    );
    assert_eq!(decode_entities("&Amp;&QUOT;&unknown;"), "&Amp;\"&unknown;");
}
#[test]
fn decode_is_one_pass_and_does_not_reparse_slack_syntax() {
    assert_eq!(
        decode_entities("&lt;@U1&gt; &lt;#C1&gt; &lt;https://example.test|label&gt;"),
        "<@U1> <#C1> <https://example.test|label>"
    );
    assert_eq!(
        decode_entities("&amp;lt;@U1&amp;gt; &#38;amp; &amp;amp;"),
        "&lt;@U1&gt; &amp; &amp;"
    );
    assert_eq!(
        decode_entities("plain & text \u{4e2d}\u{1f642}"),
        "plain & text \u{4e2d}\u{1f642}"
    );
}
#[test]
fn decimal_hex_zero_and_maximum_are_literal_codepoints() {
    assert_eq!(decode_entities("&#65;&#x41;&#X41;&#00065;"), "AAAA");
    assert_eq!(
        decode_entities("&#128578;&#x1F642;&#1114111;"),
        "\u{1f642}\u{1f642}\u{10ffff}"
    );
    assert_eq!(decode_entities("&#0;&#128;&#x80;"), "\u{0}\u{80}\u{80}");
}
#[test]
fn malformed_unknown_missing_semicolons_and_numeric_overflow_are_retained() {
    for value in [
        "&amp",
        "&#65",
        "&#x41",
        "&#;",
        "&#x;",
        "&#65foo;",
        "&#x41zz;",
        "&#-1;",
        "&name_1;",
        "&;",
        "&#1114112;",
        "&#x110000;",
        "&#9999999999999999999999999;",
    ] {
        assert_eq!(decode_entities(value), value);
    }
    assert_eq!(decode_entities("&&amp; &#bogus;&amp;"), "&& &#bogus;&");
}
#[test]
fn isolated_surrogates_are_explicit_native_preservation_boundary() {
    assert_eq!(
        decode_entities("&#55296;&#xDFFF;&#xD83D;&#xDE42;"),
        "&#55296;&#xDFFF;&#xD83D;&#xDE42;"
    );
    assert_eq!(decode_entities("&#55295;&#57344;"), "\u{d7ff}\u{e000}");
}
