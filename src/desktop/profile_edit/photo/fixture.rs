//! Synthetic, disconnected layout states. No account/session or network fallback.
use super::*;
use serde_json::json;
pub(crate) fn install(shell: &mut ShellState) {
    super::super::fixture::install(shell);
    let draft = shell.profile_edit.draft.as_ref().unwrap();
    let token = draft.token.clone();
    let visit = draft.visit;
    let photo=protocol::Photo::from_profile(&json!({"avatar_hash":"synthetic-old","image_48":"https://fixture.slack.com/old.png","image_original":"https://fixture.slack.com/old.png"})).unwrap();
    photo.adopt(
        shell
            .core
            .workspaces
            .get_mut(&token.owner.team)
            .unwrap()
            .users
            .get_mut(&token.owner.user)
            .unwrap()
            .profile
            .as_mut()
            .unwrap(),
    );
    let cap = protocol::Capability {
        team: token.owner.team.clone(),
        user: token.owner.user.clone(),
        permission: api::ProfileEditCapability::Allowed,
        photo: photo.clone(),
    };
    shell.profile_edit.photo.draft = Some(PhotoDraft {
        token,
        visit,
        serial: next_id(),
        cap: Some(cap),
        observed: Some(photo),
        preview: None,
        remove: false,
        loading: false,
        message: Some("Choose a square crop. This preview uses a synthetic local image.".into()),
    });
    let image = protocol::Image::parse(include_bytes!("fixture.png").to_vec()).unwrap();
    let prepared = protocol::PreparedPhoto::parse(
        &json!({"ok":true,"id":"12345","url":"https://fixture.slack.com/prepared.png"}),
    )
    .unwrap();
    network::install_preview(shell, image, Some(prepared));
    shell
        .profile_edit
        .photo
        .draft
        .as_mut()
        .unwrap()
        .preview
        .as_mut()
        .unwrap()
        .decoded = true;
}
