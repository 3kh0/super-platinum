use super::*;
use crate::{
    canvas::section_titles,
    slack::api::{CanvasSectionsRequest, custom::texty::file_title_source},
};
use serde_json::json;
fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn all_original_utf16_formatter_vectors_remain_code_unit_exact() {
    // Corrected UTF8 adapter, independently crosschecked against direct JS
    // labelUtf16 arrays. No invalid scalar String is parsed by serde_json.
    let vectors: Value = serde_json::from_str(include_str!("original-utf16.json")).unwrap();
    let mut checked = 0;
    for vector in vectors["vectors"].as_array().unwrap() {
        let status = match vector["variant"].as_str().unwrap() {
            "deleted" => SectionVariant::Deleted,
            "hidden" => SectionVariant::Private,
            _ => SectionVariant::Section,
        };
        let name = vector["options"]["labelOverride"]
            .as_str()
            .unwrap_or("Canvas title");
        let section = vector["state"]["canvasSectionRichText"]["FSTATUS0001"].get("s1");
        let result = format(
            name,
            section,
            status,
            vector["options"]["hideTitle"] == true,
            &mut |_| Err(Unavailable::UnsupportedTitle),
        )
        .unwrap();
        assert_eq!(
            result.units(),
            units(&vector["labelUtf16"]),
            "{}",
            vector["name"]
        );
        assert_eq!(
            result.preview_units(),
            units(&vector["previewUtf16"]),
            "{}",
            vector["name"]
        );
        if vector["name"] == "unicode-split-surrogate" {
            assert!(String::from_utf16(result.units()).is_err());
            assert_eq!(
                &result.units()[result.units().len() - 2..],
                &[0xd83d, 0x2026]
            );
            let mut changed = result.units().to_vec();
            let at = changed.len() - 2;
            changed[at] = 0xd83e;
            assert_eq!(
                String::from_utf16_lossy(&changed),
                String::from_utf16_lossy(result.units())
            );
            assert!(!result.matches_units(&changed));
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
}
#[test]
fn original_first_child_projection_preserves_opaque_data_without_source_mutation() {
    let vector: Value = serde_json::from_str(include_str!("original-nested.json")).unwrap();
    let input = vector["input"].clone();
    assert_eq!(section_preview_block(&input).unwrap(), vector["output"]);
    assert_eq!(input, vector["input"]);
}
fn observed(file: &str) -> file_title_source::ReferenceObservation {
    let mut store = file_title_source::Store::default();
    let response = file_title_source::FileReferenceResponse::decode(
        file,
        &serde_json::to_vec(
            &json!({"ok":true,"file":{"id":file,"mode":"quip","filetype":"quip","title":"Name"}}),
        )
        .unwrap(),
    )
    .unwrap();
    store.ingest_reference("Tfixture", &response).unwrap()
}
#[test]
fn source_bound_public_label_distinguishes_missing_empty_and_wrong_file() {
    let mut store = section_titles::Store::default();
    let file = observed("F1");
    let missing = store.observe("F1", "s1").unwrap();
    let label = section_label(&file, &missing, "Name", CanvasSectionText::english()).unwrap();
    assert_eq!(String::from_utf16(label.units()).unwrap(), "Name: \u{2026}");
    assert_eq!(label.variant(), SectionVariant::Missing);
    assert!(
        section_label(
            &observed("F2"),
            &missing,
            "Name",
            CanvasSectionText::english()
        )
        .is_err()
    );
    let request = CanvasSectionsRequest::new("F1", &["s1".into()]).unwrap();
    let read = store.capture_read(&request).unwrap();
    let raw = json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":""}]}]});
    assert!(
        store
            .apply(
                &read,
                &request
                    .decode(json!({"ok":true,"sections":[{"id":"s1","rich_text_blocks":[raw]}]}))
            )
            .unwrap()
    );
    let empty = store.observe("F1", "s1").unwrap();
    assert!(!missing.current(&store));
    let label = section_label(&file, &empty, "Name", CanvasSectionText::english()).unwrap();
    assert_eq!(String::from_utf16(label.units()).unwrap(), "Name: ");
    assert_eq!(label.variant(), SectionVariant::Section);
}

#[test]
fn original_mixed_known_leaves_and_first_child_lists_use_existing_leaf_rules() {
    let data: Value = serde_json::from_str(include_str!("original-mixed.json")).unwrap();
    for vector in data["vectors"].as_array().unwrap() {
        let section = &vector["state"]["canvasSectionRichText"]["FSECTION001"]["temp:C:0123456789abcdef01234567"];
        let unchanged = section.clone();
        let projected = section_preview_block(section).unwrap();
        assert_eq!(projected, vector["stripped"]);
        let result = format(
            "Canvas title",
            Some(section),
            SectionVariant::Section,
            false,
            &mut |leaf| {
                if leaf["type"] == "canvas" {
                    assert!(leaf.get("section_id").is_none());
                    assert_eq!(leaf["file_id"], "FNESTED01");
                    return Ok("N".into());
                }
                section_leaf_label(
                    leaf,
                    Some(EmojiText::EnglishCanonicalNames),
                    |id| {
                        assert_eq!(id, "USECTION01");
                        Ok("@U".into())
                    },
                    |id| {
                        super::super::known_ordinary_channel_label(
                            id,
                            &vector["state"]["channels"][id],
                        )
                    },
                    |id| {
                        let group = crate::slack::models::UserGroup::from_observed(
                            &vector["state"]["userGroups"][id],
                        )
                        .unwrap();
                        super::super::known_group_label(id, &group)
                    },
                )
            },
        )
        .unwrap();
        assert_eq!(
            result.units(),
            units(&vector["labelUtf16"]),
            "{}",
            vector["name"]
        );
        assert_eq!(*section, unchanged);
        let dependencies =
            section_dependencies(section, Some(EmojiText::EnglishCanonicalNames)).unwrap();
        if vector["name"] == "mixed-known" {
            assert_eq!(
                dependencies.users.into_iter().collect::<Vec<_>>(),
                ["USECTION01"]
            );
            assert_eq!(
                dependencies.channels.into_iter().collect::<Vec<_>>(),
                ["CSECTION01"]
            );
            assert_eq!(
                dependencies.groups.into_iter().collect::<Vec<_>>(),
                ["SSECTION01"]
            );
            assert!(section_dependencies(section, None).is_err());
        } else {
            assert!(
                dependencies.users.is_empty()
                    && dependencies.channels.is_empty()
                    && dependencies.groups.is_empty()
            );
        }
    }
}
