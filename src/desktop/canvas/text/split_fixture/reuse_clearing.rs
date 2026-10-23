//! Finite source-backed nonliteral reused-child fixtures. No history regeneration.
use super::*;
#[path = "reuse_whitespace.rs"]
mod whitespace;

pub(super) fn selector() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_REUSE_CONTENT").as_deref() {
        Ok("spaces") => "spaces",
        Ok("nbsp") => "nbsp",
        Ok("br") => "br",
        Ok("mixed") => "mixed",
        Ok("mixed-br") => "mixed-br",
        Ok("separators") => "separators",
        Ok("feff-leading") => "feff-leading",
        Ok("feff-interior") => "feff-interior",
        Ok("feff-post-br") => "feff-post-br",
        _ => "empty",
    }
}
fn admits(value: &str, reused: bool) -> bool {
    value == "empty"
        || (reused && (matches!(value, "spaces" | "nbsp" | "br") || whitespace::supports(value)))
}
pub(super) fn valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_REUSE_CONTENT") {
        Ok(value) => admits(&value, is_reuse()),
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
pub(super) fn rtml(shape: &str) -> &'static str {
    if whitespace::supports(shape) {
        return whitespace::rtml(shape);
    }
    match shape {
        "spaces" => "   ",
        "nbsp" => "\u{a0}",
        "br" => "<br>",
        "empty" => "",
        _ => panic!("unknown fixed reused content"),
    }
}
pub(super) fn wire(zero: bool, shape: &str) -> RawMessage {
    if whitespace::supports(shape) {
        return whitespace::wire(zero, shape);
    }
    // Original positive clean session or real base0 empty-bootstrap -> CONTENT
    // report -> controlled canonical ACK hydrate. The fixture only binds the
    // modern host ID for zero and adds its independent offline lease.
    let seeds: Value = serde_json::from_str(include_str!("reuse-clearing-original.json")).unwrap();
    let original_shape = if shape == "spaces" {
        "ascii-space"
    } else {
        shape
    };
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|seed| {
            seed["cohort"] == if zero { "zero" } else { "positive" }
                && seed["shape"] == original_shape
        })
        .unwrap();
    let bytes = STANDARD
        .decode(seed["sessionBase64"].as_str().unwrap())
        .unwrap();
    RawMessage::decode(&bytes)
        .unwrap()
        .replace_fields(1, &[t(1, id(zero))])
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap()
}

pub(super) fn html(zero: bool, unicode: bool, shape: &str) -> &'static str {
    if whitespace::supports(shape) {
        return whitespace::html(zero, unicode, shape);
    }
    match (zero, unicode, shape) {
        (false, false, "spaces") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'>   </p></div>"
        }
        (false, false, "nbsp") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'>\u{a0}</p></div>"
        }
        (false, false, "br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'><br></p></div>"
        }
        (false, true, "spaces") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'>   </p></div>"
        }
        (false, true, "nbsp") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'>\u{a0}</p></div>"
        }
        (false, true, "br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'><br></p></div>"
        }
        (true, false, "spaces") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'>   </p></div>"
        }
        (true, false, "nbsp") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'>\u{a0}</p></div>"
        }
        (true, false, "br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'><br></p></div>"
        }
        (true, true, "spaces") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'>   </p></div>"
        }
        (true, true, "nbsp") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'>\u{a0}</p></div>"
        }
        (true, true, "br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'><br></p></div>"
        }
        _ => panic!("nonliteral fixture shape required"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selectors_are_finite_and_nonliteral_requires_reuse() {
        assert!(admits("empty", false));
        for shape in [
            "spaces",
            "nbsp",
            "br",
            "mixed",
            "mixed-br",
            "separators",
            "feff-leading",
            "feff-interior",
            "feff-post-br",
        ] {
            assert!(admits(shape, true));
            assert!(!admits(shape, false));
        }
        assert!(!admits("unknown", true));
        assert!(!admits("feff", true));
    }
    #[test]
    fn host_binding_preserves_every_original_history_and_ack_metadata() {
        let seeds: Value =
            serde_json::from_str(include_str!("reuse-clearing-original.json")).unwrap();
        for zero in [false, true] {
            for shape in ["spaces", "nbsp", "br"] {
                let source_shape = if shape == "spaces" {
                    "ascii-space"
                } else {
                    shape
                };
                let seed = seeds["seeds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|seed| {
                        seed["cohort"] == if zero { "zero" } else { "positive" }
                            && seed["shape"] == source_shape
                    })
                    .unwrap();
                let original = RawMessage::decode(
                    &STANDARD
                        .decode(seed["sessionBase64"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                let bound = wire(zero, shape);
                assert_eq!(
                    bound.remove(1).unwrap().remove(4).unwrap(),
                    original.remove(1).unwrap()
                );
                assert_eq!(bound.string(1).unwrap(), Some(id(zero)));
                assert_eq!(bound.uint(2).unwrap(), Some(if zero { 0 } else { 1000 }));
                assert_eq!(bound.uint(3).unwrap(), Some(1000));
                assert!(!bound.has(10) && !bound.has(12));
                assert_eq!(seed["rtml"], rtml(shape));
                for unicode in [false, true] {
                    assert!(html(zero, unicode, shape).contains(id(zero)));
                }
            }
        }
    }
}
