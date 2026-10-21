//! Ordinary channel and mention suggestions. Native owns the caret ticket; rows contain
//! labels and an index into the host's private revisioned completion catalog.
use super::*;
use crate::slack_lists::comments::{completion as controller, draft};
use serde::Deserialize;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Trigger {
    #[default]
    Channel,
    Member,
}
impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Member => "member",
        }
    }
    fn loading(self) -> &'static str {
        match self {
            Self::Channel => "Searching channels...",
            Self::Member => "Searching mentions...",
        }
    }
    fn empty(self) -> &'static str {
        match self {
            Self::Channel => "No matching channels",
            Self::Member => "No matching mentions",
        }
    }
    fn failure(self) -> &'static str {
        match self {
            Self::Channel => "Channel suggestions could not load. Edit your query to try again.",
            Self::Member => "Mention suggestions could not load. Edit your query to try again.",
        }
    }
}
#[derive(Clone, Default)]
struct Popup {
    trigger: Trigger,
    sequence: u64,
    revision: u64,
    labels: Vec<String>,
    selected: usize,
    status: String,
}
#[derive(Default)]
struct Runtime {
    live: Cell<bool>,
    sequence: Cell<u64>,
    eval: RefCell<Option<dioxus::document::Eval>>,
    lease: RefCell<Option<controller::Lease>>,
    task: RefCell<Option<dioxus::core::Task>>,
}
impl Runtime {
    fn retire(&self, mut state: Signal<ShellState>) {
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
        if let Some(lease) = self.lease.borrow_mut().take() {
            controller::close(&mut state.write(), &lease);
        }
    }
    fn stop(&self, state: Signal<ShellState>) {
        self.live.set(false);
        self.retire(state);
        if let Some(eval) = self.eval.borrow_mut().take() {
            let _ = eval.send(json!({"kind":"stop"}));
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Stopped,
    Context {
        sequence: u64,
        before: String,
        revision: u64,
    },
    Query {
        trigger: Trigger,
        sequence: u64,
        query: String,
        before: String,
        index: usize,
        revision: u64,
    },
    Closed {
        sequence: u64,
    },
    Highlight {
        sequence: u64,
        revision: u64,
        index: usize,
    },
    Choose {
        sequence: u64,
        revision: u64,
        index: usize,
    },
    Inserted {
        sequence: u64,
        accepted: bool,
    },
}

#[component]
pub(super) fn Suggestions(state: Signal<ShellState>, generation: u64, scope: String) -> Element {
    let binding = use_hook(move || draft::presentation(&state.peek()).map(|view| view.binding));
    let mut shown = use_signal(Popup::default);
    let runtime = use_hook(|| {
        Rc::new(Runtime {
            live: Cell::new(true),
            ..Runtime::default()
        })
    });
    let cleanup = runtime.clone();
    use_drop(move || cleanup.stop(state));
    let watched = runtime.clone();
    let watched_binding = binding.clone();
    use_effect(move || {
        let current = watched_binding
            .as_ref()
            .is_some_and(|binding| binding.current(&state.read()));
        if !current && watched.live.get() {
            watched.stop(state);
            shown.set(Popup::default());
        }
    });
    let running = runtime.clone();
    use_future(move || {
        let runtime = running.clone();
        let binding = binding.clone();
        let scope = scope.clone();
        async move {
            let Some(binding) = binding else { return };
            if binding.generation() != generation || binding.scope() != scope {
                return;
            }
            let mut mounted = None;
            for _ in 0..60 {
                if !runtime.live.get() || !binding.current(&state.peek()) {
                    return;
                }
                mounted = draft::native(&state.peek(), &binding);
                if mounted.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            }
            let Some(mounted) = mounted else { return };
            let mount = mounted.mount().to_owned();
            let script = format!(
                r#"
                const matcher=({matcher})({data});
                for(let i=0;i<60&&!window.__listCommentsComposeCaptures?.get({mount})?.running;i++)await new Promise(r=>setTimeout(r,16));
                const control=({bridge})({scope},{generation},{mount},matcher,event=>dioxus.send(event));
                if(control){{try{{while(control.current())control.message(await dioxus.recv());}}finally{{control.stop();}}}}
            "#,
                matcher = include_str!("../../profile_edit/sections/rich/trigger.js"),
                data = include_str!("../../profile_edit/sections/rich/trigger-data.json"),
                bridge = include_str!("completion.js"),
                scope = json!(scope),
                mount = json!(mount)
            );
            let mut eval = dioxus::document::eval(&script);
            *runtime.eval.borrow_mut() = Some(eval);
            while let Ok(event) = eval.recv::<Event>().await {
                if !runtime.live.get()
                    || !binding.current(&state.peek())
                    || !draft::native(&state.peek(), &binding)
                        .is_some_and(|native| native.mount() == mount)
                {
                    break;
                }
                match event {
                    Event::Stopped => break,
                    Event::Context {
                        sequence,
                        before,
                        revision,
                    } => {
                        if sequence <= runtime.sequence.get()
                            || sequence >= 9_007_199_254_740_991
                            || revision >= 9_007_199_254_740_991
                            || before.len() > 1_048_576
                        {
                            continue;
                        }
                        runtime.sequence.set(sequence);
                        runtime.retire(state);
                        shown.set(Popup::default());
                        let mut state = state;
                        let lease =
                            controller::begin(&mut state.write(), &binding, &mount, sequence);
                        *runtime.lease.borrow_mut() = lease.clone();
                        let Some(lease) = lease else { continue };
                        let task_runtime = runtime.clone();
                        let task = spawn(async move {
                            let result = controller::policy(state, &lease).await;
                            if task_runtime.live.get()
                                && task_runtime.sequence.get() == sequence
                                && lease.current(&state.peek())
                            {
                                match result {
                                    Ok(friendly)=>{let _=eval.send(json!({"kind":"policy","sequence":sequence,"friendly":friendly}));}
                                    Err(_)=>shown.set(Popup{sequence,status:"Suggestions could not load. Edit your query to try again.".into(),..Popup::default()}),
                                }
                            }
                        });
                        *runtime.task.borrow_mut() = Some(task);
                    }
                    Event::Query {
                        trigger,
                        sequence,
                        query: term,
                        before,
                        index,
                        revision,
                    } => {
                        if sequence != runtime.sequence.get()
                            || revision >= 9_007_199_254_740_991
                            || !valid_query(trigger, &before, &term, index)
                        {
                            continue;
                        }
                        let lease = runtime.lease.borrow().clone();
                        let Some(lease) = lease.filter(|lease| lease.current(&state.peek())) else {
                            continue;
                        };
                        shown.set(Popup {
                            trigger,
                            sequence,
                            status: trigger.loading().into(),
                            ..Popup::default()
                        });
                        let task_runtime = runtime.clone();
                        let results_runtime = runtime.clone();
                        let results_lease = lease.clone();
                        let task = spawn(async move {
                            let on_results = move |choices| {
                                if results_runtime.live.get()
                                    && results_runtime.sequence.get() == sequence
                                    && results_lease.current(&state.peek())
                                {
                                    publish(eval, shown, trigger, choices);
                                }
                            };
                            let result = match trigger {
                                Trigger::Channel => {
                                    controller::run(state, lease.clone(), term, on_results).await
                                }
                                Trigger::Member => {
                                    controller::run_members(state, lease.clone(), term, on_results)
                                        .await
                                }
                            };
                            if task_runtime.live.get() && task_runtime.sequence.get() == sequence {
                                if !lease.current(&state.peek()) {
                                    let _ = eval
                                        .send(json!({"kind":"invalidated","sequence":sequence}));
                                    shown.set(Popup::default());
                                } else if let Err(error) = result {
                                    let _ =
                                        eval.send(json!({"kind":"failure","sequence":sequence}));
                                    let mut popup = shown.write();
                                    popup.labels.clear();
                                    popup.status = if matches!(
                                        error,
                                        super_platinum_core::slack::Error::RateLimited { .. }
                                    ) {
                                        "Slack asked us to wait before searching again."
                                    } else {
                                        trigger.failure()
                                    }
                                    .into();
                                }
                            }
                        });
                        *runtime.task.borrow_mut() = Some(task);
                    }
                    Event::Closed { sequence } => {
                        if sequence <= runtime.sequence.get() || sequence >= 9_007_199_254_740_991 {
                            continue;
                        }
                        runtime.sequence.set(sequence);
                        runtime.retire(state);
                        shown.set(Popup::default());
                    }
                    Event::Highlight {
                        sequence,
                        revision,
                        index,
                    } => {
                        let mut popup = shown.write();
                        if popup.sequence == sequence
                            && popup.revision == revision
                            && index < popup.labels.len()
                        {
                            popup.selected = index;
                        }
                    }
                    Event::Choose {
                        sequence,
                        revision,
                        index,
                    } => {
                        if sequence != runtime.sequence.get() {
                            continue;
                        }
                        let lease = runtime.lease.borrow().clone();
                        let Some(lease) = lease else {
                            continue;
                        };
                        let mut state = state;
                        let completion =
                            controller::choose(&mut state.write(), &lease, revision, index);
                        runtime.retire(state);
                        if let Some(completion) = completion {
                            let value =
                                serde_json::to_value(completion).expect("serializable completion");
                            let _=eval.send(json!({"kind":"insert","sequence":sequence,"revision":revision,"index":index,
                                "value":value["value"],"label":value["label"],"suffix":" "}));
                        } else {
                            let _=eval.send(json!({"kind":"rejected","sequence":sequence,"revision":revision,"index":index}));
                        }
                        shown.set(Popup {
                            sequence,
                            ..Popup::default()
                        });
                    }
                    Event::Inserted { sequence, accepted } => {
                        if sequence == runtime.sequence.get() && !accepted {
                            shown.set(Popup {sequence,status:"The suggestion could not be inserted. Review the comment before continuing.".into(),..Popup::default()});
                        }
                    }
                }
            }
            runtime.stop(state);
            shown.set(Popup::default());
        }
    });
    let view = shown();
    rsx! {
        div { class:"slack-list-channel-completions",
            "data-query-kind":view.trigger.name(),
            "data-query-sequence":view.sequence.to_string(),
            "data-query-revision":view.revision.to_string(),
            if !view.labels.is_empty() {
                div { id:"slack-list-channel-choices-{generation}",class:"slack-list-channel-options",role:"listbox","aria-label":match view.trigger { Trigger::Channel=>"Channel suggestions",Trigger::Member=>"Mention suggestions" },
                    for (index,label) in view.labels.iter().enumerate() {
                        button { key:"{view.sequence}-{view.revision}-{index}",id:"slack-list-channel-choice-{generation}-{view.sequence}-{index}",r#type:"button",role:"option",
                            class:if index==view.selected {"slack-list-channel-option selected"} else {"slack-list-channel-option"},
                            "aria-selected":(index==view.selected).to_string(),tabindex:"-1",
                            onmousedown:move |e|e.prevent_default(),
                            onclick:{let runtime=runtime.clone();let sequence=view.sequence;let revision=view.revision;move |_| {
                                if let Some(eval)=runtime.eval.borrow().as_ref() { let _=eval.send(json!({"kind":"choose","sequence":sequence,"revision":revision,"index":index})); }
                            }},
                            {icon(match view.trigger { Trigger::Channel=>Icon::Tag,Trigger::Member=>Icon::AlternateEmail },"icon slack-list-channel-icon")}
                            span { class:"slack-list-channel-option-label","{label}" }
                        }
                    }
                }
            }
            if !view.status.is_empty() { p { class:"slack-list-channel-status",role:"status","{view.status}" } }
        }
    }
}
fn publish(
    eval: dioxus::document::Eval,
    mut shown: Signal<Popup>,
    trigger: Trigger,
    choices: controller::Presentation,
) {
    let count = choices.labels.len();
    if eval.send(json!({"kind":"choices","sequence":choices.sequence,"revision":choices.revision,"count":count})).is_err() { return; }
    shown.set(Popup {
        trigger,
        sequence: choices.sequence,
        revision: choices.revision,
        labels: choices.labels,
        selected: 0,
        status: if count == 0 {
            trigger.empty().into()
        } else {
            String::new()
        },
    });
}

fn valid_query(trigger: Trigger, before: &str, term: &str, index: usize) -> bool {
    before.len() <= 1_048_576
        && term.len() <= 1_048_576
        && match trigger {
            Trigger::Channel => matches!(term.chars().next(), Some('#' | '\u{ff03}')),
            Trigger::Member => matches!(term.chars().next(), Some('@' | '\u{ff20}')),
        }
        && index.checked_add(term.encode_utf16().count()) == Some(before.encode_utf16().count())
        && before.ends_with(term)
}
#[cfg(test)]
mod tests {
    use super::Trigger;
    #[test]
    fn raw_query_preserves_prefix_and_utf16_boundary() {
        assert!(super::valid_query(
            Trigger::Channel,
            "\u{1f642} #Old name",
            "#Old name",
            3
        ));
        assert!(super::valid_query(
            Trigger::Channel,
            "\u{ff03}",
            "\u{ff03}",
            0
        ));
        assert!(!super::valid_query(
            Trigger::Channel,
            "\u{1f642} #Old name",
            "#Old name",
            2
        ));
        assert!(!super::valid_query(Trigger::Channel, "#name", "name", 1));
        assert!(!super::valid_query(Trigger::Channel, "@name", "@name", 0));
        assert!(super::valid_query(
            Trigger::Member,
            "\u{1f642} @Ada Lovelace",
            "@Ada Lovelace",
            3
        ));
        assert!(super::valid_query(
            Trigger::Member,
            "\u{ff20}",
            "\u{ff20}",
            0
        ));
        assert!(!super::valid_query(Trigger::Member, "#name", "#name", 0));
        assert!(!super::valid_query(
            Trigger::Member,
            "\u{1f642} @Ada",
            "@Ada",
            2
        ));
    }
}
