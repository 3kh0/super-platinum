//! Mounted cell drafts stay local until the explicit Save action.
use crate::{
    slack_lists::{CellEditValue, EditorKind, EditorView},
    state::ShellState,
};
use dioxus::prelude::*;
mod entities;
mod link;

#[component]
pub(super) fn CellEditor(state: Signal<ShellState>, editor: EditorView, visit: u64) -> Element {
    let initial = editor.original.clone();
    let mut number = use_signal(move || match initial.as_ref() {
        CellEditValue::Number(value) => value.as_ref().map(ToString::to_string).unwrap_or_default(),
        _ => String::new(),
    });
    let initial = editor.original.clone();
    let mut selected = use_signal(move || match initial.as_ref() {
        CellEditValue::Select(values)
        | CellEditValue::Users(values)
        | CellEditValue::Channels(values) => values.clone(),
        _ => Vec::new(),
    });
    let mut reading = use_signal(|| false);
    let initial = editor.original.clone();
    let date = use_hook(move || match initial.as_ref() {
        CellEditValue::Date(value) => value.clone().unwrap_or_default(),
        _ => String::new(),
    });
    let mut error = use_signal(|| None::<String>);
    let generation = editor.generation;
    let layout_generation = editor.layout_generation;
    let row_id = serde_json::to_string(&editor.row_id).unwrap();
    let column_id = serde_json::to_string(&editor.column_id).unwrap();
    let disabled = editor.disabled || reading();
    let busy = editor.saving || reading();
    let save_disabled = disabled || editor.stale;
    let kind = editor.kind.clone();
    let initial = editor.original.clone();
    let blocks = use_hook(move || match initial.as_ref() {
        CellEditValue::RichText(blocks) => {
            serde_json::to_string(blocks).unwrap_or_else(|_| "[]".into())
        }
        _ => "[]".into(),
    });
    let initial = editor.original.clone();
    let labels = use_hook(move || {
        let shell = state.peek();
        match (
            initial.as_ref(),
            shell
                .core
                .active_team
                .as_ref()
                .and_then(|team| shell.core.workspaces.get(team)),
        ) {
            (CellEditValue::RichText(blocks), Some(workspace)) => {
                super::super::rich_editor::mention_labels(workspace, blocks)
            }
            _ => "{}".into(),
        }
    });
    rsx! {
        div { class:"slack-list-editor-shade" }
        section { id:"slack-list-cell-editor",class:"slack-list-cell-editor",role:"dialog",
            "aria-modal":"true","aria-labelledby":"slack-list-editor-title","aria-busy":busy,
            "data-keyboard-dialog":"true","data-escape-local":"true","data-editor-generation":generation,
            tabindex:"-1",
            onmounted:move |_| {dioxus::document::eval(&format!("{MOUNT}\nmountListCellEditor({generation},{visit},{row_id},{column_id});"));},
            onkeydown:move |event| if event.key()==Key::Escape {
                event.prevent_default();event.stop_propagation();
                crate::slack_lists::cancel_editor(state,generation);
            },
            h3 { id:"slack-list-editor-title","Edit {editor.column_label}" }
            p {class:"slack-list-editor-item","{editor.row_label}"}
            div { class:"slack-list-editor-body",
                match editor.kind {
                    EditorKind::RichText => rsx! {super::super::rich_editor::RichEditor {id:"slack-list-rich-draft",label:editor.column_label.clone(),blocks,labels,disabled,expected_generation:Some(generation)}},
                    EditorKind::Number => rsx! {
                        label {r#for:"slack-list-number-draft","{editor.column_label}"}
                        input {id:"slack-list-number-draft",r#type:"text",inputmode:"decimal",value:"{number}",disabled,
                            oninput:move |event| {number.set(event.value());error.set(None);}}
                        p {class:"slack-list-editor-hint","Leave empty to clear the value."}
                    },
                    EditorKind::Link => rsx! {link::LinkFields {state,editor:editor.clone(),reading,error}},
                    EditorKind::Date => rsx! {
                        label {r#for:"slack-list-date-draft","{editor.column_label}"}
                        // Keep native partial date segments intact. Save reads validity and the
                        // ISO value together; an invalid partial entry must never mean Clear.
                        input {id:"slack-list-date-draft",r#type:"date",initial_value:date,
                            disabled:editor.disabled,"aria-describedby":"slack-list-date-hint",
                            oninput:move |_|error.set(None)}
                        div {class:"slack-list-date-actions",
                            p {id:"slack-list-date-hint",class:"slack-list-editor-hint","Leave empty to clear the date."}
                            button {id:"slack-list-date-clear",r#type:"button",disabled,
                                onclick:move |_| {
                                    error.set(None);
                                    // WebKit's partial date has value="" but populated segments.
                                    // Its date setter refreshes segments only on value change;
                                    // force that path before clearing, in the same JS task.
                                    dioxus::document::eval(&format!("const root=document.getElementById('slack-list-cell-editor');const input=root?.querySelector('#slack-list-date-draft');if(root?.dataset.editorGeneration==='{generation}'&&input?.type==='date'&&!input.disabled){{if(input.value===''&&input.validity.badInput)input.value='2000-01-01';input.value='';input.dispatchEvent(new Event('input',{{bubbles:true}}));input.focus();}}"));
                                },"Clear date"}
                        }
                    },
                    EditorKind::Select | EditorKind::MultiSelect => rsx! {
                        fieldset {disabled,legend {"{editor.column_label}"}
                            if editor.kind==EditorKind::Select {
                                label {class:"slack-list-choice",input {r#type:"radio",name:"slack-list-choice",checked:selected.read().is_empty(),
                                    onchange:move |_|selected.set(Vec::new())}span {"No selection"}}
                            }
                            for choice in editor.choices.iter() {
                                {
                                    let id=choice.id.clone();
                                    let multi=editor.kind==EditorKind::MultiSelect;
                                    let checked=selected.read().contains(&id);
                                    rsx! {label {class:"slack-list-choice",key:"{choice.id}",
                                        input {r#type:if multi {"checkbox"} else {"radio"},name:"slack-list-choice",value:"{choice.id}",checked,
                                            onchange:move |event| {
                                                if multi {let mut values=selected.write();values.retain(|value|value!=&id);if event.checked(){values.push(id.clone());}}
                                                else {selected.set(vec![id.clone()]);}
                                            }}span {"{choice.label}"}
                                    }}
                                }
                            }
                        }
                    },
                    EditorKind::User | EditorKind::MultiUser => rsx! {entities::EntityPicker {
                        state,generation,selected,initial:editor.choices.clone(),directory:editor.directory.clone(),kind:crate::slack_lists::entities::Kind::Users,multi:editor.kind==EditorKind::MultiUser,disabled
                    }},
                    EditorKind::Channel | EditorKind::MultiChannel => rsx! {entities::EntityPicker {
                        state,generation,selected,initial:editor.choices.clone(),directory:editor.directory.clone(),kind:crate::slack_lists::entities::Kind::Channels,multi:editor.kind==EditorKind::MultiChannel,disabled
                    }},
                }
            }
            if let Some(message)=error.read().as_ref().or(editor.error.as_ref()) {p {class:"slack-list-error",role:"alert","{message}"}}
            div {class:"slack-list-editor-actions",
                if editor.stale {button {id:"slack-list-editor-refresh",r#type:"button",disabled:busy,onclick:move |_|crate::slack_lists::refresh(state),"Refresh"}}
                button {id:"slack-list-editor-cancel",r#type:"button",onclick:move |_|crate::slack_lists::cancel_editor(state,generation),"Cancel"}
                button {id:"slack-list-editor-save",r#type:"button",class:"primary",disabled:save_disabled,
                    onclick:move |_| {
                        if save_disabled || reading(){return;}
                        error.set(None);
                        match kind {
                            EditorKind::Number => {
                                let input=number.read().trim().to_owned();
                                let value=if input.is_empty(){Ok(None)}else{input.parse::<serde_json::Number>().map(Some)};
                                match value {
                                    Ok(value)=>crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::Number(value)),
                                    Err(_)=>error.set(Some("Enter a valid number, or leave the value empty.".into())),
                                }
                            }
                            EditorKind::Select | EditorKind::MultiSelect => crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::Select(selected.read().clone())),
                            EditorKind::User | EditorKind::MultiUser => crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::Users(selected.read().clone())),
                            EditorKind::Channel | EditorKind::MultiChannel => crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::Channels(selected.read().clone())),
                            EditorKind::Link => link::save(state,generation,layout_generation,reading,error),
                            EditorKind::Date => {
                                reading.set(true);
                                spawn(async move {
                                    let mut eval=dioxus::document::eval(&format!("let input,root;try{{root=document.getElementById('slack-list-cell-editor');input=root?.querySelector('#slack-list-date-draft');let snapshot=null;if(root?.dataset.editorGeneration==='{generation}'&&input?.type==='date'&&!input.disabled){{snapshot=[input.value,!input.validity.badInput&&input.validity.valid];if(snapshot[1])input.disabled=true;}}dioxus.send(snapshot);}}catch{{dioxus.send(null);}}const restore=await dioxus.recv();if(restore&&root?.isConnected&&root.dataset.editorGeneration==='{generation}'&&input?.isConnected)input.disabled=false;"));
                                    let value=eval.recv::<Option<(String,bool)>>().await.ok().flatten();
                                    if let Ok(mut pending)=reading.try_write(){*pending=false;}
                                    match value {
                                        Some((value,true))=>crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::Date((!value.is_empty()).then_some(value))),
                                        Some((_,false))=>if let Ok(mut message)=error.try_write(){*message=Some("Enter a complete date, or clear the field.".into());},
                                        None=>if let Ok(mut message)=error.try_write(){*message=Some("The date could not be read. Try again.".into());},
                                    }
                                    // A rejected value leaves the same draft editable. Accepted
                                    // saves stay locked until the controller's normal readback.
                                    let restore=crate::slack_lists::presentation(&state.peek())
                                        .and_then(|view|view.editor)
                                        .is_some_and(|editor|editor.generation==generation&&!editor.disabled);
                                    let _=eval.send(restore);
                                });
                            }
                            EditorKind::RichText => {
                                reading.set(true);
                                spawn(async move {
                                    let mut eval=dioxus::document::eval(&format!("try{{const root=document.getElementById('slack-list-cell-editor');dioxus.send(root?.dataset.editorGeneration==='{generation}'?window.superPlatinumRichEditor.read('slack-list-rich-draft'):null);}}catch{{dioxus.send(null);}}await dioxus.recv();"));
                                    let value=eval.recv::<Option<Vec<serde_json::Value>>>().await.ok().flatten();
                                    let _=eval.send(());
                                    if let Ok(mut pending)=reading.try_write(){*pending=false;}
                                    match value {
                                        Some(blocks)=>crate::slack_lists::save_editor(state,generation,layout_generation,CellEditValue::RichText(blocks)),
                                        None=>if let Ok(mut message)=error.try_write(){*message=Some("The cell could not be read. Try again.".into());},
                                    }
                                });
                            }
                        }
                    },if busy {"Saving…"}else{"Save"}}
            }
        }
    }
}

