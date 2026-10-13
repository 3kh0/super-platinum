use super::*;

fn opening() -> (ShellState, Reader, checklist::Read) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(CanvasTarget {
        id: "Ffixture".into(),
        title: "Canvas".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    checklist::open_visit(&mut shell);
    let read = checklist::begin_read(&mut shell).unwrap();
    let reader = Reader {
        generation: 1,
        loading: true,
        ..Default::default()
    };
    (shell, reader, read)
}

fn canonical(text: &str) -> Canonical {
    Ok(text::fixture::snapshot(text, 1000))
}

fn export(title: &str) -> Export {
    let file = serde_json::from_value(serde_json::json!({
        "id":"Ffixture", "title":title, "filetype":"quip", "editable":true
    }))
    .unwrap();
    let document = CanvasDocument::parse(
        "<div class='quip-canvas-content'><p id='paragraph'><span id='paragraph'>Exported paragraph</span></p></div>"
    ).unwrap();
    Ok((file, document))
}

fn start_reader(reader: &mut Reader) -> u64 {
    reader.generation += 1;
    reader.loading = true;
    reader.generation
}

#[test]
fn first_load_event_installs_readonly_preview_then_latest_pair_without_manual_refresh() {
    let (mut shell, mut reader, first) = opening();
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(finish(
        &mut shell,
        &mut reader,
        &first,
        1,
        canonical("Old"),
        export("Preview")
    ));
    assert!(
        reader.document.is_some(),
        "an event stream must not starve first paint"
    );
    assert!(!reader.loading);
    assert!(!reader.editable);
    assert!(
        reader.text.items.is_empty(),
        "an export alone grants no text edit authority"
    );
    let view = checklist::presentation(&shell).unwrap();
    assert!(view.snapshot.is_none() && view.stale);

    let visit = checklist::event_reads::needed(&shell).unwrap();
    let next = checklist::event_reads::claim(&mut shell, visit).unwrap();
    let generation = start_reader(&mut reader);
    assert!(checklist::event_reads::needed(&shell).is_none());
    assert!(finish(
        &mut shell,
        &mut reader,
        &next,
        generation,
        canonical("Latest"),
        export("Latest title")
    ));
    assert_eq!(reader.title.as_deref(), Some("Latest title"));
    assert!(reader.editable);
    assert!(!reader.loading);
    assert!(!checklist::presentation(&shell).unwrap().stale);
    assert_eq!(reader.text.items.len(), 1);
    assert!(checklist::event_reads::needed(&shell).is_none());
}

#[test]
fn event_bursts_coalesce_and_preserve_preview_until_a_current_pair_finishes() {
    let (mut shell, mut reader, first) = opening();
    let visit = checklist::presentation(&shell).unwrap().visit;
    let mut read = first;
    for index in 0..3 {
        for _ in 0..10 {
            checklist::invalidate(&mut shell, "T1", "Ffixture");
        }
        let generation = reader.generation;
        assert!(finish(
            &mut shell,
            &mut reader,
            &read,
            generation,
            canonical("Old"),
            export("First preview")
        ));
        assert!(reader.document.is_some());
        assert!(!reader.editable);
        assert!(checklist::presentation(&shell).unwrap().snapshot.is_none());
        read = checklist::event_reads::claim(&mut shell, visit).unwrap();
        assert!(checklist::event_reads::claim(&mut shell, visit).is_none());
        assert_eq!(start_reader(&mut reader), index + 2);
    }
    let generation = reader.generation;
    assert!(finish(
        &mut shell,
        &mut reader,
        &read,
        generation,
        canonical("Settled"),
        export("Latest")
    ));
    assert_eq!(reader.title.as_deref(), Some("Latest"));
    assert!(reader.editable);
    assert!(checklist::event_reads::needed(&shell).is_none());
}

#[test]
fn either_failed_half_latches_event_recovery_even_when_response_was_invalidated() {
    for canonical_fails in [false, true] {
        for invalidated in [false, true] {
            let (mut shell, mut reader, read) = opening();
            if invalidated {
                checklist::invalidate(&mut shell, "T1", "Ffixture");
            }
            let canonical = if canonical_fails {
                Err("Canonical rate limited")
            } else {
                canonical("New")
            };
            let export = if canonical_fails {
                export("Preview")
            } else {
                Err("Export rate limited")
            };
            assert!(finish(&mut shell, &mut reader, &read, 1, canonical, export));
            assert!(!reader.loading && !reader.editable);
            assert!(reader.error.is_some());
            assert!(checklist::presentation(&shell).unwrap().stale);
            for _ in 0..5 {
                checklist::invalidate(&mut shell, "T1", "Ffixture");
            }
            assert!(
                checklist::event_reads::needed(&shell).is_none(),
                "errors must not become event retries"
            );
            let manual = checklist::begin_read(&mut shell).unwrap();
            let generation = start_reader(&mut reader);
            assert!(finish(
                &mut shell,
                &mut reader,
                &manual,
                generation,
                self::canonical("Recovered"),
                self::export("Recovered")
            ));
            assert!(reader.error.is_none() && reader.editable);
        }
    }
}

#[test]
fn superseded_generation_visit_account_or_deleted_file_cannot_install_export_or_release_loading() {
    for change in 0..4 {
        let (mut shell, mut reader, read) = opening();
        match change {
            0 => {
                checklist::invalidate(&mut shell, "T1", "Ffixture");
                checklist::begin_read(&mut shell).unwrap();
                start_reader(&mut reader);
            }
            1 => {
                shell.shortcut_generation += 1;
                checklist::open_visit(&mut shell);
            }
            2 => shell.core.account_epoch += 1,
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .deleted_files
                    .insert("Ffixture".into());
            }
        }
        assert!(!finish(
            &mut shell,
            &mut reader,
            &read,
            1,
            canonical("Stale"),
            export("Stale")
        ));
        assert!(reader.document.is_none() && reader.loading);
    }
}

#[test]
fn refresh_retains_existing_document_and_failed_or_stale_permissions_do_not_enable_editing() {
    let (mut shell, mut reader, first) = opening();
    assert!(finish(
        &mut shell,
        &mut reader,
        &first,
        1,
        canonical("Original"),
        export("Original")
    ));
    let document = reader.document.clone().unwrap();
    let read = checklist::begin_read(&mut shell).unwrap();
    let generation = start_reader(&mut reader);
    assert!(Arc::ptr_eq(reader.document.as_ref().unwrap(), &document));
    checklist::invalidate(&mut shell, "T1", "Ffixture");
    assert!(finish(
        &mut shell,
        &mut reader,
        &read,
        generation,
        canonical("Older"),
        export("Older")
    ));
    assert!(Arc::ptr_eq(reader.document.as_ref().unwrap(), &document));
    assert_eq!(reader.title.as_deref(), Some("Original"));
    assert!(!reader.editable);
}
