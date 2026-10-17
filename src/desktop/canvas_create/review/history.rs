use super::*;

pub(super) fn retirable(review: &domain::RetirementReview, id: &str) -> bool {
    let Some(draft) = review.drafts().iter().find(|d| d.id() == id) else {
        return false;
    };
    let attempts: Vec<_> = review
        .attempts()
        .iter()
        .filter(|a| a.draft_id() == id)
        .collect();
    !attempts.is_empty()
        && !review.conflicts().iter().any(|c| c.draft_id() == id)
        && !review
            .late_results()
            .iter()
            .any(|e| e.original().draft_id() == id)
        && attempts.iter().all(|a| match a.delivery() {
            domain::Delivery::CanceledBeforeAdmission | domain::Delivery::RateLimited { .. } => {
                true
            }
            domain::Delivery::Created { .. } => draft.archived(),
            _ => false,
        })
}
fn prepare(
    mut state: Signal<ShellState>,
    visit: &str,
    writer: &str,
) -> Option<(Arc<Editor>, View, super::super::Root)> {
    let mut shell = state.write();
    let root = shell.canvas_creation.clone();
    let d = shell
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit && !d.busy)?;
    let editor = d.editor.clone()?;
    let view = d
        .review
        .as_mut()
        .filter(|v| v.writer == writer && !v.busy)?;
    let snapshot = editor.snapshot();
    if snapshot.serial != view.serial || snapshot.capture.as_deref() != Some(&view.token) {
        return None;
    }
    view.busy = true;
    d.busy = true;
    Some((editor, view.clone(), root))
}
async fn complete(
    mut state: Signal<ShellState>,
    visit: String,
    editor: Arc<Editor>,
    view: View,
    root: super::super::Root,
    result: domain::Result<()>,
) {
    match result {
        Ok(()) => {
            editor.release_capture(&view.token);
            root.release(&editor);
            let current = {
                let shell = state.peek();
                super::super::controller::owner(&shell).as_ref() == Some(editor.storage.owner())
                    && shell.overlay == Some(super::super::Overlay::CanvasCreate)
                    && shell
                        .canvas_create
                        .as_ref()
                        .is_some_and(|d| d.visit == visit)
            };
            if current {
                super::super::controller::close_exact(state, &visit);
                super::super::controller::open(state);
            }
        }
        Err(_) => {
            if let Some(d) = state
                .write()
                .canvas_create
                .as_mut()
                .filter(|d| d.visit == visit)
            {
                d.busy = false;
                if let Some(v) = d.review.as_mut() {
                    v.busy = false;
                    v.error = Some(
                        "The receipt inventory changed or storage failed. Retained history was not automatically discarded; review again or retry this exact action.",
                    );
                }
            }
        }
    }
}
pub(super) fn retire_history(state: Signal<ShellState>, visit: &str, writer: &str, id: &str) {
    let Some((editor, view, root)) = prepare(state, visit, writer) else {
        return;
    };
    let visit = visit.to_owned();
    let id = id.to_owned();
    adapter::retain(async move {
        let expected = view.inventory.clone();
        let result = editor
            .storage
            .run(move |store| {
                if !retirable(&expected, &id) {
                    return Err(domain::Fault::Blocked);
                }
                let selected = expected
                    .attempts()
                    .iter()
                    .filter(|a| a.draft_id() == id)
                    .map(|a| {
                        domain::RetirementSelection::new(
                            a,
                            match a.delivery() {
                                domain::Delivery::Created { file_id } => Some(file_id.as_str()),
                                _ => None,
                            },
                        )
                    })
                    .collect::<domain::Result<Vec<_>>>()?;
                store.retire_terminal(&expected, &selected).map(|_| ())
            })
            .await;
        complete(state, visit, editor, view, root, result).await;
    });
}
pub(super) fn dispose_evidence(state: Signal<ShellState>, visit: &str, writer: &str, id: &str) {
    let Some((editor, view, root)) = prepare(state, visit, writer) else {
        return;
    };
    let Some(evidence) = view
        .inventory
        .late_results()
        .iter()
        .find(|e| e.id() == id)
        .cloned()
    else {
        return;
    };
    let visit = visit.to_owned();
    adapter::retain(async move {
        let result = editor
            .storage
            .run(move |store| match store.dispose_late_result(&evidence) {
                Ok(()) | Err(domain::Fault::Retired) => Ok(()),
                Err(e) => Err(e),
            })
            .await;
        complete(state, visit, editor, view, root, result).await;
    });
}
pub(super) fn open_result(
    mut state: Signal<ShellState>,
    visit: &str,
    writer: &str,
    id: &str,
    title: &str,
) {
    let (editor, view, root) = {
        let shell = state.peek();
        let Some(d) = shell
            .canvas_create
            .as_ref()
            .filter(|d| d.visit == visit && d.current(&shell) && !d.busy)
        else {
            return;
        };
        let Some(view) = d.review.as_ref().filter(|v| v.writer == writer && !v.busy) else {
            return;
        };
        if !view.inventory.attempts().iter().any(|a| {
            matches!(a.delivery(),domain::Delivery::Created{file_id} if file_id==id)
                && a.content().title() == title
        }) {
            return;
        }
        let Some(editor) = d.editor.clone() else {
            return;
        };
        (editor, view.clone(), shell.canvas_creation.clone())
    };
    editor.release_capture(&view.token);
    root.release(&editor);
    super::super::controller::close_exact(state, visit);
    crate::canvas::open_target(
        state,
        crate::state::CanvasTarget {
            id: id.into(),
            title: title.into(),
            permalink: None,
        },
    );
}
