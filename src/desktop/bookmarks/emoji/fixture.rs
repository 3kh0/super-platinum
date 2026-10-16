use crate::state::ShellState;

pub(crate) fn apply(shell: &mut ShellState) {
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().apply_emojis(vec![
        serde_json::from_value(serde_json::json!({"name":"bookmark-ready","value":"https://example.test/bookmark-ready.png"})).unwrap(),
        serde_json::from_value(serde_json::json!({"name":"bookmark-pending","value":"https://example.test/bookmark-pending.png"})).unwrap(),
    ]);
    shell.core.bookmarks.items = serde_json::from_value(serde_json::json!([
        {"id":"BkEmojiStandard","title":"Launch guide","link":"https://example.com/launch","type":"link","emoji":":rocket:","rank":"a"},
        {"id":"BkEmojiCustom","title":"Team handbook","link":"https://example.com/team","type":"link","emoji":":bookmark-ready:","rank":"b"},
        {"id":"BkEmojiPending","title":"Pending custom image","link":"https://example.com/pending","type":"link","emoji":":bookmark-pending:","rank":"c"},
        {"id":"BkEmojiUnknown","title":"Unknown emoji uses bookmark","link":"https://example.com/unknown","type":"link","emoji":":not-a-known-bookmark-emoji:","rank":"d"},
        {"id":"BkEmojiAbsent","title":"Link without emoji","link":"https://example.com/plain","type":"link","rank":"e"},
        {"id":"BkEmojiFolder","title":"References","type":"local_folder","emoji":":rocket:","rank":"f"},
        {"id":"BkEmojiChild","title":"Nested checklist","link":"https://example.com/checklist","type":"link","parent_id":"BkEmojiFolder","rank":"g"}
    ])).unwrap();
    let id = super::custom_id(
        shell,
        &team,
        "bookmark-ready",
        "https://example.test/bookmark-ready.png",
    );
    shell.media.insert(
        id,
        "image/png",
        include_bytes!("../../../../assets/icons/icon-512.png").as_slice(),
    );
}
