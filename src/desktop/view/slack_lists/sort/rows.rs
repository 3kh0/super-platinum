use super::*;
#[component]
pub(super) fn Rules(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: sort::Presentation,
    editable: bool,
) -> Element {
    let mut add = use_signal(String::new);
    let structure = p.choice;
    let values = p.desired.rules();
    let unused = p
        .choices
        .iter()
        .filter(|(id, _)| {
            !values
                .iter()
                .any(|v| v.field().is_some_and(|(key, _)| key == id))
        })
        .cloned()
        .collect::<Vec<_>>();
    let add_value = add.read().clone();
    let mut addition = values.to_vec();
    let add_fits = sort::column_choice(&state.read(), &add_value, true).is_some_and(|v| {
        addition.push(v);
        fits(state, &ViewSortChoice::from_rules(addition))
    });
    let add_selected = unused.iter().any(|(id, _)| id == &add_value);
    let can_add = editable && add_fits && add_selected;
    let add_disabled = !editable || unused.is_empty();
    let rendered_add = p.desired.clone();
    let rendered_clear = p.desired.clone();
    let original = label(Some(&p.original), &p.choices);
    let current = label(p.current.as_ref(), &p.choices);
    rsx! {
        div{id:"slack-list-sort-set","data-sort-structure":"{structure}","data-sort-visit":"{visit}",
            p{class:"slack-list-editor-hint","Sort in this order. Later fields break ties in earlier fields."}
            {p.rows.iter().zip(values.iter()).enumerate().map(|(index,(id,value))|row_fields(state,serial,visit,*id,index,value.clone(),&p,editable))}
            if values.is_empty(){p{class:"slack-list-editor-hint","No sort"}}
            div{class:"slack-list-sort-add",
                label{r#for:"slack-list-sort-add-field","Add field"}
                select{id:"slack-list-sort-add-field",value:add_value.clone(),disabled:add_disabled,onchange:move|e|{if sort::editable(&state.peek(),serial,visit,structure){add.set(e.value());}},
                    option{value:"",selected:add_value.is_empty(),"Choose a field"}
                    for(id,name)in unused{option{value:id.clone(),selected:add_value==id,"{name}"}}
                }
                button{id:"slack-list-sort-add",r#type:"button",class:"slack-list-action",disabled:!can_add,onclick:move |_|focus::action(state,serial,visit,structure,rendered_add.clone(),sort::Action::Add(add.read().clone()),Some(add)),{icon(Icon::Add,"icon")}span{"Add sort"}}
                if add_selected && !add_fits {p{class:"slack-list-editor-hint","This field would exceed the sort request limit. Remove a sort before adding it."}}
                button{id:"slack-list-sort-clear",r#type:"button",class:"slack-list-action",disabled:!editable||values.is_empty(),onclick:move |_|focus::action(state,serial,visit,structure,rendered_clear.clone(),sort::Action::Clear,None),"Clear all"}
            }
        }
        p{id:"slack-list-sort-current",class:"slack-list-editor-hint","Original: {original}. Current: {current}."}
        p{class:"slack-list-editor-hint","Save changes this shared Table for everyone with access. Only currently loaded items can be ordered here; stored values are kept."}
    }
}
fn row_fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    id: u64,
    index: usize,
    value: ViewSortChoice,
    p: &sort::Presentation,
    editable: bool,
) -> Element {
    let Some((column, ascending)) = value.field() else {
        return rsx! {};
    };
    let column = column.to_owned();
    let structure = p.choice;
    let last = index + 1 == p.rows.len();
    let fields = p
        .choices
        .iter()
        .filter(|(key, _)| {
            key == &column
                || !p
                    .desired
                    .rules()
                    .iter()
                    .any(|v| v.field().is_some_and(|(id, _)| id == key))
        })
        .cloned()
        .collect::<Vec<_>>();
    let fields = fields
        .into_iter()
        .map(|(key, name)| {
            let mut next = p.desired.rules().to_vec();
            let allowed = sort::column_choice(&state.read(), &key, ascending).is_some_and(|v| {
                next[index] = v;
                fits(state, &ViewSortChoice::from_rules(next))
            });
            (key, name, allowed)
        })
        .collect::<Vec<_>>();
    let field_input = p.desired.clone();
    let direction_input = p.desired.clone();
    let up_input = p.desired.clone();
    let down_input = p.desired.clone();
    let remove_input = p.desired.clone();
    let direction = if ascending { "ascending" } else { "descending" };
    let direction_allowed = |ascending| {
        let mut next = p.desired.rules().to_vec();
        let Some(v) = sort::column_choice(&state.read(), &column, ascending) else {
            return false;
        };
        next[index] = v;
        fits(state, &ViewSortChoice::from_rules(next))
    };
    let ascending_allowed = direction_allowed(true);
    let descending_allowed = direction_allowed(false);
    rsx! {div{key:"{id}",class:"slack-list-sort-row","data-sort-row":"{id}",
        label{r#for:"sort-field-{id}","Sort {index+1} by"}
        select{id:"sort-field-{id}","data-sort-field":"true",value:column.clone(),disabled:!editable,onchange:move|e|focus::action(state,serial,visit,structure,field_input.clone(),sort::Action::Field(id,e.value()),None),for(key,name,allowed)in fields{option{value:key.clone(),selected:key==column,disabled:!allowed,"{name}"}}}
        label{r#for:"sort-direction-{id}","Direction"}
        select{id:"sort-direction-{id}","data-sort-direction":"true",value:direction,disabled:!editable,onchange:move|e|{if let Some(ascending)=parse_direction(&e.value()){focus::action(state,serial,visit,structure,direction_input.clone(),sort::Action::Direction(id,ascending),None);}},
            option{value:"ascending",selected:ascending,disabled:!ascending_allowed,"Ascending"}option{value:"descending",selected:!ascending,disabled:!descending_allowed,"Descending"}}
        div{class:"slack-list-sort-row-actions",
            button{id:"sort-up-{id}",r#type:"button",class:"slack-list-action",disabled:!editable||index==0,"aria-label":"Move sort {index+1} up",onclick:move |_|focus::action(state,serial,visit,structure,up_input.clone(),sort::Action::Move(id,true),None),"Move up"}
            button{id:"sort-down-{id}",r#type:"button",class:"slack-list-action",disabled:!editable||last,"aria-label":"Move sort {index+1} down",onclick:move |_|focus::action(state,serial,visit,structure,down_input.clone(),sort::Action::Move(id,false),None),"Move down"}
            button{id:"sort-remove-{id}",r#type:"button",class:"slack-list-action",disabled:!editable,"aria-label":"Remove sort {index+1}",onclick:move |_|focus::action(state,serial,visit,structure,remove_input.clone(),sort::Action::Remove(id),None),"Remove sort"}
        }
    }}
}
fn fits(state: Signal<ShellState>, desired: &ViewSortChoice) -> bool {
    sort::valid_choice(&state.read(), desired)
}
#[cfg(test)]
mod tests;
