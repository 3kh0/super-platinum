use super::*;
use crate::slack_lists::rename::sort;
use super_platinum_core::slack_lists::ViewSortChoice;
pub(super) fn action(state: Signal<ShellState>, visit: u64) -> Element {
    let selection = crate::slack_lists::rename::selection(&state.read()).unwrap_or_default();
    let disabled = !sort::can_open(&state.read());
    rsx! {button{id:"slack-list-sort",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,title:"Save ordered field sorts for this shared Table",onclick:move |_|sort::open(state,visit,selection),{icon(Icon::Sort,"icon")}span{"Sort"}}}
}
fn label(choice: Option<&ViewSortChoice>, choices: &[(String, String)]) -> String {
    match choice {
        None => "Unavailable".into(),
        Some(ViewSortChoice::None) => "No sort".into(),
        Some(ViewSortChoice::Ordered(rows)) => rows
            .iter()
            .map(|r| label(Some(r), choices))
            .collect::<Vec<_>>()
            .join("; then "),
        Some(
            ViewSortChoice::Number {
                column_id,
                ascending,
            }
            | ViewSortChoice::Field {
                column_id,
                ascending,
            },
        ) => format!(
            "{}: {}",
            choices
                .iter()
                .find(|(id, _)| id == column_id)
                .map(|(_, label)| label.as_str())
                .unwrap_or("Field"),
            if *ascending {
                "Ascending"
            } else {
                "Descending"
            }
        ),
    }
}
mod focus;
mod rows;
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: sort::Presentation,
    editable: bool,
) -> Element {
    rsx! { rows::Rules{state,serial,visit,p,editable} }
}
fn parse_direction(value: &str) -> Option<bool> {
    match value {
        "ascending" => Some(true),
        "descending" => Some(false),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direction_values_are_exact() {
        assert_eq!(parse_direction("ascending"), Some(true));
        assert_eq!(parse_direction("descending"), Some(false));
        for value in ["true", "false", "Ascending", "descending ", ""] {
            assert_eq!(parse_direction(value), None);
        }
    }
    #[test]
    fn field_and_number_labels_share_exact_direction_and_missing_fallback() {
        let choices = vec![("C".into(), "People".into())];
        for choice in [
            ViewSortChoice::Field {
                column_id: "C".into(),
                ascending: false,
            },
            ViewSortChoice::Number {
                column_id: "C".into(),
                ascending: false,
            },
        ] {
            assert_eq!(label(Some(&choice), &choices), "People: Descending");
        }
        assert_eq!(
            label(
                Some(&ViewSortChoice::Field {
                    column_id: "missing".into(),
                    ascending: true
                }),
                &choices
            ),
            "Field: Ascending"
        );
        assert_eq!(label(Some(&ViewSortChoice::None), &choices), "No sort");
        assert_eq!(label(None, &choices), "Unavailable");
    }
}
