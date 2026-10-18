use super::*;
use crate::slack_lists::rename::{self, filter};
use super_platinum_core::slack_lists::{NumberFilterOperator as Op, ViewFilterChoice};
mod capture;
mod date;
mod focus;
mod number;
mod rows;
mod select;

pub(super) fn action(state: Signal<ShellState>, visit: u64) -> Element {
    let selection = rename::selection(&state.read()).unwrap_or_default();
    let disabled = !filter::can_open(&state.read());
    rsx! {button{id:"slack-list-filter",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,title:"Save conditions for this shared Table",onclick:move |_|filter::open(state,visit,selection),{icon(Icon::Filter,"icon")}span{"Filter"}}}
}
fn name<'a>(id: &str, choices: &'a [(String, String)]) -> &'a str {
    choices
        .iter()
        .find(|(key, _)| key == id)
        .map(|(_, name)| name.as_str())
        .unwrap_or("Field")
}
fn label(choice: Option<&ViewFilterChoice>, choices: &[(String, String)]) -> String {
    match choice {
        None => "Unavailable".into(),
        Some(ViewFilterChoice::All(rows)) => rows
            .iter()
            .map(|r| label(Some(r), choices))
            .collect::<Vec<_>>()
            .join(" AND "),
        Some(ViewFilterChoice::None) => "No filter".into(),
        Some(ViewFilterChoice::Checkbox { column_id, checked }) => format!(
            "{} {}",
            name(column_id, choices),
            if *checked {
                "is checked"
            } else {
                "is not checked"
            }
        ),
        Some(ViewFilterChoice::Select {
            column_id,
            operator,
            values,
        }) => format!(
            "{} {}{}",
            name(column_id, choices),
            select::label(*operator),
            if operator.requires_values() {
                format!(" ({} options)", values.len())
            } else {
                String::new()
            }
        ),
        Some(ViewFilterChoice::Date {
            column_id,
            operator,
            value,
        }) => format!(
            "{} {}{}",
            name(column_id, choices),
            date::label(*operator),
            value
                .as_deref()
                .map(|v| format!(" {}", date::value_label(v)))
                .unwrap_or_default()
        ),
        Some(ViewFilterChoice::Number {
            column_id,
            operator,
            value,
        }) => format!(
            "{} {}{}",
            name(column_id, choices),
            number::label(*operator),
            value.as_ref().map(|v| format!(" {v}")).unwrap_or_default()
        ),
    }
}
fn options(choices: &[(String, String)], numbers: &[String]) -> Vec<ViewFilterChoice> {
    let mut out = vec![ViewFilterChoice::None];
    for (id, _) in choices {
        if numbers.contains(id) {
            out.push(ViewFilterChoice::Number {
                column_id: id.clone(),
                operator: Op::Equal,
                value: None,
            });
        } else {
            out.extend([true, false].map(|checked| ViewFilterChoice::Checkbox {
                column_id: id.clone(),
                checked,
            }));
        }
    }
    out
}
fn same_option(a: &ViewFilterChoice, b: &ViewFilterChoice) -> bool {
    match (a, b) {
        (
            ViewFilterChoice::Number { column_id: a, .. },
            ViewFilterChoice::Number { column_id: b, .. },
        ) => a == b,
        _ => a == b,
    }
}
fn selected(value: &str, choices: &[ViewFilterChoice]) -> Option<ViewFilterChoice> {
    let index: usize = value.parse().ok()?;
    if index.to_string() != value {
        return None;
    }
    choices.get(index).cloned()
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: filter::Presentation,
    editable: bool,
) -> Element {
    rsx! {rows::Conditions{state,serial,visit,p,editable}}
}
pub(super) fn confirm(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: Option<filter::Presentation>,
) {
    let Some(p) = p else {
        rename::confirm(state, serial);
        return;
    };
    capture::confirm(state, serial, visit, p.choice);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_condition_values_keep_field_and_operator_distinct() {
        let choices = options(
            &[
                ("C/one:true".into(), "Same name".into()),
                ("C/two".into(), "Same name".into()),
            ],
            &[],
        );
        assert_eq!(choices.len(), 5);
        for (i, value) in choices.iter().enumerate() {
            assert_eq!(selected(&i.to_string(), &choices), Some(value.clone()));
        }
        assert_eq!(
            label(Some(&choices[1]), &[("C/one:true".into(), "Done".into())]),
            "Done is checked"
        );
        assert_eq!(
            label(Some(&choices[2]), &[("C/one:true".into(), "Done".into())]),
            "Done is not checked"
        );
        for value in [
            "",
            "01",
            "+1",
            "-1",
            "1 ",
            "true",
            "5",
            "999999999999999999999999999999",
        ] {
            assert_eq!(selected(value, &choices), None);
        }
    }

    #[test]
    fn number_field_option_is_stable_across_all_operators_and_values() {
        let fields = vec![
            ("CHECK".into(), "Same".into()),
            ("NUMBER".into(), "Same".into()),
        ];
        let choices = options(&fields, &["NUMBER".into()]);
        assert_eq!(choices.len(), 4);
        assert!(matches!(
            &choices[1],
            ViewFilterChoice::Checkbox { checked: true, .. }
        ));
        for op in [
            Op::Equal,
            Op::NotEqual,
            Op::LessThan,
            Op::GreaterThan,
            Op::IsEmpty,
            Op::IsNotEmpty,
        ] {
            for v in [None, Some(""), Some("1e"), Some("12.50")] {
                let desired = ViewFilterChoice::Number {
                    column_id: "NUMBER".into(),
                    operator: op,
                    value: v.map(str::to_owned),
                };
                assert_eq!(
                    choices.iter().position(|c| same_option(c, &desired)),
                    Some(3)
                );
            }
        }
        assert_eq!(
            selected("3", &choices),
            Some(ViewFilterChoice::Number {
                column_id: "NUMBER".into(),
                operator: Op::Equal,
                value: None
            })
        );
    }
}

#[cfg(test)]
mod route_tests;
