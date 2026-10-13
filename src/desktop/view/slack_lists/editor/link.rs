//! Link drafts are inert native text inputs. Only explicit actions can save.
use super::*;
use super_platinum_core::slack_lists::LinkValue;

#[component]
pub(super) fn LinkFields(
    state: Signal<ShellState>,
    editor: EditorView,
    reading: Signal<bool>,
    mut error: Signal<Option<String>>,
) -> Element {
    let original = editor.original.clone();
    let initial = use_hook(move || match original.as_ref() {
        CellEditValue::Link(value) => value.clone(),
        _ => None,
    });
    let mut dirty = use_signal(|| false);
    let generation = editor.generation;
    let layout = editor.layout_generation;
    let disabled = editor.disabled || reading();
    let url = initial
        .as_ref()
        .map(|value| value.original_url.clone())
        .unwrap_or_default();
    let name = use_hook(move || editor.link_name_seed.clone());
    let mode = initial.clone();
    rsx! {
        label {r#for:"slack-list-link-url","Link"}
        input {id:"slack-list-link-url",r#type:"text",initial_value:url,disabled:editor.disabled,
            autocomplete:"off",spellcheck:"false",
            oninput:move |_| {dirty.set(true);error.set(None);}}
        label {r#for:"slack-list-link-name","Name"}
        input {id:"slack-list-link-name",r#type:"text",initial_value:name,disabled:editor.disabled,
            autocomplete:"off",oninput:move |_| {dirty.set(true);error.set(None);}}
        p {class:"slack-list-editor-hint","Optional name shown instead of the link. Saving uses the name when present."}
        if let Some(value)=mode {
            div {class:"slack-list-date-actions",
                button {id:"slack-list-link-display",r#type:"button",disabled:disabled||dirty(),
                    title:if dirty(){"Save or cancel your edits before changing the display."}else{"Change how the saved link is displayed."},
                    onclick:move |_| {
                        if disabled||reading()||dirty(){return;}
                        let mut value=value.clone();value.display_as_url=!value.display_as_url;
                        crate::slack_lists::save_editor(state,generation,layout,CellEditValue::Link(Some(value)));
                    },if initial.as_ref().is_some_and(|value|value.display_as_url){"Display as name"}else{"Display as URL"}}
                button {id:"slack-list-link-remove",r#type:"button",disabled,
                    onclick:move |_| {if !disabled&&!reading() {crate::slack_lists::save_editor(state,generation,layout,CellEditValue::Link(None));}},"Remove Link"}
            }
        }
    }
}

pub(super) fn save(
    state: Signal<ShellState>,
    generation: u64,
    layout: u64,
    mut reading: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    reading.set(true);
    spawn(async move {
        let mut eval = dioxus::document::eval(&format!(
            "let root,url,name;try{{root=document.getElementById('slack-list-cell-editor');url=root?.querySelector('#slack-list-link-url');name=root?.querySelector('#slack-list-link-name');let snapshot=null;if(root?.dataset.editorGeneration==='{generation}'&&url&&name&&!url.disabled&&!name.disabled){{snapshot=[url.value,name.value];url.disabled=true;name.disabled=true;}}dioxus.send(snapshot);}}catch{{dioxus.send(null);}}const restore=await dioxus.recv();if(restore&&root?.isConnected&&root.dataset.editorGeneration==='{generation}'){{url.disabled=false;name.disabled=false;}}"
        ));
        let input = eval.recv::<Option<(String, String)>>().await.ok().flatten();
        match input {
            Some((url, name)) => {
                let permit = crate::slack_lists::link_normalization_slot().await;
                let current = crate::slack_lists::presentation(&state.peek())
                    .and_then(|view| view.editor)
                    .is_some_and(|editor| {
                        editor.generation == generation
                            && editor.layout_generation == layout
                            && !editor.disabled
                    });
                if !current {
                    if let Ok(mut pending) = reading.try_write() {
                        *pending = false;
                    }
                    let _ = eval.send(false);
                    return;
                }
                match tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    LinkValue::from_input(&url, Some(&name))
                })
                .await
                {
                    Ok(Ok(value)) => crate::slack_lists::save_editor(
                        state,
                        generation,
                        layout,
                        CellEditValue::Link(value),
                    ),
                    _ => {
                        if let Ok(mut message) = error.try_write() {
                            *message = Some(
                                "This link could not be prepared. Review the link and try again."
                                    .into(),
                            );
                        }
                    }
                }
            }
            None => {
                if let Ok(mut message) = error.try_write() {
                    *message = Some("The link could not be read. Try again.".into());
                }
            }
        }
        if let Ok(mut pending) = reading.try_write() {
            *pending = false;
        }
        let restore = crate::slack_lists::presentation(&state.peek())
            .and_then(|view| view.editor)
            .is_some_and(|editor| editor.generation == generation && !editor.disabled);
        let _ = eval.send(restore);
    });
}
