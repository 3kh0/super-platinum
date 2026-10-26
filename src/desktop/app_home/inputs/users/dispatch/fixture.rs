//! Closed synthetic interaction with the production Home member picker.
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::static_select::fixture as base;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::users::fixture as people;
use crate::app_home::inputs::{self, static_select};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use people::{ADA, BLAIR, CASEY, REMOTE};
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputKind, InputValue};
mod channel;
mod conversation;
mod gestures;
mod helpers;
mod lanes;
mod multi_channel;
mod multi_conversation;
mod multiple;
mod navigation;
use gestures::*;
use helpers::refresh;
use helpers::*;
use navigation::{reset, revisit};

pub(crate) fn conversation_requested() -> bool {
    conversation::requested() || multi_conversation::requested()
}
pub(crate) fn conversation_named() -> bool {
    conversation_requested() && named()
}
pub(crate) fn channel_requested() -> bool {
    channel::requested() || multi_channel::requested() || conversation_requested()
}
pub(crate) fn channel_named() -> bool {
    channel_requested() && named()
}
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-conversation-dispatch"
                | "app-home-multi-conversation-dispatch-narrow"
                | "app-home-conversation-dispatch"
                | "app-home-conversation-dispatch-narrow"
                | "app-home-multi-channel-dispatch"
                | "app-home-multi-channel-dispatch-narrow"
                | "app-home-channel-dispatch"
                | "app-home-channel-dispatch-narrow"
                | "app-home-user-dispatch"
                | "app-home-user-dispatch-narrow"
                | "app-home-multi-user-dispatch"
                | "app-home-multi-user-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var(if multi_conversation::requested() {
            "SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_DISPATCH_CHECK"
        } else if conversation::requested() {
            "SUPER_PLATINUM_APP_HOME_CONVERSATION_DISPATCH_CHECK"
        } else if multi_channel::requested() {
            "SUPER_PLATINUM_APP_HOME_MULTI_CHANNEL_DISPATCH_CHECK"
        } else if channel::requested() {
            "SUPER_PLATINUM_APP_HOME_CHANNEL_DISPATCH_CHECK"
        } else if multiple::requested() {
            "SUPER_PLATINUM_APP_HOME_MULTI_USER_DISPATCH_CHECK"
        } else {
            "SUPER_PLATINUM_APP_HOME_USER_DISPATCH_CHECK"
        })
        .as_deref()
            == Ok("1")
        && matches!(
            case().as_str(),
            "selection" | "props" | "retirement" | "admission"
        )
}
pub(crate) fn case() -> String {
    std::env::var(if multi_conversation::requested() {
        "SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_DISPATCH_CASE"
    } else if conversation::requested() {
        "SUPER_PLATINUM_APP_HOME_CONVERSATION_DISPATCH_CASE"
    } else if multi_channel::requested() {
        "SUPER_PLATINUM_APP_HOME_MULTI_CHANNEL_DISPATCH_CASE"
    } else if channel::requested() {
        "SUPER_PLATINUM_APP_HOME_CHANNEL_DISPATCH_CASE"
    } else if multiple::requested() {
        "SUPER_PLATINUM_APP_HOME_MULTI_USER_DISPATCH_CASE"
    } else {
        "SUPER_PLATINUM_APP_HOME_USER_DISPATCH_CASE"
    })
    .unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    if channel_requested() {
        inputs::channels::fixture::install(s);
    } else {
        people::install(s);
    }
    // Replace only the unrelated, pristine setup form before any native mount.
    let previous = inputs::owner(s).unwrap();
    super::reset(s, &previous);
    if channel_requested() {
        inputs::channels::dispatch::reset(s, &previous);
    }
    inputs::native::reset(s, &previous);
    static_select::reset(s, &previous);
    s.app_home.forms.remove(&previous);
    s.app_home.input_revisions.remove(&previous);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw.as_object_mut().unwrap().remove("state");
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release reviewer"},"element":{"type":"users_select","action_id":"value","initial_user":ADA,"placeholder":{"type":"plain_text","text":"Find a person"}}},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required reviewer"},"element":{"type":"users_select","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet reviewer"},"element":{"type":"users_select","action_id":"value","initial_user":ADA}},
        {"type":"input","block_id":"multi","optional":true,"label":{"type":"plain_text","text":"Quiet reviewers"},"element":{"type":"multi_users_select","action_id":"value","initial_users":[ADA],"max_selected_items":2}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    if channel_requested() {
        channel::configure(&mut raw);
        if multi_conversation::requested() {
            multi_conversation::configure(&mut raw);
        } else if conversation::requested() {
            conversation::configure(&mut raw);
        } else if multi_channel::requested() {
            multi_channel::configure(&mut raw);
        }
    } else if multiple::requested() {
        multiple::configure(&mut raw);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    base::initialize(s);
    buttons::initialize(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(inputs::native::date::proof(s).as_object().unwrap().clone());
    if channel_requested() {
        x.as_object_mut().unwrap().extend(
            inputs::channels::dispatch::proof(s)
                .as_object()
                .unwrap()
                .clone(),
        );
        x["channelMode"] = json!(true);
        if conversation_requested() {
            x["conversationMode"] = json!(true);
            x["activeConversation"] =
                json!({"channel":s.core.active_channel,"generation":s.channel_generation});
        } else {
            x.as_object_mut().unwrap().remove("conversationWidgets");
        }
        if multi_channel::requested() {
            x["multiChannelMode"] = json!(true);
        } else if !conversation_requested() {
            x.as_object_mut().unwrap().remove("multiChannelWidgets");
        }
        if multi_conversation::requested() {
            x["multiConversationMode"] = json!(true);
        } else {
            x.as_object_mut()
                .unwrap()
                .remove("multiConversationWidgets");
        }
        let p = inputs::channels::fixture::snapshot(s);
        x["directory"] = json!({"requests":p["requests"],"releases":p["releases"],"pending":p["pending"],"query":p["query"],"cooling":p["cooling"]});
        x["pickerBlock"] = x["channelPickerBlock"].clone();
        x["channels"] = p["channels"].clone();
        x["channelSearchMode"] = p["mode"].clone();
        if conversation_requested() {
            x["members"] = p["members"].clone();
        }
    } else {
        x.as_object_mut()
            .unwrap()
            .extend(super::proof(s).as_object().unwrap().clone());
        if multiple::requested() {
            x["multiUserMode"] = json!(true);
        } else {
            x.as_object_mut().unwrap().remove("multiUserWidgets");
        }
        let p = people::snapshot(s);
        x["directory"] = json!({"requests":p["requests"],"releases":p["releases"],"pending":p["pending"],"query":p["query"],"cooling":p["cooling"]});
        x["pickerBlock"] = json!(
            s.app_home
                .users
                .picker
                .as_ref()
                .map(|p| &p.ticket.key.block_id)
        );
        x["multiVisible"] = json!(
            inputs::owner(s)
                .and_then(|o| s.app_home.users.multiples.get(&(o, key("multi"))))
                .map(|w| &w.visible)
        );
    }
    x["case"] = json!(case());
    x["buttonProcessing"] = actions::wrappers_fixture_processing(s);
    x
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let n = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &n)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":e,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":"User dispatch fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!(
                "{}: {result}",
                if multi_conversation::requested() {
                    "app-home-multi-conversation-dispatch-check"
                } else if conversation::requested() {
                    "app-home-conversation-dispatch-check"
                } else if multi_channel::requested() {
                    "app-home-multi-channel-dispatch-check"
                } else if channel::requested() {
                    "app-home-channel-dispatch-check"
                } else if multiple::requested() {
                    "app-home-multi-user-dispatch-check"
                } else {
                    "app-home-user-dispatch-check"
                }
            )
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let source = crate::app_home::context::fixture::publish(s, false, "TWORK0002").await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"source":source,"events":"actual typed picker DOM callbacks; synthetic events, no physical OS input","homeOpenedFixtureAdapter":true}),&mut stages,&mut captures).await?;
    if multi_conversation::requested() {
        multi_conversation::run(s, d, n, &mut stages, &mut captures).await?;
    } else if conversation::requested() {
        conversation::run(s, d, n, &mut stages, &mut captures).await?;
    } else if multi_channel::requested() {
        multi_channel::run(s, d, n, &mut stages, &mut captures).await?;
    } else if channel::requested() {
        channel::run(s, d, n, &mut stages, &mut captures).await?;
    } else if multiple::requested() {
        multiple::run(s, d, n, &mut stages, &mut captures).await?;
    } else {
        match case().as_str() {
            "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
            "props" => lanes::props(s, d, n, &mut stages, &mut captures).await?,
            "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
            "admission" => lanes::admission(s, d, n, &mut stages, &mut captures).await?,
            _ => return Err("Unknown user dispatch case".into()),
        }
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
