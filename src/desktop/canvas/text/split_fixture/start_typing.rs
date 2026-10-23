//! Offline typed-start pair; the blank before original is never edited here.
use super::*;
use super_platinum_core::canvas::edit::structure::{StartTypingIntent, StartTypingWitness};
pub(super) fn reports(kind: &str, unicode: bool) -> &'static [&'static str] {
    match (kind, unicode) {
        ("correction", false) => &[
            "New Keep thisSecond paragraph",
            "New draftx Keep thisSecond paragraph",
            "New draft Keep thisSecond paragraph",
            "New paragraph Keep thisSecond paragraph",
        ],
        ("correction", true) => &[
            "New Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
            "New \u{65e5}\u{672c}\u{1f600} draftx Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
            "New \u{65e5}\u{672c}\u{1f600} draft Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
            "New \u{65e5}\u{672c}\u{1f600} paragraph Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
        ],
        ("erase", false) => &["xKeep thisSecond paragraph", ""],
        ("erase", true) => &["xCaf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph", ""],
        ("restore", false) => &["xKeep thisSecond paragraph", "Keep thisSecond paragraph"],
        ("restore", true) => &[
            "xCaf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
        ],
        _ => &[],
    }
}
pub(super) fn parent_source(raw: &RawMessage) -> RawMessage {
    if parent_mode() == "bootstrap" {
        return raw.remove(38).unwrap();
    }
    let seeds: Value = serde_json::from_str(include_str!("start-typing-original.json")).unwrap();
    let seed = seeds["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sourceMode"] == "retained" && s["corpus"] == corpus().name)
        .unwrap();
    assert_eq!(seed["rtml"], corpus().seed_rtml);
    let wire = RawMessage::decode(
        &STANDARD
            .decode(seed["canonicalSessionBase64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
    .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
    .unwrap();
    raw.replace_fields(38, &[m(38, &wire)]).unwrap()
}
pub(in crate::canvas::text) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: StartTypingIntent,
    witness: &StartTypingWitness,
) -> Result<(), &'static str> {
    if !enabled(&state.peek()) || !is_start_typing() {
        return Err("The disconnected typed insertion fixture is unavailable.");
    }
    let fresh = state
        .peek()
        .canvas_checklist
        .split_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let prepared = intent.prepare(&fresh, witness, checklist::save::now_usec().unwrap());
    state.write().canvas_checklist.split_fixture.as_mut().unwrap().preflights.push(json!({"source":inventory(&attempt.snapshot),"fresh":inventory(&fresh),"outcome":match &prepared {Ok(_)=>"prepared".to_owned(),Err(e)=>format!("{e:?}")}}));
    let prepared =
        prepared.map_err(|_| "This paragraph or your access changed. Cancel and refresh.")?;
    let ticket = checklist::recovery::reserve_start_typing(state, attempt, &prepared).await?;
    if !checklist::start_typing_ops::mark_dispatched(&mut state.write(), attempt, &prepared) {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas view changed before the edit was sent.");
    }
    let mut shell = state.write();
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    f.reservations += 1;
    f.writes.push(json!({"kind":"start","parent":prepared.target_section(),"child":prepared.child_section(),"documentData":STANDARD.encode(prepared.document_data().raw()),"formTitle":prepared.form_title(),"session":prepared.session_id(),"sequence":prepared.sequence(),"httpFactoryExecuted":false}));
    f.pending_start_typing = Some((attempt.clone(), prepared));
    Ok(())
}
fn canonical(
    sent: &RawMessage,
    previous: &CanvasCollabSnapshot,
    original: bool,
    parent_text: &str,
) -> RawMessage {
    let sequence = if original { 2000 } else { 1000 };
    let wire = sent
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(
            4,
            &[n(4, checklist::save::now_usec().unwrap() + 60_000_000)],
        )
        .unwrap();
    let escaped = typing::escaped(parent_text);
    let content = body(if original { &escaped } else { "" }, original);
    if original {
        previous
            .section("paragraph")
            .unwrap()
            .raw()
            // Hydrate all returned status metadata, including modified=1. The
            // sparse captured source legitimately has no fields 9/13/19.
            .merge_partial(sent)
            .unwrap()
            .replace_fields(2, &[n(2, sequence)])
            .unwrap()
            .replace_fields(12, &[m(12, &content)])
            .unwrap()
            .replace_fields(20, &[n(20, sent.uint(20).unwrap().unwrap())])
            .unwrap()
            .replace_fields(33, &[t(33, sent.string(33).unwrap().unwrap())])
            .unwrap()
            .replace_fields(38, &[m(38, &wire)])
            .unwrap()
    } else {
        let position = sent
            .string(8)
            .unwrap()
            .unwrap()
            .strip_suffix(":temp")
            .unwrap();
        sent.replace_fields(2, &[n(2, sequence)])
            .unwrap()
            .replace_fields(6, &[t(6, previous.thread_id())])
            .unwrap()
            .replace_fields(7, &[t(7, previous.document_id())])
            .unwrap()
            .replace_fields(8, &[t(8, position)])
            .unwrap()
            .replace_fields(21, &[t(21, position)])
            .unwrap()
            .replace_fields(12, &[m(12, &content)])
            .unwrap()
            .replace_fields(38, &[m(38, &wire)])
            .unwrap()
    }
}
pub(super) async fn complete(mut state: Signal<ShellState>) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let Some((attempt, prepared)) = state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .pending_start_typing
        .take()
    else {
        return false;
    };
    let previous = state
        .peek()
        .canvas_checklist
        .split_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let rows = prepared.document_data().messages(1).unwrap();
    assert_eq!(rows.len(), 2);
    let parent = canonical(&rows[0], &previous, true, parent_text());
    let child = canonical(&rows[1], &previous, false, parent_text());
    let partial = scenario() == "partial";
    let mut data = vec![t(15, previous.document_id()), m(1, &parent)];
    if !partial {
        data.push(m(1, &child));
    }
    let bytes = msg(&[n(1, 1), m(2, &msg(&data))]);
    let ack = EditAck::decode(bytes.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack);
    state.write().canvas_checklist.split_fixture.as_mut().unwrap().acknowledgements.push(json!({"raw":STANDARD.encode(bytes.raw()),"partial":partial,"accepted":accepted.is_ok(),"error":accepted.as_ref().err().map(|e|format!("{e:?}"))}));
    if partial {
        return accepted.is_err()
            && checklist::finish(&mut state.write(), &attempt, checklist::Outcome::Uncertain);
    }
    let Ok(accepted) = accepted else {
        return false;
    };
    let mut rows = Vec::new();
    for old in &previous.sections {
        if old.id() == "paragraph" {
            rows.push(child.clone());
            rows.push(parent.clone());
        } else {
            rows.push(old.raw().clone());
        }
    }
    let fresh = rebuild(&previous, rows);
    if !accepted.confirms_readback(&fresh).unwrap_or(false) {
        return false;
    }
    {
        let mut shell = state.write();
        let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
        f.readbacks.push(inventory(&fresh));
        f.canonical = Some(fresh.clone());
        if !checklist::start_typing_ops::finish_start_typing(&mut shell, &attempt, accepted) {
            return false;
        }
    }
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return false;
    };
    {
        let mut shell = state.write();
        checklist::finish_read(&mut shell, &read, Ok(fresh));
        checklist::confirm_readback(&mut shell, &attempt, &read);
    }
    checklist::recovery::settle(state, &read).await;
    checklist::text_ops::settled(&state.peek(), &attempt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{structure::StartTypingWitness, text::TextDraft};
    #[test]
    fn sparse_original_ack_hydrates_modified_status_before_complete_pair_validation() {
        let mut source = (*structure_fixture::snapshot()).clone();
        let original = source
            .sections
            .iter_mut()
            .find(|s| s.id() == "paragraph")
            .unwrap();
        *original = Section::from_raw(
            original
                .raw()
                .replace_fields(12, &[m(12, &body("Seed", true))])
                .unwrap(),
        )
        .unwrap();
        assert!(!original.raw().has(19));
        let intent = StartTypingIntent::capture(&source, "paragraph", 100).unwrap();
        let witness =
            StartTypingWitness::new(intent.seed(), &[TextDraft::from_rtml("xSeed").unwrap()])
                .unwrap();
        let prepared = intent.prepare(&source, &witness, 100).unwrap();
        let outgoing = prepared.document_data().messages(1).unwrap();
        let parent = canonical(&outgoing[0], &source, true, "xSeed");
        let child = canonical(&outgoing[1], &source, false, "xSeed");
        assert_eq!(parent.uint(19).unwrap(), Some(1));
        assert_eq!(parent.uint(9).unwrap(), Some(0));
        let ack = |parent: &RawMessage| {
            EditAck::decode(
                msg(&[
                    n(1, 1),
                    m(
                        2,
                        &msg(&[t(15, source.document_id()), m(1, parent), m(1, &child)]),
                    ),
                ])
                .raw(),
            )
            .unwrap()
        };
        let accepted = prepared.validate_ack(&ack(&parent)).unwrap();
        let mut fresh = source.clone();
        let row = fresh
            .sections
            .iter_mut()
            .find(|s| s.id() == "paragraph")
            .unwrap();
        *row = Section::from_raw(parent.clone()).unwrap();
        fresh
            .sections
            .push(Section::from_raw(child.clone()).unwrap());
        assert!(accepted.confirms_readback(&fresh).unwrap());
        assert!(
            prepared
                .validate_ack(&ack(&parent.remove(19).unwrap()))
                .is_err()
        );
    }
    #[test]
    fn typed_start_fixture_reports_preserve_full_source_and_revert() {
        for (unicode, source) in [(false, ASCII.seed_rtml), (true, UNICODE.seed_rtml)] {
            let seed = TextDraft::from_rtml(source).unwrap();
            for kind in ["correction", "erase", "restore"] {
                let updates = reports(kind, unicode)
                    .iter()
                    .map(|s| TextDraft::from_rtml(&typing::escaped(s)).unwrap())
                    .collect::<Vec<_>>();
                let witness = StartTypingWitness::new(&seed, &updates).unwrap();
                assert_eq!(
                    witness.original_updates().len(),
                    if kind == "correction" { 4 } else { 2 }
                );
                if kind == "restore" {
                    assert!(witness.parent_draft() == &seed);
                }
                if kind == "erase" {
                    assert!(witness.parent_draft() == &TextDraft::default());
                }
            }
        }
    }
    #[test]
    fn typed_start_retained_seeds_match_exact_unicode_source_without_base_flip() {
        let source: Value =
            serde_json::from_str(include_str!("start-typing-original.json")).unwrap();
        for seed in source["seeds"].as_array().unwrap() {
            let expected = if seed["corpus"] == "ascii" {
                ASCII.seed_rtml
            } else {
                UNICODE.seed_rtml
            };
            assert_eq!(seed["rtml"], expected);
            if seed["sourceMode"] == "bootstrap" {
                assert!(seed["canonicalSessionBase64"].is_null());
                continue;
            }
            let raw = RawMessage::decode(
                &STANDARD
                    .decode(seed["canonicalSessionBase64"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(raw.string(1).unwrap(), Some("paragraph"));
            assert_eq!(raw.uint(2).unwrap(), Some(1000));
            assert_eq!(raw.uint(3).unwrap(), Some(1000));
            assert!(raw.uint(4).unwrap().is_none());
        }
    }
}
