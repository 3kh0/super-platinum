//! Exact native range custody; local review is separate from sending either write.
use super::*;
use crate::canvas::checklist;
use serde::Deserialize;
use serde_json::json;

const SCRIPT: &str = include_str!("range.js");
const HIDDEN_ROUTING: &str = include_str!("hidden_routing.js");
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    token: String,
    section: String,
    visit: String,
    read: String,
    text: String,
    start: usize,
    end: usize,
    annotations: Vec<ControlRange>,
    #[serde(rename = "selectedControl")]
    selected_control: Option<String>,
    removed: Vec<String>,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlRange {
    control: String,
    start: usize,
    end: usize,
    archived: bool,
    hidden: bool,
}
fn displayed_controls(
    seed: &super_platinum_core::canvas::edit::annotation::AnnotationParagraph,
) -> Vec<ControlRange> {
    let mut out: Vec<ControlRange> = Vec::new();
    let mut offset = 0;
    for run in &seed.runs {
        let end = offset + run.text.encode_utf16().count();
        if let Some(control) = &run.control {
            if let Some(last) = out.last_mut().filter(|last| {
                last.control == *control
                    && last.end == offset
                    && last.archived == seed.archived.contains(control)
                    && last.hidden == seed.hidden.contains(control)
            }) {
                last.end = end;
            } else {
                out.push(ControlRange {
                    control: control.clone(),
                    start: offset,
                    end,
                    archived: seed.archived.contains(control),
                    hidden: seed.hidden.contains(control),
                });
            }
        }
        offset = end;
    }
    out
}
#[derive(Clone)]
pub(super) struct Selected {
    pub section: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub removed: Option<Vec<String>>,
}
pub(in crate::canvas) fn choose(
    mut state: Signal<ShellState>,
    reader: Signal<crate::canvas::Reader>,
    model: Signal<super::super::State>,
    mut opened: Signal<bool>,
) {
    // The body-selection control is available only with the thread panel closed.
    // No active-reaction editor exists on this surface; ordinary context is None/None.
    if *opened.peek() {
        return;
    }
    let Some(view) = checklist::presentation(&state.peek())
        .filter(|v| !v.loading && !v.stale && v.operation.is_none() && v.capacity_reason.is_none())
    else {
        return;
    };
    let Some(snapshot) = view.snapshot else {
        return;
    };
    if reader.peek().loading || reader.peek().text.owner != Some((view.visit, view.read)) {
        return;
    }
    let Some(comments_owner) = super::super::Owner::capture(&state.peek()) else {
        return;
    };
    let token = uuid::Uuid::new_v4().to_string();
    let options =
        json!({"token":token,"visit":view.visit.to_string(),"read":view.read.to_string()});
    dioxus::core::spawn_forever(async move {
        let mut new_ticket: Option<Ticket> = None;
        let result: Result<(Capture, Option<String>), ()> =
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                let mut eval = dioxus::document::eval(&format!(
                    "{HIDDEN_ROUTING}\n{SCRIPT}\nreturn await captureCanvasAnnotationRange({options},dioxus);"
                ));
                let captured = eval.recv::<Capture>().await.map_err(|_| ())?;
                if captured.token != token
                    || captured.visit != view.visit.to_string()
                    || captured.read != view.read.to_string()
                {
                    return Err(());
                }
                if *opened.peek() || !comments_owner.current(&state.peek()) {
                    return Err(());
                }
                let valid = {
                    let shell = state.peek();
                    let r = reader.peek();
                    comments_owner.current(&shell)
                        && !r.loading
                        && r.text.owner == Some((view.visit, view.read))
                        && r.text
                            .items
                            .get(&captured.section)
                            .and_then(|item| item.annotation.as_ref())
                            .is_some_and(|seed| {
                                seed.runs
                                    .iter()
                                    .map(|r| r.text.as_str())
                                    .collect::<String>()
                                    == captured.text
                                    && displayed_controls(seed) == captured.annotations
                            })
                        && checklist::presentation(&shell).is_some_and(|v| {
                            v.visit == view.visit
                                && v.read == view.read
                                && !v.loading
                                && !v.stale
                                && v.operation.is_none()
                        })
                };
                if !valid {
                    return Err(());
                }
                let existing = {
                    let r = reader.peek();
                    let seed = r
                        .text
                        .items
                        .get(&captured.section)
                        .and_then(|item| item.annotation.as_ref())
                        .ok_or(())?;
                    seed.existing_native_control(
                        captured.start,
                        captured.end,
                        captured.selected_control.as_deref(),
                    )
                    .map_err(|_| ())?
                    .map(str::to_owned)
                };
                let archived = reader
                    .peek()
                    .text
                    .items
                    .get(&captured.section)
                    .and_then(|item| item.annotation.as_ref())
                    .is_some_and(|seed| !seed.hidden.is_empty());
                if existing.is_some() && !captured.removed.is_empty() {
                    return Err(());
                }
                let removed = archived.then(|| captured.removed.clone());
                if !archived && !captured.removed.is_empty() {
                    return Err(());
                }
                if existing.is_none()
                    && capture_intent(
                        &snapshot,
                        &captured.section,
                        captured.start,
                        captured.end,
                        removed.as_deref(),
                    )
                    .is_err()
                {
                    return Err(());
                }
                // Retain before the JS bridge releases its original node/range.
                if existing.is_none() {
                    let ticket = bind(state).ok_or(())?;
                    {
                        let shell = state.peek();
                        let e = entry(&shell, &ticket.id).ok_or(())?;
                        if e.pending.is_some()
                            || e.acknowledged.is_some()
                            || e.verified.is_some()
                            || e.accepted.is_some()
                        {
                            return Err(());
                        }
                    }
                    if !start(state, &ticket) {
                        return Err(());
                    }
                    new_ticket = Some(ticket.clone());
                    let mut shell = state.write();
                    if !ticket.current(&shell) {
                        return Err(());
                    }
                    let e = entry_mut(&mut shell, &ticket.id).ok_or(())?;
                    e.snapshot = Some(snapshot.clone());
                    e.selected = captured.section.clone();
                    e.selected_image = None;
                    e.selected_range = Some(Selected {
                        section: captured.section.clone(),
                        start: captured.start,
                        end: captured.end,
                        removed,
                        text: String::from_utf16(
                            &captured
                                .text
                                .encode_utf16()
                                .skip(captured.start)
                                .take(captured.end - captured.start)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|_| ())?,
                    });
                }
                eval.send(json!({"token":token,"accepted":true}))
                    .map_err(|_| ())?;
                let confirmed = eval.join::<Capture>().await.map_err(|_| ())?;
                if confirmed != captured {
                    return Err(());
                }
                Ok((captured, existing))
            })
            .await
            .unwrap_or(Err(()));
        let current = !*opened.peek()
            && comments_owner.current(&state.peek())
            && new_ticket
                .as_ref()
                .is_none_or(|ticket| ticket.current(&state.peek()))
            && checklist::presentation(&state.peek()).is_some_and(|v| {
                v.visit == view.visit
                    && v.read == view.read
                    && !v.loading
                    && !v.stale
                    && v.operation.is_none()
            });
        if result.is_err() || !current {
            if let Some(ticket) = &new_ticket {
                if let Some(e) = entry_mut(&mut state.write(), &ticket.id) {
                    e.selected_range = None;
                    e.selected.clear();
                }
                issue(
                    state,
                    ticket,
                    "Select text in one current paragraph, then choose Comment on selection. Nothing was sent.",
                );
            }
            if current {
                state
                    .write()
                    .show_toast("Select text in one current paragraph. Nothing was sent.");
            }
            return;
        }
        if let Some(ticket) = &new_ticket {
            if let Some(e) = entry_mut(&mut state.write(), &ticket.id) {
                e.busy = false;
            }
        }
        opened.set(true);
        if let Ok((_, Some(control))) = result {
            super::super::selection::open(state, model, comments_owner, control);
        } else {
            super::super::open(state, model);
        }
    });
}

