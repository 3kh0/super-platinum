//! Disconnected image fixture. Its HTTPS identity is synthetic and preloaded;
//! the original native img still performs the actual export on explicit choice.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
const PREFIX: &str = "https://image-fixture.invalid";
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-image" | "canvas-comment-image-narrow")
        )
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn text(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn num(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn message(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Image fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn url(snapshot: &CanvasCollabSnapshot) -> String {
    format!(
        "{PREFIX}/-/collab-slack-blob/{}/FIMAGE",
        snapshot.thread_id()
    )
}
pub(super) fn seed(mut snapshot: CanvasCollabSnapshot) -> Record {
    let i = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "paragraph")
        .unwrap();
    let mut image = snapshot.sections[i].raw().remove(38).unwrap();
    for field in [
        num(9, 2),
        num(10, 11),
        message(
            12,
            &raw(&[message(
                2,
                &raw(&[
                    message(3, &raw(&[text(5, "FIMAGE"), num(2, 512), num(3, 512)])),
                    text(7, "Illustration.png"),
                    text(15, "Local image annotation fixture"),
                    text(99, "unrelated metadata remains byte exact"),
                ]),
            )]),
        ),
    ] {
        image = image
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    snapshot.sections[i] = Section::from_raw(image).unwrap();
    let options = snapshot
        .init
        .options()
        .replace_fields(8, &[message(8, &raw(&[text(2, PREFIX)]))])
        .unwrap();
    snapshot.init=api::CanvasControllerInit::parse(json!({"ok":true,"quip_user_id":snapshot.init.quip_user_id(),"init_options":STANDARD.encode(options.raw())}),&workspace()).unwrap();
    section::seed(snapshot)
}
pub(super) fn preload(shell: &ShellState, snapshot: &CanvasCollabSnapshot) {
    let id = shell.media.register_image(
        super_platinum_core::MediaAssetKind::Attachment,
        &url(snapshot),
        "image/png",
    );
    shell.media.insert(
        id,
        "image/png",
        include_bytes!("../../../../../../assets/icons/icon-512.png").as_slice(),
    );
}
pub(super) fn document(
    snapshot: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    if snapshot.section("paragraph")?.section_type().ok().flatten() != Some(2) {
        return None;
    }
    // Every interpolated identity is a closed synthetic fixture value.
    Some(super_platinum_core::canvas::CanvasDocument::parse(&format!(
        "<div class='quip-canvas-content'><h1>Image annotation verification</h1><div id='paragraph'><img src='{}' alt='Local image annotation fixture'></div><p id='neighbor'>Another paragraph</p></div>",url(snapshot))).unwrap())
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        canvas::edit::annotation::{
            AnnotationIntent, AnnotationPresentation, ImageExport, SectionAnnotationChoice,
            image_seed,
        },
        slack::client::SlackClient,
    };
    #[test]
    fn image_fixture_uses_real_anchor_batch_and_separate_qpc_without_changing_image_metadata() {
        for outcome in [
            ImageExport::Exported,
            ImageExport::NoImage,
            ImageExport::EmptyData,
            ImageExport::ExportError,
        ] {
            let mut record = seed(initial());
            let snapshot = record.canonical.clone().unwrap();
            assert!(document(&snapshot).is_some());
            assert!(
                image_seed(&snapshot, "paragraph")
                    .unwrap()
                    .matches_rendered_url(&url(&snapshot))
            );
            let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_image(
                &snapshot,
                "paragraph",
                &AnnotationPresentation::ordinary(None, None).unwrap(),
                outcome,
            )
            .unwrap() else {
                panic!("new")
            };
            let p = intent.prepare(&snapshot).unwrap();
            let original = snapshot.section("paragraph").unwrap().clone();
            let request = wire(
                api::canvas_annotation(&SlackClient::default(), &workspace(), &snapshot, &p)
                    .unwrap(),
            )
            .unwrap();
            let accepted = complete_anchor(&mut record, &snapshot, p, request).unwrap();
            let canonical = record.canonical.as_ref().unwrap();
            assert_eq!(canonical.section("paragraph"), Some(&original));
            let proof = accepted.confirm(canonical).unwrap();
            let bootstrap = BootstrapIntent::capture(proof, 1_720_000_000_003).unwrap();
            let request = wire(
                api::canvas_comment_bootstrap(&SlackClient::default(), &workspace(), &bootstrap)
                    .unwrap(),
            )
            .unwrap();
            assert!(request.to_string().contains("Qpc:"));
            let accepted = complete_bootstrap(&mut record, bootstrap, request).unwrap();
            assert_eq!(
                accepted.record().annotation().snippet()["fileIds"],
                json!(["FIMAGE"])
            );
            assert_eq!(record.prepared.len(), 2);
        }
    }
}
