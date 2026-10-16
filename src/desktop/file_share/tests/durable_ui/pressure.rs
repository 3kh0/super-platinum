use super::*;
use crate::file_share::pressure;

async fn failed_capture(f: &mut Fixture, index: usize) -> Ticket {
    let mut body = note();
    body[0]["future"] = json!(index);
    let capture = f.capture();
    let visit = capture.visit.clone();
    let ticket = capture
        .persist(body, Some(destination()), true)
        .await
        .unwrap();
    let outcome = result(&ticket).await;
    assert!(matches!(outcome, Outcome::Failed(Refusal::Storage)));
    assert!(durable::apply_capture(
        &mut f.shell,
        &f.id,
        &visit,
        &ticket,
        &outcome
    ));
    ticket
}

#[tokio::test]
async fn explicit_earlier_capture_discard_frees_one_full_queue_slot_without_rebasing_current_editor()
 {
    let mut f = Fixture::new().await;
    let expected = f.draft();
    let storage = f.storage();
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage =
        Some(recovery::Storage::at(storage.key.clone(), f.temp.0.clone()));
    let mut failures = Vec::new();
    for i in 0..32 {
        failures.push(failed_capture(&mut f, i).await);
    }
    let queue = f.queue();
    assert_eq!(queue.retained().unwrap().len(), 32);
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage = Some(storage.clone());
    assert!(matches!(
        Capture::prepare(&mut f.shell, &f.id),
        Err(Refusal::Capacity)
    ));
    // Model seeds below are not a DOM simulation. Native verification must also
    // prove actual editor node, selection and undo state survive these controls.
    let mut current_body = note();
    current_body[0]["future"] = json!("current unsampled editor body");
    let d = f.shell.file_share.dialog.as_mut().unwrap();
    d.note = current_body.clone();
    d.destination = Some(destination());
    let old_error = d.error;
    let old_visit = d.draft_visit.clone().unwrap();
    let preview = pressure::capture(&f.shell, &f.id).unwrap();
    assert_eq!(preview.note[0]["future"], json!(31));
    assert_eq!(preview.destination.as_deref(), Some("ship"));
    assert_eq!(queue.retained().unwrap().len(), 32);
    pressure::discard(&mut f.shell, &preview).unwrap();
    assert!(!pressure::current(&f.shell, &preview));
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    let d = visible(&f.shell).unwrap();
    assert_eq!(d.phase, Phase::Editing);
    assert!(!d.closing && !d.draft_local && d.draft_task.is_none());
    assert!(d.durable.as_ref() == Some(&expected));
    assert_eq!(d.note, current_body);
    assert_eq!(
        d.destination.as_ref().map(|d| d.id.as_str()),
        Some("C00000001")
    );
    assert_eq!(d.error, old_error);
    assert!(Arc::ptr_eq(d.draft_visit.as_ref().unwrap(), &old_visit));
    let retained = queue.retained().unwrap();
    assert_eq!(retained.len(), 31);
    for (index, earlier) in failures[..31].iter().enumerate() {
        assert!(retained.iter().any(|t| t.same(earlier)));
        assert_eq!(
            durable::captured(earlier).unwrap().note[0]["future"],
            json!(index)
        );
    }
    assert!(f.row().await == Some(expected.clone()));
    let ticket = f
        .capture()
        .persist(current_body.clone(), Some(destination()), true)
        .await
        .unwrap();
    let stored = saved(result(&ticket).await);
    assert_eq!(stored.token, expected.token);
    assert_eq!(stored.revision, expected.revision + 1);
    assert_eq!(stored.content.note, current_body);
    let Some(Command::Save {
        expected: actual, ..
    }) = ticket.input()
    else {
        panic!("original save CAS")
    };
    assert!(actual == expected);
    assert_eq!(queue.retained().unwrap().len(), 32);
}

#[tokio::test]
async fn earlier_capture_actions_refuse_retired_owner_dialog_visit_ticket_and_unfinished_input() {
    let mut f = Fixture::new().await;
    let storage = f.storage();
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage =
        Some(recovery::Storage::at(storage.key.clone(), f.temp.0.clone()));
    let original = failed_capture(&mut f, 1).await;
    let preview = pressure::capture(&f.shell, &f.id).unwrap();
    let original_visit = visible(&f.shell).unwrap().draft_visit.clone().unwrap();
    let original_id = f.id.clone();
    f.shell.core.account_epoch += 1;
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    f.shell.core.account_epoch -= 1;
    f.shell.file_share.dialog.as_mut().unwrap().id = "different-dialog".into();
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    f.shell.file_share.dialog.as_mut().unwrap().id = original_id;
    f.shell.file_share.dialog.as_mut().unwrap().closing = true;
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    f.shell.file_share.dialog.as_mut().unwrap().closing = false;
    let other_visit = drafts::Visit::review(&original_visit.key.0, &f.draft()).unwrap();
    f.shell.file_share.dialog.as_mut().unwrap().draft_visit = Some(other_visit);
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    f.shell.file_share.dialog.as_mut().unwrap().draft_visit = None;
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    f.shell.file_share.dialog.as_mut().unwrap().draft_visit = Some(original_visit.clone());
    let reservation = f.queue().reserve(storage, original_visit).unwrap();
    let unfinished = reservation.observe();
    f.shell.file_share.dialog.as_mut().unwrap().draft_task = Some(unfinished.clone());
    assert!(pressure::capture(&f.shell, &f.id).is_err());
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    drop(reservation);
    assert!(matches!(result(&unfinished).await, Outcome::Failed(_)));
    assert!(pressure::capture(&f.shell, &f.id).is_err());
    let replacement = failed_capture(&mut f, 2).await;
    assert!(pressure::discard(&mut f.shell, &preview).is_err());
    let queue = f.queue();
    assert!(queue.retained().unwrap().iter().any(|t| t.same(&original)));
    assert!(
        queue
            .retained()
            .unwrap()
            .iter()
            .any(|t| t.same(&replacement))
    );
    let current = pressure::capture(&f.shell, &f.id).unwrap();
    pressure::discard(&mut f.shell, &current).unwrap();
    assert!(queue.retained().unwrap().iter().any(|t| t.same(&original)));
}

#[tokio::test]
async fn earlier_capture_clear_preview_preserves_empty_input_and_destination_only_preview_is_visible()
 {
    let mut f = Fixture::new().await;
    let storage = f.storage();
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage =
        Some(recovery::Storage::at(storage.key.clone(), f.temp.0.clone()));
    for destination in [None, Some(destination())] {
        let capture = f.capture();
        let visit = capture.visit.clone();
        let ticket = capture
            .persist(vec![], destination.clone(), true)
            .await
            .unwrap();
        let outcome = result(&ticket).await;
        assert!(matches!(outcome, Outcome::Failed(Refusal::Storage)));
        assert!(durable::apply_capture(
            &mut f.shell,
            &f.id,
            &visit,
            &ticket,
            &outcome
        ));
        let preview = pressure::capture(&f.shell, &f.id).unwrap();
        assert!(preview.note.is_empty());
        assert_eq!(preview.destination, destination.map(|d| d.name));
        pressure::discard(&mut f.shell, &preview).unwrap();
    }
    assert!(f.queue().retained().unwrap().is_empty());
}
