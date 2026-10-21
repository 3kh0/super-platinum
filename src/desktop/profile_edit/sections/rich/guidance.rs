use super::{Value, collect};

/// Describes the kinds of original references, not which async source failed.
/// A mixed opening can finish file acquisition before it waits for mentions.
/// The view applies observed group rate-limit guidance before calling this.
pub(crate) fn reference_guidance(blocks: &[Value], recovering: bool) -> &'static str {
    let (files, mentions) = collect(blocks).map_or((false, false), |tokens| {
        (
            tokens
                .iter()
                .any(|raw| matches!(raw["type"].as_str(), Some("file" | "canvas"))),
            tokens
                .iter()
                .any(|raw| matches!(raw["type"].as_str(), Some("user" | "usergroup" | "channel"))),
        )
    });
    match (files, mentions, recovering) {
        (true, true, true) => "Loading reference details. Your description is unchanged.",
        (true, true, false) => {
            "Reference details are unavailable or changed while opening. Your description is unchanged. Retry to load document and mention details."
        }
        (true, false, true) => "Loading document titles. Your description is unchanged.",
        (true, false, false) => {
            "Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details."
        }
        (false, _, true) => "Loading mention names. Your description is unchanged.",
        (false, _, false) => {
            "Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocks(tokens: Vec<Value>) -> Vec<Value> {
        vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":tokens}
        ]})]
    }
    #[test]
    fn every_supported_mixed_pair_uses_reference_guidance_without_mutating_tokens() {
        for file in ["file", "canvas"] {
            for mention in ["user", "usergroup", "channel"] {
                let raw = blocks(vec![
                    json!({"type":file,"file_id":"FFILE0001","url":"original",
                        "style":{"bold":true},"opaque":[0,false,null]}),
                    json!({"type":mention,"user_id":"UMEMBER01","usergroup_id":"SGROUP001",
                        "channel_id":"CCHANNEL1","opaque":{"keep":true}}),
                ]);
                let original = raw.clone();
                assert_eq!(
                    reference_guidance(&raw, true),
                    "Loading reference details. Your description is unchanged."
                );
                assert_eq!(
                    reference_guidance(&raw, false),
                    "Reference details are unavailable or changed while opening. Your description is unchanged. Retry to load document and mention details."
                );
                assert_eq!(raw, original);
            }
        }
    }

    #[test]
    fn single_kind_guidance_is_unchanged_in_both_states() {
        for kind in ["file", "canvas"] {
            let raw = blocks(vec![json!({"type":kind,"file_id":"FFILE0001"})]);
            assert_eq!(
                reference_guidance(&raw, true),
                "Loading document titles. Your description is unchanged."
            );
            assert_eq!(
                reference_guidance(&raw, false),
                "Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details."
            );
        }
        for kind in ["user", "usergroup", "channel"] {
            let raw = blocks(vec![json!({"type":kind})]);
            assert_eq!(
                reference_guidance(&raw, true),
                "Loading mention names. Your description is unchanged."
            );
            assert_eq!(
                reference_guidance(&raw, false),
                "Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details."
            );
        }
    }

    #[test]
    fn nested_original_references_count_but_text_links_and_opaque_fields_do_not() {
        let nested = vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_quote","elements":[{"type":"file","file_id":"FFILE0001"}]},
            {"type":"rich_text_list","elements":[{"type":"rich_text_section","elements":[
                {"type":"usergroup","usergroup_id":"SGROUP001"}
            ]}]}
        ]})];
        assert_eq!(
            reference_guidance(&nested, true),
            "Loading reference details. Your description is unchanged."
        );
        let not_mixed = blocks(vec![
            json!({"type":"file","file_id":"FFILE0001","opaque":{"type":"usergroup"}}),
            json!({"type":"text","text":"@person #channel"}),
            json!({"type":"link","url":"https://example.test","text":"@mention"}),
        ]);
        assert_eq!(
            reference_guidance(&not_mixed, true),
            "Loading document titles. Your description is unchanged."
        );
    }

    #[test]
    fn invalid_or_empty_structure_preserves_existing_fallback_without_claiming_admission() {
        for raw in [vec![], vec![json!({"type":"rich_text","elements":null})]] {
            assert_eq!(
                reference_guidance(&raw, true),
                "Loading mention names. Your description is unchanged."
            );
        }
    }
}
