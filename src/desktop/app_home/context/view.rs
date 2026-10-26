use super::*;

#[component]
pub(crate) fn ContextChooser(mut state: Signal<ShellState>, locked: bool) -> Element {
    let choice = Choice::capture(&state.read());
    match choice {
        Ok(None) => rsx! {},
        Ok(Some(choice)) if choice.rows.len() > 1 && choice.witness.team_id().is_some() => {
            let selected = choice.witness.team_id().unwrap_or_default().to_owned();
            let rows = choice.rows.clone();
            let outside = outside_option(choice.witness.team_id(), &rows, choice.source.cfm.rows());
            rsx! {label {class:"app-home-context","data-home-context":"chooser", "Workspace "
                select {"aria-label":"App Home workspace",disabled:locked,value:selected,
                    onchange:move |event|{choose(&mut state.write(),&choice,&event.value());},
                    if let Some((id,name))=outside {option {value:id,disabled:true,"{name}"}}
                    for (id,name) in rows {option {value:id,"{name}"}}
                }
            }}
        }
        Ok(Some(_)) => rsx! {},
        Err(_) => {
            rsx! {span {class:"muted","data-home-context":"unavailable",role:"status","Workspace context is unavailable. Your inputs are retained."}}
        }
    }
}
