//! Actual ordinary table children; same disconnected worker and original img.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
const PREFIX: &str = "https://table-image-fixture.invalid";
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-table-bodies" | "canvas-comment-table-bodies-narrow")
        )
}
fn raw(f: &[F]) -> RawMessage {
    RawMessage::from_fields(f).unwrap()
}
fn t(n: u32, v: &str) -> F {
    F::bytes(n, v.as_bytes()).unwrap()
}
fn n(k: u32, v: u64) -> F {
    F::uint(k, v).unwrap()
}
fn m(k: u32, v: &RawMessage) -> F {
    F::message(k, v).unwrap()
}
fn changed(s: &Section, f: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Table bodies".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn url(s: &CanvasCollabSnapshot) -> String {
    format!("{PREFIX}/-/collab-slack-blob/{}/FIMAGE", s.thread_id())
}
pub(super) fn seed(snapshot: CanvasCollabSnapshot) -> Record {
    let mut record = table::seed(snapshot);
    let mut s = record.canonical.take().unwrap();
    let at = s
        .sections
        .iter()
        .position(|s| s.id() == "table-second")
        .unwrap();
    let original = s.sections[at].clone();
    for f in [n(9, 1), n(10, 7), m(12, &raw(&[]))] {
        s.sections[at] = changed(&s.sections[at], f);
    }
    for (id, path, text, checked, deleted) in [
        (
            "table-list-one",
            "zzd",
            "First checklist item",
            false,
            false,
        ),
        (
            "table-list-two",
            "zze",
            "<b>Bold checked item</b>",
            true,
            false,
        ),
        (
            "table-list-deleted",
            "zzf",
            "Deleted retained item",
            false,
            true,
        ),
    ] {
        let mut child = Section::from_raw(original.raw().remove(29).unwrap()).unwrap();
        for f in [
            t(1, id),
            t(8, path),
            t(21, &format!("zza-{path}")),
            n(3, u64::from(deleted)),
            m(13, &raw(&[t(1, "table-second"), n(4, 7)])),
            m(16, &raw(&[n(2, u64::from(checked))])),
            m(12, &raw(&[m(1, &raw(&[t(1, text)]))])),
        ] {
            child = changed(&child, f);
        }
        s.sections.push(child);
    }
    let mut image = original;
    for f in [
        t(1, "table-image"),
        t(8, "zzc"),
        t(21, "zzc"),
        n(9, 2),
        n(10, 11),
        m(
            12,
            &raw(&[m(
                2,
                &raw(&[
                    t(15, "Table image"),
                    m(3, &raw(&[t(5, "FIMAGE"), n(2, 512), n(3, 512)])),
                ]),
            )]),
        ),
    ] {
        image = changed(&image, f);
    }
    s.sections.push(image);
    let at = s
        .sections
        .iter()
        .position(|s| s.id() == "modern-table")
        .unwrap();
    let content = s.sections[at].raw().message(12).unwrap().unwrap();
    let layout = content.message(30).unwrap().unwrap();
    let cells = layout
        .messages(3)
        .unwrap()
        .into_iter()
        .map(|cell| {
            if cell.string(1).unwrap() == Some("table-cell2") {
                let mut ids = cell
                    .fields()
                    .iter()
                    .filter(|f| f.tag() == 4)
                    .map(|f| {
                        F::bytes(
                            4,
                            cell.encoded_field(f)
                                .and_then(RawMessage::decode)
                                .unwrap()
                                .bytes(4)
                                .unwrap()
                                .unwrap(),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                ids.push(t(4, "table-image"));
                cell.replace_fields(4, &ids).unwrap()
            } else {
                cell
            }
        })
        .map(|cell| m(3, &cell))
        .collect::<Vec<_>>();
    s.sections[at] = changed(
        &s.sections[at],
        m(
            12,
            &content
                .replace_fields(30, &[m(30, &layout.replace_fields(3, &cells).unwrap())])
                .unwrap(),
        ),
    );
    let options = s
        .init
        .options()
        .replace_fields(8, &[m(8, &raw(&[t(2, PREFIX)]))])
        .unwrap();
    s.init=api::CanvasControllerInit::parse(json!({"ok":true,"quip_user_id":s.init.quip_user_id(),"init_options":STANDARD.encode(options.raw())}),&workspace()).unwrap();
    record.canonical = Some(s);
    record
}
pub(super) fn preload(shell: &ShellState, s: &CanvasCollabSnapshot) {
    let id = shell.media.register_image(
        super_platinum_core::MediaAssetKind::Attachment,
        &url(s),
        "image/png",
    );
    shell.media.insert(
        id,
        "image/png",
        include_bytes!("../../../../../../assets/icons/icon-512.png").as_slice(),
    );
}
pub(super) fn document(
    s: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    s.section("table-image")?;
    let paragraph = s
        .section("paragraph")?
        .raw()
        .message(12)
        .ok()??
        .message(1)
        .ok()??;
    let mut html = format!(
        "<div class='quip-canvas-content'><h1>Table body comments</h1><table id='modern-table'><tbody><tr><td><p id='paragraph'>{}</p></td><td><div data-section-style='7'><ul id='table-second'>",
        paragraph.string(1).ok()??
    );
    for id in ["table-list-one", "table-list-two"] {
        let child = s.section(id)?;
        let content = child.raw().message(12).ok()??.message(1).ok()??;
        let class = if child.raw().message(16).ok()??.boolean(2).ok()? == Some(true) {
            " class='checked'"
        } else {
            ""
        };
        html.push_str(&format!(
            "<li id='{id}'{class}>{}</li>",
            content.string(1).ok()??
        ));
    }
    html.push_str(&format!("</ul></div><div id='table-image'><img src='{}' alt='Table image'></div></td></tr></tbody></table></div>",url(s)));
    Some(super_platinum_core::canvas::CanvasDocument::parse(&html).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        canvas::edit::annotation::{
            AnnotationIntent, AnnotationPresentation, ImageExport, SectionAnnotationChoice,
            image_seed, section_previews,
        },
        slack::client::SlackClient,
    };
    #[test]
    fn actual_table_body_fixture_builds_real_list_image_requests_and_exact_canonical_receipts() {
        for image in [false, true] {
            let mut r = seed(initial());
            let s = r.canonical.clone().unwrap();
            let before = s.sections.clone();
            assert!(document(&s).is_some());
            let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
            let previews = section_previews(&s, &presentation).unwrap();
            assert!(previews.iter().any(|p| p.id == "table-second"));
            assert!(!previews.iter().any(|p| p.id == "table-list-deleted"));
            let choice = if image {
                assert!(
                    image_seed(&s, "table-image")
                        .unwrap()
                        .matches_rendered_url(&url(&s))
                );
                AnnotationIntent::for_image(&s, "table-image", &presentation, ImageExport::Exported)
            } else {
                AnnotationIntent::for_section(&s, "table-second", &presentation)
            };
            let SectionAnnotationChoice::New(intent) = choice.unwrap() else {
                panic!("new")
            };
            let p = intent.prepare(&s).unwrap();
            assert_eq!(
                serde_json::to_value(p.record().unwrap()).unwrap()["version"],
                11
            );
            let request = wire(
                api::canvas_annotation(&SlackClient::default(), &workspace(), &s, &p).unwrap(),
            )
            .unwrap();
            let accepted = complete_anchor(&mut r, &s, p, request).unwrap();
            let canonical = r.canonical.clone().unwrap();
            let proof = accepted.confirm(&canonical).unwrap();
            assert_eq!(
                canonical
                    .sections
                    .iter()
                    .filter(|x| before.iter().any(|b| b.id() == x.id()))
                    .cloned()
                    .collect::<Vec<_>>(),
                before
            );
            let bootstrap = BootstrapIntent::capture(proof, 1_720_000_000_000).unwrap();
            let request = wire(
                api::canvas_comment_bootstrap(&SlackClient::default(), &workspace(), &bootstrap)
                    .unwrap(),
            )
            .unwrap();
            complete_bootstrap(&mut r, bootstrap, request).unwrap();
            assert_eq!(r.prepared.len(), 2);
        }
    }
}
