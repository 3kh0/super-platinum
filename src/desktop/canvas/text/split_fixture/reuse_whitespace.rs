//! Exact retained original whitespace-source sessions; no native input widening.
use super::*;

pub(super) fn supports(shape: &str) -> bool {
    matches!(
        shape,
        "mixed" | "mixed-br" | "separators" | "feff-leading" | "feff-interior" | "feff-post-br"
    )
}
pub(super) fn rtml(shape: &str) -> &'static str {
    match shape {
        "mixed" => {
            " \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}"
        }
        "mixed-br" => {
            " \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}<br>\u{3000}\u{205f}\u{202f}\u{200b}\u{200a}\u{2009}\u{2008}\u{2007}\u{2006}\u{2005}\u{2004}\u{2003}\u{2002}\u{2001}\u{2000}\u{1680}\u{a0} "
        }
        "separators" => " \u{2028}\u{a0}\u{2029}\u{200b}",
        "feff-leading" => "\u{feff} ",
        "feff-interior" => " \u{feff}\u{a0}",
        "feff-post-br" => "<br>\u{feff}",
        _ => panic!("unknown whitespace fixture"),
    }
}
fn source_shape(shape: &str) -> &'static str {
    match shape {
        "mixed" => "mixed-w",
        "mixed-br" => "mixed-w-br",
        "separators" => "ls-ps",
        "feff-leading" => "feff-leading",
        "feff-interior" => "feff-interior",
        "feff-post-br" => "feff-post-br",
        _ => panic!("unknown whitespace fixture"),
    }
}
pub(super) fn wire(zero: bool, shape: &str) -> RawMessage {
    // Frozen original sessions: clean positive, or actual base0 bootstrap +
    // CONTENT report + controlled ACK hydrate. All six source forms retain the
    // exact original history; only host identity and independent lease are bound.
    let seeds: Value =
        serde_json::from_str(include_str!("reuse-whitespace-original.json")).unwrap();
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|seed| {
            seed["cohort"] == if zero { "zero" } else { "positive" }
                && seed["shape"] == source_shape(shape)
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
    match (zero, unicode, shape) {
        (false, false, "mixed") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}</p></div>"
        }
        (false, false, "mixed-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}<br>\u{3000}\u{205f}\u{202f}\u{200b}\u{200a}\u{2009}\u{2008}\u{2007}\u{2006}\u{2005}\u{2004}\u{2003}\u{2002}\u{2001}\u{2000}\u{1680}\u{a0} </p></div>"
        }
        (false, false, "separators") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'> \u{2028}\u{a0}\u{2029}\u{200b}</p></div>"
        }
        (false, false, "feff-leading") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'>\u{feff} </p></div>"
        }
        (false, false, "feff-interior") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'> \u{feff}\u{a0}</p></div>"
        }
        (false, false, "feff-post-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'><br>\u{feff}</p></div>"
        }
        (false, true, "mixed") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}</p></div>"
        }
        (false, true, "mixed-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}<br>\u{3000}\u{205f}\u{202f}\u{200b}\u{200a}\u{2009}\u{2008}\u{2007}\u{2006}\u{2005}\u{2004}\u{2003}\u{2002}\u{2001}\u{2000}\u{1680}\u{a0} </p></div>"
        }
        (false, true, "separators") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'> \u{2028}\u{a0}\u{2029}\u{200b}</p></div>"
        }
        (false, true, "feff-leading") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'>\u{feff} </p></div>"
        }
        (false, true, "feff-interior") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'> \u{feff}\u{a0}</p></div>"
        }
        (false, true, "feff-post-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'><br>\u{feff}</p></div>"
        }
        (true, false, "mixed") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}</p></div>"
        }
        (true, false, "mixed-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}<br>\u{3000}\u{205f}\u{202f}\u{200b}\u{200a}\u{2009}\u{2008}\u{2007}\u{2006}\u{2005}\u{2004}\u{2003}\u{2002}\u{2001}\u{2000}\u{1680}\u{a0} </p></div>"
        }
        (true, false, "separators") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'> \u{2028}\u{a0}\u{2029}\u{200b}</p></div>"
        }
        (true, false, "feff-leading") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'>\u{feff} </p></div>"
        }
        (true, false, "feff-interior") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'> \u{feff}\u{a0}</p></div>"
        }
        (true, false, "feff-post-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'><br>\u{feff}</p></div>"
        }
        (true, true, "mixed") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}</p></div>"
        }
        (true, true, "mixed-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'> \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{200b}\u{202f}\u{205f}\u{3000}<br>\u{3000}\u{205f}\u{202f}\u{200b}\u{200a}\u{2009}\u{2008}\u{2007}\u{2006}\u{2005}\u{2004}\u{2003}\u{2002}\u{2001}\u{2000}\u{1680}\u{a0} </p></div>"
        }
        (true, true, "separators") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'> \u{2028}\u{a0}\u{2029}\u{200b}</p></div>"
        }
        (true, true, "feff-leading") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'>\u{feff} </p></div>"
        }
        (true, true, "feff-interior") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'> \u{feff}\u{a0}</p></div>"
        }
        (true, true, "feff-post-br") => {
            "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'><br>\u{feff}</p></div>"
        }
        _ => panic!("unknown whitespace fixture"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_selected_seed_preserves_exact_source_history_and_canonical_metadata() {
        let seeds: Value =
            serde_json::from_str(include_str!("reuse-whitespace-original.json")).unwrap();
        for zero in [false, true] {
            for shape in [
                "mixed",
                "mixed-br",
                "separators",
                "feff-leading",
                "feff-interior",
                "feff-post-br",
            ] {
                let seed = seeds["seeds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|seed| {
                        seed["cohort"] == if zero { "zero" } else { "positive" }
                            && seed["shape"] == source_shape(shape)
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
                assert_eq!(bound.uint(2).unwrap(), Some(if zero { 0 } else { 1000 }));
                assert_eq!(bound.uint(3).unwrap(), Some(1000));
                assert_eq!(bound.string(1).unwrap(), Some(id(zero)));
                assert!(!bound.has(10) && !bound.has(12));
                assert_eq!(seed["rtml"], rtml(shape));
                for unicode in [false, true] {
                    assert!(html(zero, unicode, shape).contains(rtml(shape)));
                }
            }
        }
        for excluded in ["feff", "unknown"] {
            assert!(!supports(excluded));
        }
    }
}
