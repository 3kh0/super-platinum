//! Closed underline seed and immutable original operation diagnostics.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(in crate::canvas) const RTML: &str =
    "<u><b>A&amp;\u{1f600}B</b></u><br><i><u>New &amp;\u{1f98a}</u></i> tail";
pub(in crate::canvas) const TEXT: &str = "A&\u{1f600}B\nNew &\u{1f98a} tail";
#[derive(Default)]
pub(super) struct Control {
    lost_next: bool,
    acks: Vec<String>,
    source_changes: usize,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-underline" | "canvas-comment-underline-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested() && eligible(shell) && path(shell).is_some()
}
pub(super) fn snapshot(mut s: CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    // Keep source identity/topology, replacing only this closed paragraph's content.
    let item = s
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    let content = item.raw().message(12).unwrap().unwrap();
    let text = content
        .message(1)
        .unwrap()
        .unwrap()
        .replace_fields(1, &[F::bytes(1, RTML.as_bytes()).unwrap()])
        .unwrap();
    *item = Section::from_raw(
        item.raw()
            .remove(38)
            .unwrap()
            .replace_fields(
                12,
                &[F::message(
                    12,
                    &content
                        .replace_fields(1, &[F::message(1, &text).unwrap()])
                        .unwrap(),
                )
                .unwrap()],
            )
            .unwrap(),
    )
    .unwrap();
    s
}
pub(super) fn seed(s: CanvasCollabSnapshot) -> Record {
    let mut r = existing_seed(s);
    r.underline.acks.clear();
    r
}
pub(super) fn ack(r: &mut Record, ack: &EditAck) -> Result<(), CanvasEditFailure> {
    r.underline.acks.push(STANDARD.encode(ack.raw().raw()));
    if std::mem::take(&mut r.underline.lost_next) {
        return Err(CanvasEditFailure::Uncertain(Error::Transport(
            "synthetic lost underline anchor reply".into(),
        )));
    }
    Ok(())
}
pub(in crate::canvas) fn lose_next(shell: &ShellState) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(failure());
    }
    capability(shell)?
        .record
        .lock()
        .map_err(|_| failure())?
        .underline
        .lost_next = true;
    Ok(())
}
/// A newer canonical sequence preserves content but retires the captured source.
pub(in crate::canvas) fn advance(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(failure());
    }
    let cap = capability(shell)?;
    let mut r = cap.record.lock().map_err(|_| failure())?;
    let row = r
        .canonical
        .as_mut()
        .ok_or_else(failure)?
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .ok_or_else(failure)?;
    let before = STANDARD.encode(row.raw().raw());
    let seq = row.sequence().map_err(|_| failure())?.ok_or_else(failure)? + 1;
    let mut raw = row
        .raw()
        .replace_fields(2, &[F::uint(2, seq).unwrap()])
        .map_err(|_| failure())?;
    if let Some(history) = raw.message(38).map_err(|_| failure())? {
        raw = raw
            .replace_fields(
                38,
                &[F::message(
                    38,
                    &history
                        .replace_fields(3, &[F::uint(3, seq).unwrap()])
                        .map_err(|_| failure())?,
                )
                .unwrap()],
            )
            .map_err(|_| failure())?;
    }
    *row = Section::from_raw(raw).map_err(|_| failure())?;
    let after = STANDARD.encode(row.raw().raw());
    r.underline.source_changes += 1;
    Ok(json!({"before":before,"after":after,"sequence":seq}))
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    if !enabled(shell) {
        return Value::Null;
    }
    let Ok(cap) = capability(shell) else {
        return Value::Null;
    };
    let Ok(r) = cap.record.lock() else {
        return Value::Null;
    };
    let Some(s) = &r.canonical else {
        return Value::Null;
    };
    json!({"seedRequests":r.seed_requests,"requests":r.requests,"prepared":r.prepared,"roots":r.roots,"replies":r.replies,"acks":r.underline.acks,"sourceChanges":r.underline.source_changes,
        "document":STANDARD.encode(s.document.raw()),"sections":s.sections.iter().map(|v|json!({"id":v.id(),"raw":STANDARD.encode(v.raw().raw())})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_seed_and_nested_underline_snippet_remain_separate_from_driven_creation() {
        let mut r = seed(snapshot(initial()));
        let before = r.canonical.clone().unwrap();
        let roots = r.roots.clone();
        assert_eq!(r.seed_requests.len(), 2);
        assert!(r.requests.is_empty() && r.prepared.is_empty());
        let prepared = AnnotationIntent::capture_range(&before, "paragraph", 5, 13)
            .unwrap()
            .prepare_at(&before, crate::canvas::checklist::save::now_usec().unwrap())
            .unwrap();
        let record = prepared.record().unwrap();
        let rich: Value =
            serde_json::from_str(record.snippet()["richText"].as_str().unwrap()).unwrap();
        assert!(rich.to_string().contains("\"underline\":true"));
        assert!(!record.snippet()["mrkdwn"].as_str().unwrap().contains("<u>"));
        let accepted = complete_anchor(
            &mut r,
            &before,
            prepared,
            json!({"closed":"original prepared proof"}),
        )
        .unwrap();
        accepted.confirm(r.canonical.as_ref().unwrap()).unwrap();
        assert_eq!(r.roots, roots);
        assert_eq!(r.requests.len(), 1);
        let raw = r
            .canonical
            .as_ref()
            .unwrap()
            .section("paragraph")
            .unwrap()
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap()
            .string(1)
            .unwrap()
            .unwrap()
            .to_owned();
        assert!(
            raw.contains("<u>")
                && raw.contains(roots[0]["document_comment"]["thread_id"].as_str().unwrap())
        );
        assert_eq!(TEXT.encode_utf16().count(), 18);
    }
}
