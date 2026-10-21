//! Explicit synthetic HTTP roots, not inferred server echoes of annotation snippets.
use super::*;

pub(super) fn rich() -> Value {
    let mut row = root("1719800000.000100", "DECOY text must not render");
    row["markdown_text"] = json!("DECOY markdown must not render");
    row["document_comment"]["snippet"] = json!({"rich_text":"DECOY snippet must not render"});
    row["blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Rich & \u{4e2d}\u{1f98a}","style":{"bold":true,"underline":true}},
        {"type":"text","text":" for "},
        {"type":"user","user_id":"U1"}
    ]}]}]);
    row
}

pub(super) fn fallback() -> Value {
    let mut row = root(
        "1719800020.000100",
        "Fallback *bold* &amp; \u{65e5}\u{1f600} _New &\u{1f98a} _",
    );
    row["blocks"] = json!([]);
    row["markdown_text"] = json!("DECOY secondary fallback must not render");
    row
}

pub(super) fn markdown() -> Value {
    let mut row = root("1719800030.000100", "");
    row["markdown_text"] = json!("An older thread with _italic_ &amp; \u{4e2d}\u{1f98a}.");
    row
}

pub(super) fn literal() -> Value {
    let mut row = root(
        "1719700000.000100",
        "Resolved: *literal* _marks_ <@U1> :wave: &amp; &lt;@U1&gt;",
    );
    row["mrkdwn"] = json!(false);
    row
}

pub(super) fn hidden() -> Value {
    let mut row = root("1719700020.000100", "DECOY hidden text must not render");
    row["markdown_text"] = json!("DECOY hidden markdown must not render");
    row["blocks"] = json!([{"type":"section","text":{"type":"plain_text","text":"DECOY hidden blocks must not render"}}]);
    row["document_comment"]["is_visible"] = json!(false);
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_root_carriers_keep_explicit_body_selection_inputs_and_identity() {
        let rich = rich();
        assert_eq!(rich["subtype"], "document_comment_root");
        assert_eq!(
            rich["document_comment"]["thread_id"],
            "quip-1719800000.000100"
        );
        assert_eq!(
            rich["blocks"][0]["elements"][0]["elements"][0]["style"],
            json!({"bold":true,"underline":true})
        );
        assert_eq!(fallback()["blocks"], json!([]));
        assert_eq!(markdown()["text"], "");
        assert_eq!(literal()["mrkdwn"], false);
        assert_eq!(hidden()["document_comment"]["is_visible"], false);
    }
}