// The table remains mounted while editing. Return to the same cell only if
// this list visit still owns the overlay and focus was not moved elsewhere.
const MOUNT: &str = r#"function mountListCellEditor(generation,visit,rowId,columnId){
 const editor=document.getElementById('slack-list-cell-editor');
 if(!editor||editor.dataset.editorGeneration!==String(generation))return;
 const dialog=editor.closest('.slack-list-dialog'),previous=document.activeElement;
 editor.dataset.focusPhase='mounted';
 editor.dataset.focusPreviousId=previous?.id||'';
 editor.dataset.focusPreviousTag=previous?.tagName||'';
 const matchesCell=candidate=>candidate?.dataset.columnId===columnId&&candidate.closest('[data-row-id]')?.dataset.rowId===rowId;
 const cells=()=>[...dialog.querySelectorAll('.slack-list-grid td[data-column-id],.slack-list-card-field[data-column-id],.slack-list-detail-field[data-column-id]')].filter(cell=>!cell.closest('[hidden],[inert]'));
 const previousCell=previous?.closest('td,.slack-list-card-field,.slack-list-detail-field');
 const cell=matchesCell(previousCell)?previousCell:cells().find(matchesCell);
 const instanceId=cell?.closest('[data-instance-id]')?.dataset.instanceId;
 const observer=new MutationObserver(()=>{
   if(editor.isConnected)return;observer.disconnect();
   if(dialog.isConnected&&dialog.dataset.dialogGeneration===String(visit)&&!dialog.querySelector('#slack-list-cell-editor')){
     const active=document.activeElement;
     if(!active||active===document.body||editor.contains(active)){
       const available=cells();
       const target=cell?.isConnected&&dialog.contains(cell)&&!cell.closest('[hidden],[inert]')&&matchesCell(cell)?cell:
         available.find(candidate=>matchesCell(candidate)&&candidate.closest('[data-instance-id]')?.dataset.instanceId===instanceId)||
         available.find(matchesCell)||available.find(candidate=>candidate.tabIndex===0)||dialog.querySelector('#slack-list-close');
       target?.focus({preventScroll:true});
       if(target?.matches('.slack-list-grid td,.slack-list-card-field,.slack-list-detail-field'))target.scrollIntoView({block:'nearest',inline:'nearest'});
     }
   }
 });observer.observe(dialog,{childList:true,subtree:true});
 requestAnimationFrame(()=>{
   if(!editor.isConnected||dialog.dataset.dialogGeneration!==String(visit)){editor.dataset.focusPhase='stale-owner';return;}
   const active=document.activeElement;
   editor.dataset.focusCurrentId=active?.id||'';
   editor.dataset.focusCurrentTag=active?.tagName||'';
   if(active&&active!==document.body&&active!==previous&&!editor.contains(active)){editor.dataset.focusPhase='focus-moved';return;}
   const rendered=selector=>[...editor.querySelectorAll(selector)].find(control=>control.getClientRects().length>0);
   const target=rendered('[contenteditable=true]')||rendered('input:not(:disabled)')||rendered('button:not(:disabled)')||editor;
   target.focus({preventScroll:true});
   editor.dataset.focusPhase=editor.contains(document.activeElement)?'focused':'focus-rejected';
 });
}"#;
