//! Static disconnected layouts only; this seed cannot call a live refresh.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let file = File {
        id: Some("F_REFRESH".into()),
        name: Some("release-notes.txt".into()),
        title: Some("Release notes".into()),
        mimetype: Some("text/plain".into()),
        filetype: Some("text".into()),
        is_external: Some(false),
        url_private: Some("https://files.slack.com/refresh-fixture".into()),
        extra: std::collections::BTreeMap::from([("mode".into(), serde_json::json!("hosted"))]),
        ..Default::default()
    };
    shell.core.session = None;
    shell.core.transport = None;
    let id = shell.media.register_deferred(
        super_platinum_core::MediaAssetKind::Attachment,
        file.url_private.as_deref().unwrap(),
        "text/plain",
    );
    shell.media.register_file_download(&id, &file);
    shell.media.insert(id.clone(),"text/plain",b"Release notes\nKeep this text selected while refreshing.\nThe open contents stay readable if refresh fails.".as_slice());
    shell.viewer = Some(ViewerVm {
        file: file_actions::identity(&file),
        id,
        name: "Release notes".into(),
        download_name: "release-notes.txt".into(),
        mime: "text/plain".into(),
    });
    shell.overlay = Some(Overlay::Viewer);
    opened(shell);
    if let Some(a) = shell.file_refresh.active.as_mut() {
        a.notice = if name.contains("error") {
            "Could not refresh contents. The open preview is unchanged. Try again."
        } else {
            "This file may have changed. Refresh to fetch its contents again."
        };
    }
}
