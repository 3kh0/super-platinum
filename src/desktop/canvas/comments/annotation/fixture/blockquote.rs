//! Named offline nested quote bodies, reusing exact owned table/image worker.
use super::*;
const PREFIX: &str = "https://table-image-fixture.invalid";
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-blockquote" | "canvas-comment-blockquote-narrow")
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
    let mut record = table_bodies::seed(snapshot);
    let s = record.canonical.as_mut().unwrap();
    let template = s.section("table-second").unwrap().clone();
    for (id, path, ids) in [
        (
            "outer-quote",
            "zz",
            vec!["table-second", "table-deleted", "inner-quote"],
        ),
        ("inner-quote", "zzba", vec!["table-image"]),
    ] {
        let mut q = template.raw().remove(38).unwrap().remove(15).unwrap();
        let members = ids.iter().map(|id| t(1, id)).collect::<Vec<_>>();
        for f in [
            t(1, id),
            t(8, path),
            t(21, path),
            n(9, 71),
            n(10, 50),
            m(12, &raw(&[m(65, &raw(&members))])),
        ] {
            q = q
                .merge_partial(&RawMessage::decode(f.raw()).unwrap())
                .unwrap();
        }
        s.sections.push(Section::from_raw(q).unwrap());
    }
    let i = s
        .sections
        .iter()
        .position(|s| s.id() == "modern-table")
        .unwrap();
    let content = s.sections[i].raw().message(12).unwrap().unwrap();
    let layout = content.message(30).unwrap().unwrap();
    let cells = layout
        .messages(3)
        .unwrap()
        .iter()
        .map(|cell| {
            if cell.string(1).unwrap() == Some("table-cell2") {
                m(3, &cell.replace_fields(4, &[t(4, "outer-quote")]).unwrap())
            } else {
                m(3, cell)
            }
        })
        .collect::<Vec<_>>();
    s.sections[i] = changed(
        &s.sections[i],
        m(
            12,
            &raw(&[m(30, &layout.replace_fields(3, &cells).unwrap())]),
        ),
    );
    record
}
pub(super) fn document(
    s: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    s.section("outer-quote")?;
    let paragraph = s
        .section("paragraph")?
        .raw()
        .message(12)
        .ok()??
        .message(1)
        .ok()??;
    let mut html = format!(
        "<div class='quip-canvas-content'><h1>Blockquote body comments</h1><table id='modern-table'><tbody><tr><td><p id='paragraph'>{}</p></td><td><blockquote id='outer-quote'><div data-section-style='7'><ul id='table-second'>",
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
    html.push_str(&format!("</ul></div><blockquote id='inner-quote'><div id='table-image'><img src='{}' alt='Table image'></div></blockquote></blockquote></td></tr></tbody></table></div>",url(s)));
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
    fn actual_quote_fixture_preserves_nested_roots_through_real_annotation_and_bootstrap() {
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
                12
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
