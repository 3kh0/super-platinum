use super::*;
use super_platinum_core::slack_lists::DateFilterOperator as DateOp;
const OPS: [DateOp; 6] = [
    DateOp::Equal,
    DateOp::NotEqual,
    DateOp::LessThan,
    DateOp::GreaterThan,
    DateOp::IsEmpty,
    DateOp::IsNotEmpty,
];
const PRESETS: [&str; 6] = [
    "today",
    "yesterday",
    "tomorrow",
    "this_week",
    "next_week",
    "last_week",
];
pub(super) fn label(op: DateOp) -> &'static str {
    match op {
        DateOp::Equal => "is",
        DateOp::NotEqual => "is not",
        DateOp::LessThan => "is before",
        DateOp::GreaterThan => "is after",
        DateOp::IsEmpty => "is empty",
        DateOp::IsNotEmpty => "is not empty",
    }
}
pub(super) fn value_label(value: &str) -> &str {
    match value {
        "today" => "Today",
        "yesterday" => "Yesterday",
        "tomorrow" => "Tomorrow",
        "this_week" => "This week",
        "next_week" => "Next week",
        "last_week" => "Last week",
        _ => value,
    }
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    structure: u64,
    id: u64,
    epoch: u64,
    op: DateOp,
    value: Option<String>,
    editable: bool,
) -> Element {
    let kind = value
        .as_deref()
        .filter(|v| DateOp::is_preset(v))
        .unwrap_or("custom")
        .to_owned();
    let custom = kind == "custom";
    let invalid = !op.valid_value(value.as_deref());
    let initial = value.unwrap_or_default();
    rsx! {
        label{r#for:"filter-operator-{id}","Condition"}
        select{id:"filter-operator-{id}","data-filter-operator":"true",value:op.as_str(),disabled:!editable,onchange:move|e|{if let Some(next)=OPS.into_iter().find(|v|v.as_str()==e.value()){focus::action(state,serial,visit,structure,filter::Action::DateOperator(id,epoch,next),None);}},
            for choice in OPS {option{value:choice.as_str(),selected:choice==op,"{label(choice)}"}}
        }
        if op.requires_value(){
            label{r#for:"filter-date-kind-{id}","Date"}
            select{id:"filter-date-kind-{id}","data-filter-date-kind":"true",value:kind.clone(),disabled:!editable,onchange:move|e|focus::action(state,serial,visit,structure,filter::Action::DateValue(id,epoch,e.value()),None),
                for preset in PRESETS {option{value:preset,selected:kind==preset,"{value_label(preset)}"}}
                option{value:"custom",selected:custom,"Custom date"}
            }
            if custom {for version in [epoch]{
                label{key:"date-{id}:{version}",r#for:"filter-date-value-{id}","Calendar date"}
                input{id:"filter-date-value-{id}","data-filter-date-value":"true",r#type:"date",min:"0001-01-01",max:"9999-12-31",initial_value:initial.clone(),disabled:!editable,"aria-invalid":invalid,oninput:move|e|{if filter::update_value(&state.peek(),serial,visit,id,epoch,e.value()){let mut state=state;state.write();}}}
            }}
        }
    }
}
