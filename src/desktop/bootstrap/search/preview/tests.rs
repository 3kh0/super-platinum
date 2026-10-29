use super::*;
use crate::{media::MediaRegistry, model::AttachmentVm, state::ShellState};

#[test]
fn blank_highlights_use_visible_image_fallback_but_retained_metadata_is_not_body_content() {
    let media = MediaRegistry::default();
    let image = media.register_image(
        super_platinum_core::MediaAssetKind::Attachment,
        "https://example.test/hidden-image.png",
        "image/png",
    );
    let mut message = MessageVm {
        body: vec![RichNode::Text(" \u{e000}\u{e001} ".into())],
        attachments: vec![AttachmentVm {
            image: Some((image.clone(), None)),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(text(&message), "Image attachment");
    message.attachments[0].image = None;
    message.attachments[0].service = Some("Service metadata".into());
    assert_eq!(text(&message), "");
    message.body = vec![RichNode::Text(
        "  Existing body\n\u{e000}content\u{e001}".into(),
    )];
    assert_eq!(text(&message), "  Existing body\n\u{e000}content\u{e001}");
    message.body.clear();
    message.attachments = vec![
        AttachmentVm {
            title: Some("Report".into()),
            image: Some((image.clone(), None)),
            ..Default::default()
        },
        AttachmentVm {
            thumb: Some(image),
            ..Default::default()
        },
        AttachmentVm {
            fields: vec![("Status".into(), vec![RichNode::Text("Ready".into())], false)],
            ..Default::default()
        },
    ];
    assert_eq!(text(&message), "Report\nImage attachment\nStatus: Ready");
}

#[test]
fn normal_and_narrow_attachment_search_fixtures_prepare_readable_existing_rows() {
    for name in ["search-attachments", "search-attachments-narrow"] {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        crate::bootstrap::search_fixture(&mut shell, name);
        assert_eq!(shell.overlay, Some(crate::state::Overlay::Search));
        assert_eq!(shell.search_results.len(), 3);
        assert_eq!(
            shell.search_results[0].text,
            "Release notice\nDesktop release report\nThe release is ready for review.\nChecks: All required checks passed"
        );
        assert!(shell.search_results[1].text.contains("Release notes"));
        assert_eq!(shell.search_results[2].text, "Image attachment");
        assert_eq!((shell.search_pager.page, shell.search_pager.total), (1, 3));
        assert!(!shell.search_pager.loading && !shell.search_pager.has_more());
    }
}