pub(super) fn capture_intent(
    snapshot: &CanvasCollabSnapshot,
    section: &str,
    start: usize,
    end: usize,
    removed: Option<&[String]>,
) -> Result<AnnotationIntent, super_platinum_core::canvas::edit::Error> {
    let context = super_platinum_core::canvas::edit::annotation::AnnotationPresentation::ordinary(
        None, None,
    )?;
    AnnotationIntent::capture_range_with_presentation(
        snapshot,
        section,
        start,
        end,
        removed.unwrap_or_default().to_vec(),
        &context,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{
        annotation::{AnnotationParagraph, AnnotationRun},
        text::{TextDraft, TextStyle},
    };
    #[test]
    fn displayed_control_receipt_merges_only_adjacent_same_identity_in_utf16() {
        let seed = AnnotationParagraph {
            draft: TextDraft::default(),
            archived: Vec::new(),
            hidden: Vec::new(),
            runs: vec![
                AnnotationRun {
                    text: "A".into(),
                    style: TextStyle::default(),
                    control: None,
                },
                AnnotationRun {
                    text: "\u{1f600}".into(),
                    style: TextStyle::default(),
                    control: Some("old".into()),
                },
                AnnotationRun {
                    text: "B".into(),
                    style: TextStyle {
                        bold: true,
                        ..Default::default()
                    },
                    control: Some("old".into()),
                },
                AnnotationRun {
                    text: "C".into(),
                    style: TextStyle::default(),
                    control: Some("other".into()),
                },
            ],
        };
        let actual = displayed_controls(&seed);
        assert!(
            actual
                == vec![
                    ControlRange {
                        control: "old".into(),
                        start: 1,
                        end: 4,
                        archived: false,
                        hidden: false,
                    },
                    ControlRange {
                        control: "other".into(),
                        start: 4,
                        end: 5,
                        archived: false,
                        hidden: false,
                    }
                ]
        );
    }
}
