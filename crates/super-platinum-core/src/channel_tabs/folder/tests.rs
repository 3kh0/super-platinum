use super::*;
use serde_json::json;
fn base() -> Inventory {
    Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":[
        {"id":"Ct00000001","type":"folder","data":{"folder_bookmark_id":"Bk1"}},
        {"id":"Ct00000002","type":"folder"},
        {"id":"Ct00000003","type":"future","opaque":[3,1,2]}
    ]}}))
    .unwrap()
}
fn response() -> Value {
    json!({"ok":true,"bookmarks":[
        {"id":"Bk1","type":"local_folder","title":"Resources"},
        {"id":"Bk2","type":"link","parent_id":"Bk1","title":"Guide","link":"https://example.com","rank":"b","opaque":{"keep":true}},
        {"id":"Bk3","type":"file","parent_id":"Bk1","title":"Plan","entity_id":"F00000001","rank":"a"},
        {"id":"Bk4","type":"future","parent_id":"Bk1","title":"Future","rank":"c"},
        {"id":"Bk5","type":"local_folder","parent_id":"Bk1"},
        {"id":"Bk6","type":"link","parent_id":"Bk5","link":"https://nested.invalid"},
        {"id":"Bk7","type":"link","link":"https://top.invalid"}
    ]})
}
#[test]
fn direct_children_preserve_order_unknown_rows_and_full_raw_inventory() {
    let base = base();
    let f = Folder::capture(&base, "Ct00000001").unwrap();
    let raw = response();
    let c = Contents::parse(&base, &f, &raw).unwrap();
    assert_eq!(c.raw(), raw["bookmarks"].as_array().unwrap());
    assert_eq!(
        c.items()
            .iter()
            .map(|i| i.bookmark.id.as_str())
            .collect::<Vec<_>>(),
        ["Bk3", "Bk2", "Bk4"]
    );
    assert_eq!(c.items()[0].open, Open::File("F00000001".into()));
    assert_eq!(c.items()[1].raw["opaque"], json!({"keep":true}));
    assert_eq!(c.items()[2].open, Open::Unavailable);
}
#[test]
fn legacy_tab_projects_top_level_without_nested_folders() {
    let base = base();
    let f = Folder::capture(&base, "Ct00000002").unwrap();
    let c = Contents::parse(&base, &f, &response()).unwrap();
    assert_eq!(
        c.items()
            .iter()
            .map(|i| i.bookmark.id.as_str())
            .collect::<Vec<_>>(),
        ["Bk7"]
    );
}
#[test]
fn membership_rebinding_unknown_kind_and_malformed_folder_refuse() {
    let base = base();
    let f = Folder::capture(&base, "Ct00000001").unwrap();
    for row in [
        json!({"id":"Ct00000001","type":"folder","data":{"folder_bookmark_id":"Bk9"}}),
        json!({"id":"Ct00000001","type":"future"}),
    ] {
        let changed =
            Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":[row]}}))
                .unwrap();
        assert!(!f.current(&changed));
        assert!(Contents::parse(&changed, &f, &response()).is_err());
    }
    assert!(Folder::capture(&base, "Ct00000003").is_err());
    let bad=Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":[{"id":"Ct00000001","type":"folder","data":{"folder_bookmark_id":42}}]}})).unwrap();
    assert!(Folder::capture(&bad, "Ct00000001").is_err());
}
#[test]
fn foreign_rows_duplicates_error_envelopes_and_bad_file_do_not_gain_authority() {
    let base = base();
    let f = Folder::capture(&base, "Ct00000001").unwrap();
    let mut raw = response();
    raw["bookmarks"][1]["channel_id"] = "C00000009".into();
    assert!(Contents::parse(&base, &f, &raw).is_err());
    let mut raw = response();
    raw["bookmarks"][1]["id"] = "Bk1".into();
    assert!(Contents::parse(&base, &f, &raw).is_err());
    let mut raw = response();
    raw["error"] = "partial".into();
    assert!(Contents::parse(&base, &f, &raw).is_err());
    let mut raw = response();
    raw["bookmarks"][2]["entity_id"] = "https://evil.invalid".into();
    assert_eq!(
        Contents::parse(&base, &f, &raw).unwrap().items()[0].open,
        Open::Unavailable
    );
}
