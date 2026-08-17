use super::super::*;
use crate::slack::models::{Attachment, File};

#[test]
fn file_summary_prefers_title_type_and_size() {
    let file = File {
        id: Some("F1".into()),
        name: Some("report.pdf".into()),
        title: Some("Quarterly report".into()),
        pretty_type: Some("PDF".into()),
        size: Some(1_572_864),
        ..Default::default()
    };

    assert_eq!(file_title(&file), "Quarterly report");
    assert_eq!(file_summary(&file), "PDF - 1.5 MB");
    assert_eq!(format_file_size(512), "512 B");
    assert_eq!(format_file_size(2048), "2.0 KB");
}

#[test]
fn file_download_name_sanitizes_paths() {
    let file = File {
        name: Some("../bad/name?.png".into()),
        title: Some("ignored".into()),
        ..Default::default()
    };
    assert_eq!(file_download_name(&file), "_bad_name_.png");

    let fallback = File::default();
    assert_eq!(file_download_name(&fallback), "download");
}

#[test]
fn file_preview_uses_largest_known_thumb_and_stable_key() {
    let mut file = File {
        id: Some("F123".into()),
        thumb_64: Some("https://files/thumb-64.png".into()),
        thumb_160: Some("https://files/thumb-160.png".into()),
        thumb_360: Some("https://files/thumb-360.png".into()),
        ..Default::default()
    };
    file.extra.insert(
        "thumb_1024".into(),
        serde_json::json!("https://files/thumb-1024.png"),
    );

    assert_eq!(file_preview_key(&file).as_deref(), Some("F123"));
    assert_eq!(
        file_preview_url(&file),
        Some("https://files/thumb-1024.png")
    );

    let without_id = File {
        thumb_80: Some("https://files/thumb-80.png".into()),
        ..Default::default()
    };
    assert_eq!(
        file_preview_key(&without_id).as_deref(),
        Some("https://files/thumb-80.png")
    );
}

#[test]
fn heic_uses_slack_raster_preview_while_video_uses_original() {
    let mut heic = File {
        name: Some("camera.heic".into()),
        mimetype: Some("image/heic".into()),
        filetype: Some("heic".into()),
        url_private: Some("https://files.slack.com/camera.heic".into()),
        thumb_360: Some("https://files.slack.com/camera-360.jpg".into()),
        ..Default::default()
    };
    heic.extra.insert(
        "thumb_1024".into(),
        serde_json::json!("https://files.slack.com/camera-1024.jpg"),
    );
    assert!(is_image_file(&heic));
    assert!(!file_original_is_viewer_decodable(&heic));
    assert_eq!(
        file_viewer_url(&heic),
        Some("https://files.slack.com/camera-1024.jpg")
    );

    let video = File {
        name: Some("demo.mp4".into()),
        mimetype: Some("video/mp4".into()),
        url_private: Some("https://files.slack.com/files-tmb/demo.mp4".into()),
        extra: BTreeMap::from([
            (
                "mp4".into(),
                serde_json::json!("https://files.slack.com/files-tmb/demo.mp4"),
            ),
            (
                "thumb_video".into(),
                serde_json::json!("https://files.slack.com/files-tmb/demo.jpeg"),
            ),
            (
                "url_private_download".into(),
                serde_json::json!("https://files.slack.com/files-pri/download/demo.mp4"),
            ),
        ]),
        ..Default::default()
    };
    assert!(is_video_file(&video));
    assert_eq!(
        file_viewer_url(&video),
        Some("https://files.slack.com/files-tmb/demo.mp4")
    );
    assert_eq!(
        file_preview_url(&video),
        Some("https://files.slack.com/files-tmb/demo.jpeg")
    );
    assert_eq!(
        file_download_url(&video),
        Some("https://files.slack.com/files-pri/download/demo.mp4")
    );
}
#[test]
fn is_browser_url_accepts_only_http_and_https() {
    assert!(is_browser_url("https://slack.com/archives/C1/p1"));
    assert!(is_browser_url("http://example.com"));
    assert!(!is_browser_url("file:///etc/passwd"));
    assert!(!is_browser_url("javascript:alert(1)"));
    assert!(!is_browser_url(""));
    assert!(!is_browser_url("ftp://example.com"));
}

#[test]
fn authenticated_url_check_rejects_lookalike_hosts() {
    assert!(is_slack_authenticated_url(
        "https://files.slack.com/files-pri/T/F/image.png"
    ));
    assert!(is_slack_authenticated_url(
        "https://workspace.slack.com/files/image.png"
    ));
    assert!(!is_slack_authenticated_url(
        "https://files.slack.com.evil.example/image.png"
    ));
    assert!(!is_slack_authenticated_url(
        "https://slack.com@evil.example/image.png"
    ));
}

#[test]
fn attachment_viewer_prefers_original_and_names_download() {
    let attachment = Attachment {
        title: Some("Launch / board.png".into()),
        image_url: Some("https://cdn.example.com/original/board.png?size=large".into()),
        thumb_url: Some("https://cdn.example.com/thumb/board.png".into()),
        ..Default::default()
    };
    assert_eq!(
        attachment_viewer_url(&attachment),
        Some("https://cdn.example.com/original/board.png?size=large")
    );
    assert_eq!(attachment_download_name(&attachment), "Launch _ board.png");
}

#[test]
fn gif_picker_attachment_exposes_nested_image_block() {
    let message: SlackMessage = serde_json::from_value(serde_json::json!({
        "type": "message",
        "user": "U08TCSANHDX",
        "text": "",
        "ts": "1785200205.163019",
        "files": [],
        "blocks": [],
        "attachments": [{
            "id": 1,
            "fallback": "shared a GIF",
            "blocks": [{
                "type": "image",
                "image_url": "https://media0.giphy.com/media/OIKS4GcqcKqNtndh1o/200w.gif?rid=200w.gif",
                "image_width": 200,
                "image_height": 206,
                "image_bytes": 17943,
                "is_animated": true,
                "alt_text": "Main Character Instagram GIF"
            }]
        }]
    }))
    .expect("decode picker message");

    let images: Vec<_> = attachment_images(&message.attachments[0]).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].width, Some(200));
    assert_eq!(images[0].height, Some(206));
    assert!(images[0].animated);
    assert_eq!(images[0].alt_text, Some("Main Character Instagram GIF"));
    assert!(images[0].preview_url.ends_with("rid=200w.gif"));
    assert_eq!(message_text(&message), "");
}

#[test]
fn image_file_detection_and_uploader_are_defensive() {
    let image = File {
        name: Some("launch.PNG".into()),
        extra: BTreeMap::from([("user".into(), serde_json::json!("U_ALICE"))]),
        ..Default::default()
    };
    assert!(is_image_file(&image));
    assert_eq!(file_uploader_id(&image), Some("U_ALICE"));
    assert!(!is_image_file(&File {
        name: Some("brief.pdf".into()),
        mimetype: Some("application/pdf".into()),
        ..Default::default()
    }));
}

#[test]
fn attachment_download_name_falls_back_to_url_then_image() {
    let from_url = Attachment {
        image_url: Some("https://cdn.example.com/path/design.jpg?width=1200".into()),
        ..Default::default()
    };
    assert_eq!(attachment_download_name(&from_url), "design.jpg");
    assert_eq!(attachment_download_name(&Attachment::default()), "image");
}
