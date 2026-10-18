use super::*;
#[component]
pub(super) fn Field(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    field: controller::SectionField,
    value: Option<SectionValue>,
    locked: bool,
    error: Option<String>,
    onrich: EventHandler<String>,
) -> Element {
    let id = field.id.clone();
    let input_id = format!("profile-section-field-{id}");
    let disabled = locked || !field.editable;
    let mut add = use_signal(String::new);
    rsx! {div{class:"profile-edit-field",
        label{r#for:input_id.clone(),"{field.label}"}
        match value {
            Some(SectionValue::Text(value)) if !field.options.is_empty()=>rsx!{
                select{id:input_id.clone(),value,disabled,"data-dialog-input":"true",onchange:move |e|controller::change(state,visit,serial,id.clone(),SectionValue::Text(e.value())),
                    option{value:"","Not set"}
                    for option in field.options {option{value:option.clone(),"{option}"}}
                }
            },
            Some(SectionValue::Text(value))=>rsx!{input{id:input_id.clone(),r#type:"text",value,disabled,"data-dialog-input":"true",oninput:move |e|controller::change(state,visit,serial,id.clone(),SectionValue::Text(e.value()))}},
            Some(SectionValue::Date(value))=>rsx!{input{id:input_id.clone(),r#type:"date",value,disabled,"data-dialog-input":"true",oninput:move |e|controller::change(state,visit,serial,id.clone(),SectionValue::Date(e.value()))}},
            Some(SectionValue::Link{display,uri})=>rsx!{
                input{id:input_id.clone(),r#type:"text",value:display.clone(),disabled,"aria-label":"Display text for {field.label}","data-dialog-input":"true",oninput:{let id=id.clone();let uri=uri.clone();move |e|controller::change(state,visit,serial,id.clone(),SectionValue::Link{display:e.value(),uri:uri.clone()})}}
                input{r#type:"url",value:uri,disabled,"aria-label":"URL for {field.label}","data-dialog-input":"true",oninput:move |e|controller::change(state,visit,serial,id.clone(),SectionValue::Link{display:display.clone(),uri:e.value()})}
            },
            Some(SectionValue::Tags(tags))=>rsx!{
                div{class:"profile-section-tags",
                    for (index,tag) in tags.iter().enumerate() {
                        button{r#type:"button",disabled,"aria-label":"Remove {tag}",onclick:{let tags=tags.clone();let id=id.clone();move |_|{let mut next=tags.clone();next.remove(index);controller::change(state,visit,serial,id.clone(),SectionValue::Tags(next));}},"{tag}",{icon(Icon::Close,"icon")}}
                    }
                }
                input{id:input_id.clone(),value:add(),disabled,"aria-label":"Add a tag for {field.label}","data-dialog-input":"true",oninput:{let id=id.clone();move |e|{add.set(e.value());controller::query_tags(state,visit,serial,id.clone(),e.value());}}}
                for suggestion in controller::tags(&state.read(),&id,&add()).into_iter().filter(|t|!tags.contains(t)).take(30) {
                    button{r#type:"button",disabled:disabled||tags.len()>=75,onclick:{let id=id.clone();let tags=tags.clone();let choice=suggestion.clone();move |_|{let mut next=tags.clone();next.push(choice.clone());controller::change(state,visit,serial,id.clone(),SectionValue::Tags(next));add.set(String::new());}},"{suggestion}"}
                }
                button{r#type:"button",disabled:disabled||super_platinum_core::slack::api::custom::trim_tag_query(&add.read()).is_empty()||tags.len()>=75,onclick:move |_|{
                    let text=super_platinum_core::slack::api::custom::trim_tag_query(&add.read()).to_owned();if !text.is_empty()&&!tags.contains(&text){let mut next=tags.clone();next.push(text);controller::change(state,visit,serial,id.clone(),SectionValue::Tags(next));add.set(String::new());}
                },"Add tag"}
            },
            Some(SectionValue::Location{city,country,description,state:region})=>rsx!{
                Location{state,visit,serial,id,disabled,city,country,description,region}
            },
            Some(SectionValue::Persons(ids))=>rsx!{
                Persons{state,visit,serial,id,disabled,ids}
            },
            Some(SectionValue::LongText(_))=>rsx!{button{id:input_id.clone(),r#type:"button",disabled,onclick:move |_|onrich.call(id.clone()),"Edit description"}},
            None=>rsx!{p{class:"profile-edit-hint","This field is preserved and uses a different editor."}},
        }
        if !field.editable {p{class:"profile-edit-hint","This field is managed by your organization or is not editable here."}}
        if !field.hint.is_empty(){p{class:"profile-edit-hint","{field.hint}"}}
        if let Some(error)=error{p{class:"profile-edit-field-error",role:"alert","{error}"}}
    }}
}
#[component]
fn Location(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
    disabled: bool,
    city: String,
    country: String,
    description: String,
    region: String,
) -> Element {
    let values = [city, country, description, region];
    rsx! {
        for (index,label) in [(0,"City (optional)"),(2,"Location description (optional)"),(3,"State (optional)")] {
            label{r#for:format!("profile-section-field-{id}-{index}"),"{label}"}
            input{id:format!("profile-section-field-{id}-{index}"),r#type:"text",value:values[index].clone(),disabled,"aria-label":label,"data-dialog-input":"true",
                oninput:{let id=id.clone();let values=values.clone();move |e|{let mut values=values.clone();values[index]=e.value();location_change(state,visit,serial,id.clone(),values);}}}
        }
        label{"Country (optional)"}
        select{value:values[1].clone(),disabled,"aria-label":"Country (optional)","data-dialog-input":"true",
            onchange:{let id=id.clone();let values=values.clone();move |e|{let mut values=values.clone();values[1]=e.value();location_change(state,visit,serial,id.clone(),values);}},
            option{value:"","Not set"}
            for country in countries(){option{value:country.code.clone(),"{country.name}"}}
        }
        button{r#type:"button",disabled,onclick:move |_|location_change(state,visit,serial,id.clone(),std::array::from_fn(|_|String::new())),"Clear location"}
    }
}
fn location_change(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
    values: [String; 4],
) {
    let [city, country, description, region] = values;
    controller::change(
        state,
        visit,
        serial,
        id,
        SectionValue::Location {
            city,
            country,
            description,
            state: region,
        },
    );
}
#[component]
fn Persons(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
    disabled: bool,
    ids: Vec<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let people = controller::people(&state.read(), &query());
    rsx! {
        for (index,selected) in ids.iter().enumerate(){
            button{r#type:"button",disabled,"aria-label":"Remove {selected}",onclick:{let id=id.clone();let ids=ids.clone();move |_|{let mut next=ids.clone();next.remove(index);controller::change(state,visit,serial,id.clone(),SectionValue::Persons(next));}},"{controller::person_name(&state.read(), selected)}",{icon(Icon::Close,"icon")}}
        }
        input{id:format!("profile-section-field-{id}"),r#type:"search",value:query(),disabled,"aria-label":"Find a person","data-dialog-input":"true",oninput:move |e|{query.set(e.value());controller::query_people(state,visit,serial,e.value());}}
        div{class:"profile-section-people",
            for (person,name) in people.into_iter().filter(|(p,_)|!ids.contains(p)).take(30){
                button{r#type:"button",disabled,onclick:{let id=id.clone();let ids=ids.clone();move |_|{let mut next=ids.clone();next.push(person.clone());controller::change(state,visit,serial,id.clone(),SectionValue::Persons(next));}},"{name}"}
            }
        }
    }
}
