use super::*;
use crate::block_actions::directory_endpoint::{DirectoryEndpoint, channel_info::Read};
use crate::slack::SlackClient;
fn request() -> PreparedRequest {
    let w = crate::config::WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let now = std::time::Instant::now();
    let endpoint=DirectoryEndpoint::decode(&w,&serde_json::json!({"ok":true,"url":"https://edgeapi.slack.com/cache/T00000001","ttl_seconds":60}),now).unwrap();
    endpoint
        .channel_info(
            &SlackClient::default(),
            &w,
            now,
            &Read::capture(vec!["C00000001".into()], None).unwrap(),
        )
        .unwrap()
}
#[test]
fn transport_admission_retains_closed_channel_info_shape() {
    let mut req = request();
    assert!(allowed(&req));
    req.url = req.url.replace("channels/info", "users/info");
    assert!(!allowed(&req));
    let mut req = request();
    let RequestBody::Json(ref mut b) = req.body else {
        panic!("json")
    };
    b["check_membership"] = serde_json::json!(false);
    assert!(!allowed(&req));
    let mut req = request();
    req.method = "GET".into();
    assert!(!allowed(&req));
}
