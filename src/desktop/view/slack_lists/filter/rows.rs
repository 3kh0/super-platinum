use super::*;
#[component]
pub(super) fn Conditions(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: filter::Presentation,
    editable: bool,
) -> Element {
    let mut add = use_signal(String::new);
    let structure = p.choice;
    let values = p.desired.conditions();
    let unused = p
        .choices
        .iter()
        .filter(|(id, _)| !values.iter().any(|v| v.column_id() == Some(id.as_str())))
        .cloned()
        .collect::<Vec<_>>();
    let add_value = add.read().clone();
    let can_add = editable && values.len() < 8 && unused.iter().any(|(id, _)| id == &add_value);
    let add_disabled = !editable || values.len() >= 8 || unused.is_empty();
    let original = label(Some(&p.original), &p.choices);
    let current = label(p.current.as_ref(), &p.choices);
    rsx! {
    div{id:"slack-list-filter-set","data-filter-structure":"{structure}","data-filter-visit":"{visit}","data-filter-editable":editable,
    p{class:"slack-list-editor-hint","Match all conditions"}
    {p.rows.iter().zip(values.iter()).map(|(row,value)|
        row_fields(state,serial,visit,structure,row.clone(),value.clone(),&p,editable)
    )}
    div{class:"slack-list-filter-add",
    label{r#for:"slack-list-filter-add-field","Add field"}
    select{id:"slack-list-filter-add-field",value:add_value.clone(),disabled:add_disabled,onchange:move|e|{if filter::editable(&state.peek(),serial,visit,structure){add.set(e.value());}},
    option{value:"",selected:add_value.is_empty(),"Choose a field"}
    for (id,name) in unused {option{value:id.clone(),selected:add_value==id,"{name}"}}
    }
    button{id:"slack-list-filter-add",r#type:"button",class:"slack-list-action",disabled:!can_add,onclick:move |_|focus::action(state,serial,visit,structure,filter::Action::Add(add.read().clone()),Some(add)),{icon(Icon::Add,"icon")}span{"Add condition"}}
    button{id:"slack-list-filter-clear",r#type:"button",class:"slack-list-action",disabled:!editable||values.is_empty(),onclick:move |_|focus::action(state,serial,visit,structure,filter::Action::Clear,None),"Clear all"}
    }
    }
    p{id:"slack-list-filter-current",class:"slack-list-editor-hint","Original: {original}. Current: {current}."}
    p{class:"slack-list-editor-hint","Save changes this shared Table for everyone with access. All conditions must match. Only currently loaded items can be filtered here; stored values are kept."}
    }
}
fn row_fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    structure: u64,
    row: filter::Row,
    value: ViewFilterChoice,
    p: &filter::Presentation,
    editable: bool,
) -> Element {
    let id = row.id;
    let epoch = row.epoch;
    let column = value.column_id().unwrap_or_default().to_owned();
    let fields = p
        .choices
        .iter()
        .filter(|(key, _)| {
            key == &column
                || !p
                    .desired
                    .conditions()
                    .iter()
                    .any(|v| v.column_id() == Some(key.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    let (op, number, initial) = match &value {
        ViewFilterChoice::Checkbox { checked, .. } => (
            if *checked {
                Op::IsNotEmpty
            } else {
                Op::IsEmpty
            },
            false,
            String::new(),
        ),
        ViewFilterChoice::Number {
            operator, value, ..
        } => (*operator, true, value.clone().unwrap_or_default()),
        _ => return rsx! {},
    };
    let ops = if number {
        vec![
            Op::Equal,
            Op::NotEqual,
            Op::LessThan,
            Op::GreaterThan,
            Op::IsEmpty,
            Op::IsNotEmpty,
        ]
    } else {
        vec![Op::IsEmpty, Op::IsNotEmpty]
    };
    let invalid = number
        && !op.valid_value(if op.requires_value() {
            Some(&initial)
        } else {
            None
        });
    rsx! {
    div{key:"{id}",class:"slack-list-filter-row","data-filter-row":"{id}","data-filter-epoch":"{epoch}",
    label{r#for:"filter-field-{id}","Field"}
    select{id:"filter-field-{id}","data-filter-field":"true",value:column.clone(),disabled:!editable,onchange:move|e|focus::action(state,serial,visit,structure,filter::Action::Field(id,epoch,e.value()),None),for(key,name)in fields {option{value:key.clone(),selected:key==column,"{name}"}}}
    label{r#for:"filter-operator-{id}","Condition"}
    select{id:"filter-operator-{id}","data-filter-operator":"true",value:op.as_str(),disabled:!editable,onchange:move|e|{if let Some(op)=[Op::Equal,Op::NotEqual,Op::LessThan,Op::GreaterThan,Op::IsEmpty,Op::IsNotEmpty].into_iter().find(|op|op.as_str()==e.value()){focus::action(state,serial,visit,structure,filter::Action::Operator(id,epoch,op),None);}},
    for choice in ops {option{value:choice.as_str(),selected:choice==op,if number{"{number::label(choice)}"}else if choice==Op::IsEmpty{"is not checked"}else{"is checked"}}}}
    if number&&op.requires_value(){for version in [epoch]{
    label{key:"{id}:{version}",r#for:"filter-value-{id}","Value"}
    input{id:"filter-value-{id}","data-filter-value":"true",r#type:"number",step:"any",initial_value:initial.clone(),disabled:!editable,"aria-invalid":invalid,oninput:move|e|{if filter::update_value(&state.peek(),serial,visit,id,epoch,e.value()){let mut state=state;state.write();}}}
    }}
    button{id:"filter-remove-{id}","data-filter-remove":"true",r#type:"button",class:"slack-list-action",disabled:!editable,onclick:move |_|focus::action(state,serial,visit,structure,filter::Action::Remove(id),None),"Remove condition"}
    }
    }
}

#[cfg(test)]
mod tests;
