use super::*;
const OPS: [Op; 6] = [
    Op::Equal,
    Op::NotEqual,
    Op::LessThan,
    Op::GreaterThan,
    Op::IsEmpty,
    Op::IsNotEmpty,
];
pub(super) fn label(op: Op) -> &'static str {
    match op {
        Op::Equal => "is equal to",
        Op::NotEqual => "is not equal to",
        Op::LessThan => "is less than",
        Op::GreaterThan => "is greater than",
        Op::IsEmpty => "is empty",
        Op::IsNotEmpty => "is not empty",
    }
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: &filter::Presentation,
    editable: bool,
) -> Element {
    let ViewFilterChoice::Number {
        column_id,
        operator,
        value,
    } = &p.desired
    else {
        return rsx! {};
    };
    let choice = p.choice;
    let rendered = p.desired.clone();
    let value_rendered = rendered.clone();
    let initial = value.clone().unwrap_or_default();
    let current = operator.as_str();
    let needs = operator.requires_value();
    let invalid = !operator.valid_value(value.as_deref());
    rsx! {
        label{r#for:"slack-list-filter-operator","Comparison"}
        select{id:"slack-list-filter-operator",value:current,disabled:!editable,"data-filter-choice":"{choice}",
            onchange:move |e|{if let Some(op)=OPS.into_iter().find(|op|op.as_str()==e.value()){
                filter::number_operator(state,serial,visit,choice,rendered.clone(),op);
            }},for op in OPS {option{value:op.as_str(),selected:op.as_str()==current,"{label(op)}"}}}
        if needs {
            for epoch in [choice] {
            label{key:"{serial}:{epoch}",r#for:"slack-list-filter-number","Value"}
            // Uncontrolled native value keeps incomplete exponent/decimal input
            // through same-epoch renders. A field/operator change remounts it.
            input{id:"slack-list-filter-number",r#type:"number",step:"any",initial_value:initial.clone(),
                disabled:!editable,"data-filter-choice":"{choice}","data-filter-column":"{column_id}","data-filter-operator":current,
                "aria-describedby":"slack-list-filter-number-hint","aria-invalid":invalid,
                oninput:{let draft=value_rendered.clone();move |e|filter::number_value(state,serial,visit,choice,draft.clone(),e.value())}}
            p{id:"slack-list-filter-number-hint",class:"slack-list-editor-hint","Enter a finite number. Decimal and exponent values are supported; leave this value empty only by choosing an empty condition."}
            }
        }
    }
}
pub(super) fn confirm(state: Signal<ShellState>, serial: u64, visit: u64, p: filter::Presentation) {
    let ViewFilterChoice::Number {
        column_id,
        operator,
        ..
    } = &p.desired
    else {
        return;
    };
    let choices = super::options(&p.choices, &p.numbers);
    let Some(field) = choices
        .iter()
        .position(|c| super::same_option(c, &p.desired))
        .map(|i| i.to_string())
    else {
        return;
    };
    let args = serde_json::json!({"serial":serial.to_string(),"choice":p.choice.to_string(),"column":column_id,"operator":operator.as_str(),"field":field});
    spawn(async move {
        let mut eval = dioxus::document::eval(&format!(
            "{};\nconst owned=numberFilterSnapshot({args});dioxus.send(owned?.snapshot??null);const restore=await dioxus.recv();if(restore)owned?.restore();",
            include_str!("number.js")
        ));
        let snapshot = eval.recv::<Option<(String, bool)>>().await.ok().flatten();
        if let Some((value, valid)) = snapshot {
            filter::confirm_number(state, serial, visit, p.choice, p.desired, value, valid);
        }
        let restore = rename::presentation(&state.peek()).is_some_and(|v| {
            v.serial == serial
                && v.stage == rename::Stage::Editing
                && v.filter.is_some_and(|f| f.choice == p.choice)
        });
        let _ = eval.send(restore);
    });
}

#[cfg(test)]
mod tests;
