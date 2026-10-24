use super::*;

fn setup() -> (ShellState, Reader) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Canvas".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    crate::canvas::checklist::open_visit(&mut shell);
    let read = crate::canvas::checklist::begin_read(&mut shell).unwrap();
    assert!(crate::canvas::checklist::finish_read(
        &mut shell,
        &read,
        Ok(text::fixture::snapshot("Canonical text", 1000))
    ));
    let mut reader = Reader {
        generation: 1,
        document: Some(Arc::new(
            CanvasDocument::parse(
                "<div class='quip-canvas-content'><p id='paragraph'>Lagging export</p></div>",
            )
            .unwrap(),
        )),
        ..Default::default()
    };
    super::super::update_mapping(&shell, &mut reader);
    (shell, reader)
}

#[test]
fn mapping_queue_is_pure_coalesced_and_publishes_only_after_preparation() {
    fn send<T: Send + 'static>() {}
    send::<Input>();
    send::<Prepared>();
    let (shell, mut reader) = setup();
    assert!(reader.text.items.is_empty());
    assert!(reader.mapping.ready());
    assert!(!needed(&shell, &reader));
    let (owner, input) = reader.mapping.claim().unwrap();
    assert!(!reader.mapping.ready());
    assert!(reader.mapping.claim().is_none());
    assert!(
        !needed(&shell, &reader),
        "ordinary shell ticks do not schedule proofs"
    );
    assert!(finish(&shell, &mut reader, &owner, Some(input.run())));
    let item = reader.text.items.get("paragraph").unwrap();
    assert!(item.blocks.contains("Canonical text"));
    assert!(!item.blocks.contains("Lagging export"));
    assert!(item.intent.is_some());
    assert!(!reader.mapping.ready());
    assert!(!needed(&shell, &reader));
}

#[test]
fn mapping_worker_rejects_exact_owner_and_object_substitution() {
    for defect in 0..14 {
        let (mut shell, mut reader) = setup();
        let (owner, input) = reader.mapping.claim().unwrap();
        let proof = input.run();
        match defect {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("replacement-account".into()),
            2 => shell.self_account.user_id = "replacement-viewer".into(),
            3 => shell.core.active_team = Some("replacement-team".into()),
            4 => shell.canvas_checklist.view.as_mut().unwrap().revision += 1,
            5 => shell.canvas_checklist.view.as_mut().unwrap().read += 1,
            6 => shell.canvas_checklist.view.as_mut().unwrap().loading = true,
            7 => shell.canvas_checklist.view.as_mut().unwrap().stale = true,
            8 => reader.generation += 1,
            9 => {
                let view = shell.canvas_checklist.view.as_mut().unwrap();
                view.snapshot = Some(Arc::new((**view.snapshot.as_ref().unwrap()).clone()));
            }
            10 => reader.document = Some(Arc::new(
                CanvasDocument::parse(
                    "<div class='quip-canvas-content'><p id='paragraph'>Lagging export</p></div>",
                )
                .unwrap(),
            )),
            11 => shell.overlay = None,
            12 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .deleted_files
                    .insert("Ffixture".into());
            }
            13 => {
                shell.shortcut_generation += 1;
                crate::canvas::checklist::open_visit(&mut shell);
            }
            _ => unreachable!(),
        }
        assert!(needed(&shell, &reader), "defect {defect}");
        assert!(
            !finish(&shell, &mut reader, &owner, Some(proof)),
            "defect {defect}"
        );
        assert!(reader.text.items.is_empty());
        assert!(reader.checklist.items.is_empty());
    }
}

#[test]
fn mapping_replacement_queue_and_worker_failure_never_publish_old_actions() {
    let (shell, mut reader) = setup();
    let (old, input) = reader.mapping.claim().unwrap();
    let proof = input.run();
    super::super::update_mapping(&shell, &mut reader);
    assert!(
        !reader.mapping.ready(),
        "only one worker runs while the latest request waits"
    );
    assert!(!finish(&shell, &mut reader, &old, Some(proof)));
    assert!(reader.mapping.ready());
    assert!(reader.text.items.is_empty());
    let (current, input) = reader.mapping.claim().unwrap();
    drop(input);
    assert!(!finish(&shell, &mut reader, &current, None));
    assert!(reader.text.items.is_empty());
    assert!(
        !reader.mapping.ready(),
        "worker failure does not spin on ordinary ticks"
    );
    assert!(!needed(&shell, &reader));
    super::super::update_mapping(&shell, &mut reader);
    assert!(
        settle_for_test(&shell, &mut reader),
        "an explicit refresh can retry"
    );
}

#[test]
fn reset_reader_cannot_receive_previous_worker_or_clear_new_running_job() {
    let (shell, mut reader) = setup();
    let (old, input) = reader.mapping.claim().unwrap();
    let proof = input.run();
    reader.mapping = State::default();
    super::super::update_mapping(&shell, &mut reader);
    let (current, input) = reader.mapping.claim().unwrap();
    assert!(!finish(&shell, &mut reader, &old, Some(proof)));
    assert!(
        reader
            .mapping
            .running
            .as_ref()
            .is_some_and(|r| Arc::ptr_eq(r, &current))
    );
    assert!(finish(&shell, &mut reader, &current, Some(input.run())));
}
