//! Original retained zero-base history, explicitly bound to a modern host ID.
//! This is not the positive fixture with a legacy ID and a flipped base field.
use super::*;

pub(super) const ID: &str = "temp:C:Dte0000000000000000000000000";

pub(super) fn wire() -> RawMessage {
    // Actual original-client retained-zero-base vector: base0, lastSaved1000,
    // one FEFF-backed history, processed/basehead and sequenceHeads preserved.
    // Only host Section/Session identity and independent offline lease differ.
    let seed: Value = serde_json::from_str(include_str!("reuse-zero-original.json")).unwrap();
    let bytes = STANDARD.decode(seed["session"].as_str().unwrap()).unwrap();
    RawMessage::decode(&bytes)
        .unwrap()
        .replace_fields(1, &[t(1, ID)])
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap()
}

pub(super) fn html(unicode: bool) -> &'static str {
    if unicode {
        "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='temp:C:Dte0000000000000000000000000'></p></div>"
    } else {
        "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='temp:C:Dte0000000000000000000000000'></p></div>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_host_binding_preserves_original_zero_history_and_metadata() {
        let seed: Value = serde_json::from_str(include_str!("reuse-zero-original.json")).unwrap();
        let original =
            RawMessage::decode(&STANDARD.decode(seed["session"].as_str().unwrap()).unwrap())
                .unwrap();
        let bound = wire();
        assert_eq!(ID.len(), 35);
        assert_eq!(bound.string(1).unwrap(), Some(ID));
        assert_eq!(bound.uint(2).unwrap(), Some(0));
        assert_eq!(bound.uint(3).unwrap(), Some(1000));
        assert_eq!(bound.uint(4).unwrap(), Some(8_000_000_000_000_000));
        assert_eq!(
            bound.remove(1).unwrap().remove(4).unwrap(),
            original.remove(1).unwrap()
        );
        assert!(bound.has(5) && bound.has(6) && bound.has(8) && bound.has(11));
        assert!(!bound.has(9) && !bound.has(10) && !bound.has(12));
    }
}
