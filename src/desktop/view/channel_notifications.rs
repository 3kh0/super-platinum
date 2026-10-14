//! Current channel mode and Advanced device overrides; every activation is a
//! separately guarded preference change, sharing mute's pending receipt.
use crate::{
    channel_mute::settings::{self, Action, Mode},
    state::ShellState,
};
use dioxus::prelude::*;
#[component]
pub(crate) fn ChannelNotifications(
    state: Signal<ShellState>,
    channel: String,
    disabled: bool,
) -> Element {
    let alive = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(true)));
    let drop_alive = alive.clone();
    use_drop(move || drop_alive.set(false));
    let mut expanded = use_signal(|| false);
    let mut mobile_expanded = use_signal(|| false);
    let view = settings::presentation(&state.read(), &channel);
    let open_alive = alive.clone();
    let change_alive = alive.clone();
    let token = view.as_ref().map(|v| v.token.clone());
    let locked = disabled || view.as_ref().is_none_or(|v| !v.enabled);
    let onaction = use_callback(move |action: Action| {
        if change_alive.get() && !locked {
            if let Some(token) = token.as_ref() {
                settings::change(state, token.clone(), action);
            }
        }
    });
    let Some(view) = view else {
        return rsx! {};
    };
    rsx! {
        section{class:"channel-notifications","aria-label":"Notification preferences",
            button{id:"channel-notifications-open",r#type:"button",class:"channel-notifications-disclosure","aria-expanded":expanded(),"aria-controls":"channel-notifications-settings",disabled,
                onclick:move |_|if open_alive.get()&&!disabled{expanded.set(!expanded());},if expanded(){"Hide notification settings"}else{"Notification settings"}}
            if expanded(){
                div{id:"channel-notifications-settings",
                    if let Some(s)=view.settings {
                        div{class:"channel-notifications-group",role:"group","aria-labelledby":"channel-notifications-mode-label",
                            h3{id:"channel-notifications-mode-label","Notify me about"}
                            div{class:"channel-notifications-modes",
                                button{id:"channel-notifications-everything",r#type:"button",disabled:locked,"aria-pressed":!view.muted&&s.desktop==Mode::Everything,onclick:move |_|onaction.call(Action::Mode(Mode::Everything)),"All new posts"}
                                button{id:"channel-notifications-mentions",r#type:"button",disabled:locked,"aria-pressed":!view.muted&&s.desktop==Mode::Mentions,onclick:move |_|onaction.call(Action::Mode(Mode::Mentions)),"Just mentions"}
                            }
                            if view.muted {p{class:"scheduled-hint","Muted. Choosing a mode unmutes this channel."}}
                        }
                        div{class:"channel-notifications-group",
                            h3{"How to notify me"}
                            Toggle{id:"channel-notifications-desktop",label:"Desktop notifications",checked:s.desktop_push,disabled:locked,onchange:move |v|onaction.call(Action::DesktopPush(v))}
                            Toggle{id:"channel-notifications-mobile",label:"Mobile notifications",checked:s.mobile_enabled,disabled:locked,onchange:move |v|onaction.call(Action::MobilePush(v))}
                            p{class:"scheduled-hint","These controls save your Slack preferences. Banner delivery also depends on your system notification permissions."}
                        }
                        if !view.muted {
                            if s.mobile_enabled {
                                button { id:"channel-notifications-mobile-disclosure", r#type:"button", class:"channel-notifications-disclosure", "aria-expanded":mobile_expanded(), onclick:move |_|mobile_expanded.set(!mobile_expanded()), if mobile_expanded(){"Hide advanced mobile overrides"}else{"Show advanced mobile overrides"} }
                            }
                            if s.mobile_enabled && mobile_expanded() {
                                div{class:"channel-notifications-group",role:"group","aria-labelledby":"channel-notifications-mobile-label",
                                    h3{id:"channel-notifications-mobile-label","Mobile override"}
                                    div{class:"channel-notifications-modes",
                                        button{id:"channel-notifications-mobile-everything",r#type:"button",disabled:locked,"aria-pressed":s.mobile==Mode::Everything,onclick:move |_|onaction.call(Action::MobileOverride(Mode::Everything)),"All new posts"}
                                        button{id:"channel-notifications-mobile-mentions",r#type:"button",disabled:locked,"aria-pressed":s.mobile==Mode::Mentions,onclick:move |_|onaction.call(Action::MobileOverride(Mode::Mentions)),"Just mentions"}
                                    }
                                    if s.has_different_mobile {p{class:"scheduled-hint","Uses a different setting from desktop."}}else{p{class:"scheduled-hint","Same as desktop."}}
                                }
                            }
                            div{class:"channel-notifications-group",
                                h3{"Also notify me about"}
                                Toggle{id:"channel-notifications-threads",label:"Follow every thread",checked:s.follow_all_threads,disabled:locked,onchange:move |v|onaction.call(Action::FollowAllThreads(v))}
                                Toggle{id:"channel-notifications-broadcasts",label:"Get broadcast messages",checked:!s.suppress_at_channel,disabled:locked,onchange:move |v:bool|onaction.call(Action::SuppressAtChannel(!v))}
                                if s.desktop==Mode::Everything {Toggle{id:"channel-notifications-badges",label:"Badge every new message",checked:s.badge_all_unreads,disabled:locked,onchange:move |v|onaction.call(Action::BadgeAllUnreads(v))}}
                            }
                        }
                        if !s.desktop_push&&!s.mobile_enabled {p{class:"scheduled-hint",role:"status","Desktop and mobile banners are off. Your message notification filter is still saved."}}
                    }else {p{role:"status",class:"scheduled-hint","{view.message.as_deref().unwrap_or_default()}"}}
                }
            }
        }
    }
}
#[component]
fn Toggle(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {div{class:"channel-notifications-row",
        span{id:"{id}-label","{label}"}
        button{id,r#type:"button",role:"switch","aria-checked":checked,"aria-labelledby":"{id}-label",class:"channel-notifications-toggle",disabled,onclick:move |_|onchange.call(!checked),if checked{"On"}else{"Off"}}
    }}
}
