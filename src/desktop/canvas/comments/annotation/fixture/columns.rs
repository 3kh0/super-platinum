//! Named loaded column bodies using the existing owned worker and linear export.
use super::*;
const PREFIX: &str = "https://table-image-fixture.invalid";
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-flexbox"
                | "canvas-comment-flexbox-narrow"
                | "canvas-comment-callout"
                | "canvas-comment-callout-narrow")
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
    let callout = matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("canvas-comment-callout" | "canvas-comment-callout-narrow")
    );
    seed_kind(snapshot, callout)
}
fn seed_kind(snapshot: CanvasCollabSnapshot, callout: bool) -> Record {
    let mut record = blockquote::seed(snapshot);
    let s = record.canonical.as_mut().unwrap();
    s.sections.retain(|s| s.id() != "modern-table");
    let at = s
        .sections
        .iter()
        .position(|s| s.id() == "paragraph")
        .unwrap();
    s.sections[at] = changed(&s.sections[at], n(29, 0));
    let at = s
        .sections
        .iter()
        .position(|s| s.id() == "outer-quote")
        .unwrap();
    let (kind, style, tag, member) = if callout {
        (69, 49, 63, 2)
    } else {
        (26, 21, 24, 1)
    };
    let column = |name: &str, ids: &[&str]| {
        let mut fields = if callout {
            vec![t(1, name)]
        } else {
            vec![n(2, 3)]
        };
        fields.extend(ids.iter().map(|id| t(member, id)));
        m(1, &raw(&fields))
    };
    for f in [
        t(1, "column-root"),
        n(9, kind),
        n(10, style),
        n(29, 0),
        m(
            12,
            &raw(&[m(
                tag,
                &raw(&[
                    column("first", &["table-second", "table-deleted"]),
                    column("second", &["inner-quote"]),
                    n(2, 0),
                ]),
            )]),
        ),
    ] {
        s.sections[at] = changed(&s.sections[at], f);
    }
    record
}
pub(super) fn document(
    s: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    s.section("column-root")?;
    let paragraph = s
        .section("paragraph")?
        .raw()
        .message(12)
        .ok()??
        .message(1)
        .ok()??;
    let mut html = format!(
        "<div class='quip-canvas-content'><h1>Column body comments</h1><p id='paragraph'>{}</p><div id='column-root'><div data-section-style='7'><ul id='table-second'>",
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
    html.push_str(&format!("</ul></div><blockquote id='inner-quote'><div id='table-image'><img src='{}' alt='Table image'></div></blockquote></div></div>",url(s)));
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
    fn actual_column_fixtures_preserve_both_roots_through_real_annotation_and_bootstrap() {
        for (image, callout) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut r = seed_kind(initial(), callout);
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
                13
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
