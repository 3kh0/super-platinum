use super::*;
use crate::profile_edit::sections as controller;
use super_platinum_core::slack::api::custom::{SectionValue, countries};
mod fields;
const CAPTURE: &str = include_str!("sections/capture.js");
const BRIDGE: &str = include_str!("sections/bridge.js");
pub(super) fn view(state: Signal<ShellState>) -> Element {
    let Some(v) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {SectionEditor{key:"profile-section-{v.visit}-{v.serial}",state}}
}
#[component]
fn SectionEditor(state: Signal<ShellState>) -> Element {
    let mut rich = use_signal(|| None::<(String, u64)>);
    let mut rich_visit = use_signal(|| 0u64);
    let mut leaving = use_signal(|| None::<bool>);
    let Some(v) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    let visit = v.visit;
    let serial = v.serial;
    let locked = v.busy || v.uncertain || leaving().is_some();
    let dirty = rich.peek().is_some() || v.form.as_ref().is_some_and(|f| f.dirty());
    let rich_id = rich();
    rsx! {
        div{class:"scrim",onclick:move |_|request_leave(state,visit,serial,dirty,true,leaving)}
        section{id:"shell-overlay-dialog",class:"modal profile-edit",role:"dialog","aria-modal":"true",
            "aria-labelledby":"profile-sections-title",tabindex:"-1","data-keyboard-dialog":"true",
            "data-dialog-kind":"other","data-dialog-generation":visit,"data-section-serial":serial,"data-editor-generation":serial,"data-section-locked":locked.to_string(),
            onkeydown:move |e|if e.key()==Key::Escape {e.prevent_default();e.stop_propagation();if leaving().is_some(){leaving.set(None);}else{request_leave(state,visit,serial,dirty,true,leaving);}},
            header{h2{id:"profile-sections-title","Profile fields"}
                button{r#type:"button",class:"icon-btn",disabled:v.busy,"aria-label":"Close profile fields",onclick:move |_|request_leave(state,visit,serial,dirty,true,leaving),{icon(Icon::Close,"icon")}}}
            form{onsubmit:move |e|{e.prevent_default();if rich.peek().is_none()&&leaving.peek().is_none(){controller::save(state,visit,serial);}},
                div{class:"profile-edit-body",
                    if v.busy {p{role:"status","Checking your profile…"}}
                    if let Some(form)=&v.form {
                        h3{"{form.label()}"}
                        if let Some((id,key))=rich_id {
                            if let Some(SectionValue::LongText(blocks))=form.value(&id) {
                                {rich_pane(state,visit,serial,id.clone(),key,blocks.clone(),locked,rich)}
                            }
                        } else {
                            for field in form.fields() {
                                fields::Field{key:"{serial}-{field.id}",state,visit,serial,field:field.clone(),
                                    value:form.value(&field.id).cloned(),locked,error:form.validation(&field.id).err().map(|_|"Check this field's value or length.".to_owned()),
                                    onrich:move |id|{let key=rich_visit()+1;rich_visit.set(key);rich.set(Some((id,key)));}}
                            }
                        }
                    } else {
                        for (id,label) in v.sections {
                            button{r#type:"button",class:"profile-section-choice",disabled:locked,onclick:move |_|controller::select(state,visit,serial,id.clone()),"{label}"}
                        }
                    }
                    if leaving().is_some(){p{class:"profile-edit-notice",role:"alert","Discard the changes to these profile fields?"}}
                    if let Some(message)=v.message {p{class:"profile-edit-notice",role:"status","{message}"}}
                    if v.uncertain {p{class:"profile-edit-notice","Checking reads your saved section. It does not send the change again."}}
                }
                footer{
                    if let Some(close)=leaving(){
                        button{r#type:"button",onclick:move |_|leaving.set(None),"Keep editing"}
                        button{r#type:"button",onclick:move |_|{leaving.set(None);if close{profile_edit::close(state,visit);}else{controller::back(state,visit,serial);}},"Discard changes"}
                    } else {
                    if v.can_keep {button{r#type:"button",onclick:move |_|controller::keep(state,visit,serial),"Keep current section"}}
                    button{r#type:"button",disabled:v.busy,onclick:move |_|request_leave(state,visit,serial,dirty,false,leaving),"Back"}
                    if v.uncertain {button{r#type:"button",disabled:!v.can_check,onclick:move |_|controller::check(state,visit,serial),"Check profile"}}
                    else {button{r#type:"submit",class:"profile-edit-primary",disabled:!v.can_save||rich.peek().is_some(),"Save section"}}
                    }
                }
            }
        }
    }
}
fn rich_pane(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
    key: u64,
    blocks: Vec<serde_json::Value>,
    locked: bool,
    mut rich: Signal<Option<(String, u64)>>,
) -> Element {
    let serialized = serde_json::to_string(&blocks).unwrap();
    let label = "Profile description".to_owned();
    let labels = {
        let shell = state.peek();
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
            .map(|w| crate::view::rich_editor::mention_labels(w, &blocks))
            .unwrap_or_else(|| "{}".into())
    };
    rsx! {
        div{key:"rich-{key}",id:"profile-section-rich-pane","data-rich-visit":key,"data-composing":"false",
            onmounted:move |_|{dioxus::document::eval(&format!("const pane=document.getElementById('profile-section-rich-pane');if(pane?.dataset.richVisit==='{key}'){{pane.addEventListener('compositionstart',()=>pane.dataset.composing='true');pane.addEventListener('compositionend',()=>pane.dataset.composing='false');}}"));},
            crate::view::rich_editor::RichEditor{id:"profile-section-rich",label,labels,blocks:serialized,disabled:locked,expected_generation:Some(serial)}
            button{r#type:"button",disabled:locked,onclick:move |_|{
                let id=id.clone();spawn(async move{
                    // Native unlock is five seconds after capture. Never admit a
                    // queued snapshot after the earlier host deadline, including
                    // a ready recv first polled after an executor stall.
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
                    let mut eval=dioxus::document::eval(&format!("await ({BRIDGE})(({CAPTURE}),{visit},{serial},{key},dioxus);"));
                    let mut accepted=false;
                    if let Ok(Ok(Some(blocks)))=tokio::time::timeout(std::time::Duration::from_secs(2),eval.recv::<Option<Vec<serde_json::Value>>>()).await {
                        if std::time::Instant::now()>=deadline {return;}
                        if rich.peek().as_ref()==Some(&(id.clone(),key))&&controller::presentation(&state.peek()).is_some_and(|v|v.visit==visit&&v.serial==serial&&!v.busy&&!v.uncertain){
                            if controller::use_description(state,visit,serial,&id,blocks){rich.set(None);accepted=true;}
                        }
                    }
                    // Only an admitted snapshot or explicit safe refusal may
                    // release before the native watchdog.
                    let _=eval.send(accepted);
                });
            },"Use description"}
            button{r#type:"button",disabled:locked,onclick:move |_|rich.set(None),"Cancel description edit"}
            p{id:"profile-section-rich-status",role:"status"}
        }
    }
}

fn request_leave(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    dirty: bool,
    close: bool,
    mut leaving: Signal<Option<bool>>,
) {
    if !controller::presentation(&state.peek())
        .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy)
    {
        return;
    }
    if dirty {
        leaving.set(Some(close));
    } else if close {
        profile_edit::close(state, visit);
    } else {
        controller::back(state, visit, serial);
    }
}
