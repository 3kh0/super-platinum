//! Offline one-row insert-before. Original source bytes never become an update.
use super::*;
use super_platinum_core::canvas::edit::structure::StructureIntent;
const START_ASCII: Corpus = Corpus {
    left: ASCII.seed,
    left_rtml: ASCII.seed_rtml,
    right: "",
    offset: 0,
    edited: "Keep thisSecond paragraph edited",
    edited_rtml: "Keep thisSecond paragraph edited",
    ..ASCII
};
const START_UNICODE: Corpus = Corpus {
    left: UNICODE.seed,
    left_rtml: UNICODE.seed_rtml,
    right: "",
    offset: 0,
    edited: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph edited",
    edited_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second paragraph edited",
    ..UNICODE
};
pub(super) fn corpus() -> &'static Corpus {
    if std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS").as_deref() == Ok("unicode") {
        &START_UNICODE
    } else {
        &START_ASCII
    }
}
pub(in crate::canvas::text) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: StructureIntent,
) -> Result<(), &'static str> {
    if !enabled(&state.peek()) || !is_start() {
        return Err("The disconnected insertion fixture is unavailable.");
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
    let prepared = intent.prepare(&fresh, checklist::save::now_usec().unwrap());
    state.write().canvas_checklist.split_fixture.as_mut().unwrap().preflights.push(json!({
        "source":inventory(&attempt.snapshot),"fresh":inventory(&fresh),"outcome":match &prepared {Ok(_)=>"prepared".to_owned(),Err(e)=>format!("{e:?}")}}));
    let prepared =
        prepared.map_err(|_| "This paragraph or your access changed. Cancel and refresh.")?;
    let ticket = checklist::recovery::reserve(state, attempt, &prepared).await?;
    if !checklist::structure_ops::mark_dispatched(&mut state.write(), attempt, &prepared) {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas view changed before the edit was sent.");
    }
    let mut shell = state.write();
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    f.reservations += 1;
    f.writes.push(json!({"kind":"start","parent":"paragraph","child":prepared.target_section(),
        "documentData":STANDARD.encode(prepared.document_data().raw()),"formTitle":prepared.form_title(),
        "session":prepared.session_id(),"sequence":prepared.sequence(),"httpFactoryExecuted":false}));
    f.pending_start = Some((attempt.clone(), prepared));
    Ok(())
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
        .pending_start
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
    let sent = prepared.document_data().message(1).unwrap().unwrap();
    let position = sent
        .string(8)
        .unwrap()
        .unwrap()
        .strip_suffix(":temp")
        .unwrap();
    let wire = sent
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, 1000)])
        .unwrap()
        .replace_fields(
            4,
            &[n(4, checklist::save::now_usec().unwrap() + 60_000_000)],
        )
        .unwrap();
    let row = sent
        .replace_fields(2, &[n(2, 1000)])
        .unwrap()
        .replace_fields(6, &[t(6, previous.thread_id())])
        .unwrap()
        .replace_fields(7, &[t(7, previous.document_id())])
        .unwrap()
        .replace_fields(8, &[t(8, position)])
        .unwrap()
        .replace_fields(21, &[t(21, position)])
        .unwrap()
        .replace_fields(12, &[m(12, &body("", false))])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let partial = scenario() == "partial";
    let mut data = vec![t(15, previous.document_id())];
    if !partial {
        data.push(m(1, &row));
    }
    let ack_bytes = msg(&[n(1, 1), m(2, &msg(&data))]);
    let ack = EditAck::decode(ack_bytes.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack);
    state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .acknowledgements
        .push(json!({
        "raw":STANDARD.encode(ack_bytes.raw()),"partial":partial,"accepted":accepted.is_ok()}));
    if partial {
        return accepted.is_err()
            && checklist::finish(&mut state.write(), &attempt, checklist::Outcome::Uncertain);
    }
    let Ok(accepted) = accepted else {
        return false;
    };
    let mut rows = Vec::new();
    for prior in &previous.sections {
        if prior.id() == "paragraph" {
            rows.push(row.clone());
        }
        rows.push(prior.raw().clone());
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
        if !checklist::structure_ops::finish_structure(&mut shell, &attempt, accepted) {
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
    use super_platinum_core::canvas::edit::text::TextDraft;
    #[test]
    fn start_corpora_keep_full_original_and_new_row_is_empty() {
        for c in [&START_ASCII, &START_UNICODE] {
            assert_eq!(c.offset, 0);
            assert_eq!(c.seed, c.left);
            assert_eq!(c.seed_rtml, c.left_rtml);
            assert!(c.right.is_empty());
            assert_eq!(c.edited, format!("{} edited", c.seed));
            assert_eq!(c.edited_rtml, format!("{} edited", c.seed_rtml));
            assert_eq!(
                TextDraft::from_rtml(c.seed_rtml)
                    .unwrap()
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>(),
                c.seed
            );
        }
    }
}
