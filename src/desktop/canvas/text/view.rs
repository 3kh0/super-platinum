//! A single-paragraph native editor using the shared structured rich input.
use super::*;
use super_platinum_core::canvas::edit::text::TextDraft;

const STATUS_LABELS: [&str; 3] = ["Unconfirmed", "Checking\u{2026}", "Saving\u{2026}"];

#[component]
pub(in crate::canvas) fn Editor(
    state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
) -> Element {
    let blocks = use_hook(|| {
        draft
            .peek()
            .as_ref()
            .map(|d| d.blocks.clone())
            .unwrap_or_default()
    });
    let d = draft.read();
    let Some(d) = d.as_ref() else {
        return rsx! {};
    };
    let generation = d.generation;
    let visit = d.visit;
    let section = d.section.clone();
    let mount = format!(
        "{MOUNT}\nmountCanvasText({generation},{visit},{});",
        serde_json::to_string(&section).unwrap()
    );
    let label = d.kind.label();
    let replacement = matches!(d.kind, Kind::Replacement);
    let deletion = replacement || d.kind.action() == Some(Action::Delete);
    let insertion = matches!(d.kind.action(), Some(Action::Before | Action::After));
    let heading = d.kind.heading();
    let allow_underline = matches!(d.kind, Kind::Text(_));
    let field_label = if insertion || deletion {
        label
    } else if heading {
        "Heading"
    } else {
        "Paragraph"
    }
    .to_owned();
    let replacement_target = d
        .pending
        .as_ref()
        .and_then(checklist::replacement_ops::replacement)
        .unwrap_or("");
    let pending = d.pending.is_some();
    let busy = d.reading || pending;
    let stale = !pending && !current(&state.read(), d);
    let disabled = busy || stale || reader.peek().loading;
    let error = d.error;
    let stage = checklist::presentation(&state.read())
        .and_then(|view| view.operation)
        .filter(|(serial, _, _)| {
            d.pending
                .as_ref()
                .is_some_and(|attempt| attempt.serial == *serial)
        })
        .map(|(_, _, stage)| stage);
    let cancellable = !pending || stage == Some(checklist::Stage::Preparing);
    let idle_label = if deletion {
        "Delete paragraph"
    } else if insertion {
        "Add paragraph"
    } else {
        "Save"
    };
    let save_label = match stage {
        Some(checklist::Stage::Uncertain) => STATUS_LABELS[0],
        Some(checklist::Stage::AwaitingRead) => STATUS_LABELS[1],
        _ if busy => STATUS_LABELS[2],
        _ => idle_label,
    };
    rsx! {
        section { id:"canvas-text-editor",class:"canvas-text-editor","data-editor-generation":generation,"data-canvas-visit":visit,"data-replacement-target":replacement_target,
            "aria-label":label,
            onmounted:move |_|{dioxus::document::eval(&mount);if deletion {dioxus::document::eval(&format!("queueMicrotask(()=>{{const root=document.getElementById('canvas-text-editor');if(root?.dataset.editorGeneration==='{generation}')root.querySelector('#canvas-text-cancel')?.focus();}});"));}},
            onkeydown:move |event| {
                if event.key()==Key::Escape {
                    event.prevent_default();event.stop_propagation();if cancellable {cancel(state,draft,generation);}
                }
            },
            crate::view::rich_editor::RichEditor {id:"canvas-text-draft",label:field_label,blocks,allow_lists:false,allow_underline,disabled:disabled||deletion,autofocus:!deletion,expected_generation:Some(generation)},
            p {class:"canvas-text-hint",if replacement {"Delete this paragraph and leave an empty paragraph in its place? Its text and formatting will be removed. If the result cannot be confirmed, both changes stay locked for review."} else if deletion {"Delete this paragraph? Its text and formatting will be removed from this canvas."} else if insertion {"Add one paragraph here. Use Shift+Enter for a line break."} else if heading {"Edit this heading. Its heading level stays the same."} else {"Edit this paragraph. Use Shift+Enter for a line break."}}
            if stale {p {role:"status",if heading {"This canvas changed. Cancel and refresh before saving this heading."} else {"This canvas changed. Cancel and refresh before saving this paragraph."}}}
            if let Some(error)=error {p {role:"alert","{error}"}}
            div {class:"canvas-text-actions",
                button {id:"canvas-text-cancel",r#type:"button",disabled:!cancellable,onclick:move |_|cancel(state,draft,generation),"Cancel"}
                button {id:"canvas-text-save",r#type:"button",class:"primary",disabled,
                    onclick:move |_| {
                        if disabled {return;}
                        {
                            let mut d=draft.write();
                            let Some(d)=d.as_mut().filter(|d|d.generation==generation&&!d.reading&&d.pending.is_none()) else {return;};
                            d.reading=true;d.error=None;
                        }
                        if deletion { super::save::submit(state,reader,draft,generation,TextDraft::default()); return; }
                        spawn(async move {
                            // Snapshot and disable in the same JS task: no keystroke may
                            // change a draft after the value authorized for dispatch.
                            let mut eval=dioxus::document::eval(&format!("let root,field;try{{root=document.getElementById('canvas-text-editor');field=root?.querySelector('#canvas-text-draft');let value=null;if(root?.dataset.editorGeneration==='{generation}'&&root.dataset.canvasVisit==='{visit}'&&field){{value=window.superPlatinumRichEditor.read('canvas-text-draft');field.contentEditable='false';root.querySelectorAll('.format-toolbar button,#canvas-text-save').forEach(e=>e.disabled=true);}}dioxus.send(value);}}catch{{dioxus.send(null);}}const restore=await dioxus.recv();if(restore&&root?.isConnected&&root.dataset.editorGeneration==='{generation}'){{field.contentEditable='true';root.querySelectorAll('.format-toolbar button,#canvas-text-save').forEach(e=>e.disabled=false);}}"));
                            let blocks=eval.recv::<Option<Vec<serde_json::Value>>>().await.ok().flatten();
                            let value=blocks.as_deref().and_then(|blocks|TextDraft::from_rich_blocks(blocks).ok());
                            let still_owned=draft.try_read().ok().is_some_and(|d|d.as_ref().is_some_and(|d|d.generation==generation));
                            if still_owned {
                                if let Some(value)=value {super::save::submit(state,reader,draft,generation,value);}
                                else if let Ok(mut d)=draft.try_write() {if let Some(d)=d.as_mut().filter(|d|d.generation==generation) {d.reading=false;d.error=Some("Keep this edit to one paragraph with supported text formatting.");}}
                            }
                            let restore=draft.try_read().ok().is_some_and(|d|d.as_ref().is_some_and(|d|d.generation==generation&&d.pending.is_none()&&current(&state.peek(),d))) && !reader.peek().loading;
                            let _=eval.send(restore);
                        });
                    },
                    span {class:"canvas-text-save-label","{save_label}"}
                    span {class:"canvas-text-save-size","aria-hidden":"true","data-label":idle_label}
                    for status in STATUS_LABELS {span {class:"canvas-text-save-size","aria-hidden":"true","data-label":status}}
                }
            }
        }
    }
}

// One observer per mounted draft; it disconnects on unmount and never changes
// focus in a replacement canvas or after the user focused another control.
const MOUNT: &str = r#"function mountCanvasText(generation,visit,section){
 const editor=document.getElementById('canvas-text-editor');
 if(editor?.dataset.editorGeneration!==String(generation)||editor.dataset.canvasVisit!==String(visit))return;
 const dialog=editor.closest('.canvas-dialog[data-dialog-generation]');
 if(!dialog?.isConnected||dialog.dataset.dialogGeneration!==String(visit))return;
 if(!((dialog.id==='shell-overlay-dialog'&&dialog.getAttribute('role')==='dialog')||
      (dialog.id==='channel-canvas-panel'&&dialog.getAttribute('role')==='region')))return;
 const observer=new MutationObserver(()=>{
  if(editor.isConnected)return;observer.disconnect();
  if(!dialog?.isConnected||dialog.dataset.dialogGeneration!==String(visit)||dialog.querySelector('#canvas-text-editor'))return;
  const active=document.activeElement;
  if(active&&active!==document.body&&!editor.contains(active))return;
  const original=[...dialog.querySelectorAll('[data-canvas-text-section]')].find(e=>e.dataset.canvasTextSection===section);
  const wanted=original?section:(editor.dataset.replacementTarget||section);
  const paragraph=[...dialog.querySelectorAll('[data-canvas-text-section]')].find(e=>e.dataset.canvasTextSection===wanted);
  const target=paragraph?.querySelector('.canvas-text-open:not(:disabled)')||(!editor.dataset.replacementTarget&&dialog.querySelector('.canvas-paragraph-actions button:not(:disabled)'));
  if(target&&!target.disabled){target.focus({preventScroll:true});target.scrollIntoView({block:'nearest'});}
 });observer.observe(dialog,{childList:true,subtree:true});
}"#;
