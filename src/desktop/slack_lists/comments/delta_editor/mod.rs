//! Comments-only neutral Delta surface. The retained native owner, not this
//! component's lifetime, releases the original model after final capture ACK.
//! The parent waits for Eval completion before rotating the view.
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub(crate) mod clipboard_host;
pub(crate) mod clipboard_protocol;
pub(crate) mod emoji_observations;

/// The retained own-edit bridge owns invocation and its original Eval channel.
pub(crate) const CLIPBOARD_BRIDGE: &str = include_str!("dispatch.js");

const QUILL: &str = include_str!("../../../../../assets/comments-quill/quill-1.3.7.js");
const MODEL: &str = include_str!("model.js");
const BLOTS: &str = include_str!("blots.js");
const EMOTICONS: &str = include_str!("emoticons.js");
pub(crate) const EMOTICON_TABLE: &str = include_str!("emoticons.json");
const SMART_QUOTES: &str = include_str!("smart_quotes.js");
const SMART_SYMBOLS: &str = include_str!("smart_symbols.js");
const LIVE_EMOJI: &str = include_str!("live_emoji.js");
const EDITOR: &str = include_str!("editor.js");
const CLIPBOARD: &str = include_str!("clipboard.js");
const PASTE: &str = include_str!("paste.js");
const CLIPBOARD_EVENTS: &str = include_str!("clipboard_events.js");
const STYLE: &str = include_str!("editor.css");

/// Kept distinct from Slack blocks throughout capture and final custody.
/// Portable has no admitted initializer/native policy yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Mode {
    PlainText,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum Captured {
    RichBlocks { blocks: Vec<Value> },
    NeutralDelta { mode: Mode, ops: Vec<Value> },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct EmojiAppearance {
    pub name: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

/// These are positively acquired host observations. In particular `false` is
/// not a replacement for an unavailable preference or feature assignment.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Policy {
    pub slugs_enabled: bool,
    pub mac: bool,
    pub enter_creates_newline: bool,
    pub use_pre_newlines: bool,
    pub spellcheck: bool,
    pub underline_links: bool,
    pub convert_emoticons: bool,
    pub emoji_as_text: bool,
    pub composer_smart_symbols: bool,
    pub disable_smart_symbols: bool,
    pub detect_go_links: bool,
    pub flatten_tree_list_before_paste: bool,
    pub supports_portable_content: bool,
    pub markdown_mode: bool,
    pub emoji: Vec<EmojiAppearance>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "emoji_observations::serialize_live"
    )]
    pub live_emoji: Option<Arc<emoji_observations::LiveEmoji>>,
}

#[component]
pub(crate) fn DeltaEditor(
    id: &'static str,
    scope: String,
    generation: u64,
    label: String,
    mode: Mode,
    ops: Vec<Value>,
    policy: Policy,
    media_epoch: u64,
    #[props(default)] disabled: bool,
    #[props(default)] autofocus: bool,
    on_unavailable: EventHandler<()>,
) -> Element {
    use_effect(use_reactive(
        (&id, &scope, &generation, &media_epoch),
        move |(id, scope, generation, epoch)| {
            let args = serde_json::json!([id, scope, generation.to_string(), epoch.to_string()]);
            dioxus::document::eval(&format!(
                "(()=>{{const [id,scope,generation,epoch]={args};return window.superPlatinumCommentDelta?.refreshMedia(document.getElementById(id),scope,generation,epoch)??false;}})();"
            ));
        },
    ));
    let label_id = format!("{id}-label");
    // The parent keys this component by its original editor generation. Keep
    // that mount payload once; media ticks must not serialize the whole emoji
    // catalog again or replace the user's current draft with the initial ops.
    let options = use_hook({
        let scope = scope.clone();
        let label = label.clone();
        move || {
            let mut options = serde_json::to_value(policy).expect("serializable Delta policy");
            let object = options.as_object_mut().expect("Delta policy object");
            object.extend(
                serde_json::json!({"id":id,"scope":scope,"generation":generation.to_string(),
                    "label":label,"mode":mode,"ops":ops,"disabled":disabled,"autofocus":autofocus,
                    "mediaEpoch":media_epoch.to_string()})
                .as_object()
                .expect("Delta mount options")
                .clone(),
            );
            std::sync::Arc::new(options)
        }
    });
    rsx! {
        div { class: "rich-editor list-comment-delta",
            style { "{STYLE}" }
            label { id: "{label_id}", "{label}" }
            // No Dioxus children: Quill owns this exact host's descendants.
            // The parent keys this component by editor_generation.
            div {
                id,
                "data-list-comment-scope": scope,
                "data-editor-generation": generation.to_string(),
                "data-delta-disabled": disabled.to_string(),
                onmounted: move |_| {
                    let script = mount_script(&options);
                    spawn(async move {
                        let result = dioxus::document::eval(&script).join::<bool>().await;
                        if !matches!(result, Ok(true)) {
                            on_unavailable.call(());
                        }
                    });
                },
            }
        }
    }
}

fn mount_script(options: &Value) -> String {
    // UMD receives a private CommonJS module. It does not publish window.Quill,
    // replace the shared rich editor, fetch scripts, or evaluate message HTML.
    format!(
        "if(!window.superPlatinumCommentDelta){{\n\
         const CommentsQuill=(()=>{{const module={{exports:{{}}}};const exports=module.exports;\n\
         {QUILL}\nreturn module.exports;}})();\n\
         {MODEL}\n{BLOTS}\nconst LIST_COMMENT_EMOTICONS={EMOTICON_TABLE};\n{EMOTICONS}\n{SMART_QUOTES}\n{SMART_SYMBOLS}\n{CLIPBOARD}\n{PASTE}\n{CLIPBOARD_EVENTS}\n{LIVE_EMOJI}\n{EDITOR}\n\
         window.superPlatinumCommentDelta=listCommentDeltaEditor(CommentsQuill);\n}}\n\
         const options={options};\n\
         return !!window.superPlatinumCommentDelta.mount(document.getElementById(options.id),options);"
    )
}
