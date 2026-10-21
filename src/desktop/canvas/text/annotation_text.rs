//! Preserve one original DOM control wrapper across formatting fragments.
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::annotation::{AnnotationParagraph, AnnotationRun};

struct Group<'a> {
    control: Option<&'a str>,
    runs: Vec<&'a AnnotationRun>,
}
fn groups(seed: &AnnotationParagraph) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    for run in &seed.runs {
        if let Some(control) = run.control.as_deref()
            && let Some(last) = groups.last_mut().filter(|g| g.control == Some(control))
        {
            last.runs.push(run);
        } else {
            // Never wrap adjacent ordinary runs: doing so would change the
            // native common ancestor used by hidden annotation removal.
            groups.push(Group {
                control: run.control.as_deref(),
                runs: vec![run],
            });
        }
    }
    groups
}
fn styled(run: &AnnotationRun, epoch: u64, state: Signal<ShellState>) -> Element {
    rsx! { span {
        class: format!("{}{}{}{}{}", if run.style.bold {" bold"} else {""}, if run.style.italic {" italic"} else {""}, if run.style.strike {" strike"} else {""}, if run.style.code {" inline-code"} else {""}, if run.style.underline {" canvas-text-underline"} else {""}),
        if let Some(url) = &run.style.link {
            {crate::view::rich::rich_node(&crate::model::RichNode::Link {label:run.text.clone(),url:url.clone()},epoch,state)}
        } else { "{run.text}" }
    }}
}
pub(super) fn render(seed: &AnnotationParagraph, epoch: u64, state: Signal<ShellState>) -> Element {
    rsx! {
        for group in groups(seed) {
            if let Some(control) = group.control {
                span {
                    class: if seed.hidden.iter().any(|id| id == control) {"canvas-archived-comment"} else {"canvas-existing-comment"},
                    "data-canvas-comment-control": control.to_owned(),
                    "data-canvas-comment-hidden": if seed.hidden.iter().any(|id| id == control) {"true"} else {"false"},
                    "data-canvas-comment-archived": if seed.archived.iter().any(|id| id == control) {"true"} else {"false"},
                    title: (!seed.archived.iter().any(|id| id == control)).then_some("Existing comment"),
                    for run in group.runs { {styled(run,epoch,state)} }
                }
            } else {
                for run in group.runs { {styled(run,epoch,state)} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::text::{TextDraft, TextStyle};
    #[test]
    fn formatted_control_has_one_outer_node_but_plain_runs_are_not_grouped() {
        let seed = AnnotationParagraph {
            draft: TextDraft::default(),
            archived: vec!["old".into()],
            hidden: vec!["old".into()],
            runs: [
                ("A", None, false),
                ("B", None, true),
                ("C", Some("old"), false),
                ("D", Some("old"), true),
                ("E", None, false),
            ]
            .into_iter()
            .map(|(text, control, bold)| AnnotationRun {
                text: text.into(),
                control: control.map(str::to_owned),
                style: TextStyle {
                    bold,
                    underline: control.is_some(),
                    ..Default::default()
                },
            })
            .collect(),
        };
        let groups = groups(&seed);
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[2].control, Some("old"));
        assert_eq!(groups[2].runs.len(), 2);
        assert_eq!(
            groups
                .iter()
                .flat_map(|g| &g.runs)
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "ABCDE"
        );
        assert!(groups[2].runs[1].style.bold);
        assert!(groups[2].runs.iter().all(|run| run.style.underline));
    }
}
