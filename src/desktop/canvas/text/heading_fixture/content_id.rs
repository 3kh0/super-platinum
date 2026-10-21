//! Independent full-attribute byte custody for the named closed variant.
//! Known fields match retained projection; unknown bytes test native preservation,
//! not an observed server/Automerge map contract.
use super::*;

pub(super) fn attributes(id: &str) -> RawMessage {
    msg(vec![
        s(29, &format!("content-{id}")),
        u(4, 0),  // original descriptor: vertical_margin=false
        u(13, 0), // original descriptor: default_content=0
        F::bytes(177, &[0, 255, 128]).unwrap(),
        s(177, "attrs-tail"),
    ])
}

fn decode(value: &Value) -> Result<RawMessage, String> {
    let bytes = STANDARD
        .decode(value.as_str().ok_or("Missing heading wire")?)
        .map_err(|_| "Invalid heading wire encoding")?;
    RawMessage::decode(&bytes).map_err(|_| "Invalid heading wire".into())
}

fn exact(raw: &RawMessage, id: &str) -> Result<(), String> {
    let actual = raw
        .message(16)
        .map_err(|_| "Invalid heading attributes")?
        .ok_or("Missing full heading attributes")?;
    if actual.raw() != attributes(id).raw() {
        return Err(format!("Heading attributes changed: {id}"));
    }
    Ok(())
}

pub(in crate::canvas::text) fn validate_proof(proof: &Value) -> Result<(), String> {
    for row in proof["sections"]
        .as_array()
        .ok_or("Missing heading inventory")?
    {
        let id = row["id"].as_str().ok_or("Missing heading identity")?;
        if id.starts_with("heading-") {
            exact(&decode(&row["raw"])?, id)?;
        }
    }
    for write in proof["writes"].as_array().ok_or("Missing heading writes")? {
        let data = decode(&write["documentData"])?;
        let row = data
            .message(1)
            .map_err(|_| "Invalid heading update")?
            .ok_or("Missing heading update")?;
        let id = write["section"]
            .as_str()
            .ok_or("Missing heading write identity")?;
        exact(&row, id)?;
        if row.fields().iter().map(|f| f.tag()).collect::<Vec<_>>()
            != [1, 2, 9, 12, 13, 16, 19, 20, 33, 38]
        {
            return Err("Content-id heading update changed field contract".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{EditSession, text::TextDraft};

    #[test]
    fn content_id_heading_fixture_resends_exact_known_zero_and_unknown_attributes() {
        let source = initial_with(true);
        let mut session = EditSession::new();
        for (id, level) in [("heading-one", 1), ("heading-two", 2), ("heading-three", 3)] {
            let original = source.section(id).unwrap();
            exact(original.raw(), id).unwrap();
            let intent = TextIntent::capture(&source, id, 100).unwrap();
            assert_eq!(intent.heading_level(), Some(level));
            let prepared = intent
                .prepare(
                    &source,
                    &mut session,
                    &TextDraft::from_rtml("<b>Changed</b><br><br>tail").unwrap(),
                    100,
                )
                .unwrap();
            let sent = prepared.document_data().message(1).unwrap().unwrap();
            exact(&sent, id).unwrap();
            assert_eq!(sent.message(16).unwrap(), original.attributes().unwrap());
            for tag in [8, 10, 29, 177] {
                assert!(!sent.has(tag));
            }
            let cleared = sent.replace_fields(16, &[m(16, &msg(vec![]))]).unwrap();
            assert!(exact(&cleared, id).is_err());
            assert!(exact(&sent, "heading-twin").is_err());
        }
        assert_eq!(
            source
                .section("neighbor")
                .unwrap()
                .attributes()
                .unwrap()
                .unwrap()
                .raw(),
            b""
        );
        assert_ne!(attributes("heading-two"), attributes("heading-twin"));
    }
}
