//! Manually released read pairs for the disconnected native canvas checker.
use super::*;

pub(in crate::canvas) struct State {
    initial: Arc<CanvasCollabSnapshot>,
    first: checklist::Read,
    phase: u8,
    command: Option<u8>,
    pairs: u8,
    late_rejected: bool,
}
fn enabled(shell: &ShellState) -> bool {
    text::fixture::enabled(shell)
        && std::env::var("SUPER_PLATINUM_CANVAS_TEXT_CHECK").as_deref() == Ok("1")
}
pub(in crate::canvas) fn start(shell: &mut ShellState, reader: &mut Reader) {
    if !enabled(shell) {
        return;
    }
    let Some(initial) = checklist::presentation(shell).and_then(|view| view.snapshot) else {
        return;
    };
    checklist::open_visit(shell);
    let Some(first) = checklist::begin_read(shell) else {
        return;
    };
    *reader = Reader {
        generation: 1,
        loading: true,
        ..Default::default()
    };
    shell.canvas_checklist.text_fixture.as_mut().unwrap().loader = Some(State {
        initial,
        first,
        phase: 0,
        command: None,
        pairs: 1,
        late_rejected: false,
    });
}
pub(in crate::canvas) fn status(shell: &ShellState) -> Option<(u8, u8, bool)> {
    if !enabled(shell) {
        return None;
    }
    let state = shell
        .canvas_checklist
        .text_fixture
        .as_ref()?
        .loader
        .as_ref()?;
    Some((state.phase, state.pairs, state.late_rejected))
}
pub(in crate::canvas) fn queue(shell: &mut ShellState, phase: u8) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some(state) = shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .and_then(|f| f.loader.as_mut())
    else {
        return false;
    };
    if state.phase + 1 != phase || phase > 3 || state.command.is_some() {
        return false;
    }
    state.command = Some(phase);
    true
}
pub(in crate::canvas) fn pending(shell: &ShellState) -> bool {
    enabled(shell)
        && shell
            .canvas_checklist
            .text_fixture
            .as_ref()
            .and_then(|f| f.loader.as_ref())
            .is_some_and(|state| state.command.is_some())
}
fn exported(title: &str, preview: bool) -> Export {
    let file = serde_json::from_value(
        serde_json::json!({"id":"Ffixture","filetype":"quip","title":title,"editable":true}),
    )
    .unwrap();
    let html = if preview {
        text::fixture::HTML.replace(
            "A <b>bold</b> line 🦊 日本語 &amp; details.",
            "First-load preview",
        )
    } else {
        text::fixture::HTML.into()
    };
    Ok((file, CanvasDocument::parse(&html).unwrap()))
}
pub(in crate::canvas) fn tick(shell: &mut ShellState, reader: &mut Reader) {
    if !pending(shell) {
        return;
    }
    let mut state = shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .unwrap()
        .loader
        .take()
        .unwrap();
    let phase = state.command.take().unwrap();
    match phase {
        1 => {
            checklist::invalidate(shell, "T1", "Ffixture");
            assert!(finish(
                shell,
                reader,
                &state.first,
                1,
                Ok(state.initial.clone()),
                exported("First-load preview", true)
            ));
        }
        2 | 3 => {
            if phase == 2 {
                // Ten notifications still admit one request pair.
                for _ in 0..10 {
                    checklist::invalidate(shell, "T1", "Ffixture");
                }
            }
            let visit = checklist::event_reads::needed(shell).unwrap();
            let read = checklist::event_reads::claim(shell, visit).unwrap();
            assert!(checklist::event_reads::claim(shell, visit).is_none());
            state.pairs += 1;
            reader.generation += 1;
            reader.loading = true;
            if phase == 2 {
                checklist::invalidate(shell, "T1", "Ffixture");
            }
            let generation = reader.generation;
            assert!(finish(
                shell,
                reader,
                &read,
                generation,
                Ok(state.initial.clone()),
                exported(
                    if phase == 2 {
                        "Superseded export"
                    } else {
                        "Paragraph editing verification"
                    },
                    false
                )
            ));
            if phase == 3 {
                state.late_rejected = !finish(
                    shell,
                    reader,
                    &state.first,
                    1,
                    Ok(state.initial.clone()),
                    exported("Late obsolete export", true),
                );
            }
        }
        _ => unreachable!(),
    }
    state.phase = phase;
    shell.canvas_checklist.text_fixture.as_mut().unwrap().loader = Some(state);
}
