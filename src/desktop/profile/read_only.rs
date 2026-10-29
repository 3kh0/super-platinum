//! Deliberately limited details: no inferred presence, membership or actions.
use super::*;
use crate::bootstrap::profile_search::Presentation;

pub(super) fn pane(
    state: Signal<ShellState>,
    shell: &ShellState,
    details: Presentation<'_>,
) -> Element {
    let name = super_platinum_core::state::display_name(details.user, details.id);
    let profile = details.user.and_then(|u| u.profile.as_ref());
    let title = profile.and_then(|p| p.title.clone()).unwrap_or_default();
    let pronouns = profile.and_then(|p| p.pronouns.clone()).unwrap_or_default();
    let fields = [
        (
            "Email",
            profile.and_then(|p| p.email.clone()).unwrap_or_default(),
        ),
        (
            "Phone",
            profile.and_then(|p| p.phone.clone()).unwrap_or_default(),
        ),
    ];
    let initials = name
        .split_whitespace()
        .filter_map(|s| s.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let avatar = details
        .user
        .and_then(super_platinum_core::state::user_profile_image_url)
        .map(|url| shell.media.register_avatar(details.id, &url));
    let ready = avatar.as_ref().is_some_and(|id| shell.media.is_ready(id));
    let width = shell.profile_pane_width.max(200.0);
    let style = format!("--profile-pane-width:{width}px");
    rsx! {
        aside { class:if width<300.0 {"profile-pane profile-pane-narrow"} else {"profile-pane"},
            style:"{style}", role:"dialog", "aria-label":"{name} Profile", "data-profile-read-only":"true",
            {pane_header(state,width)}
            div {class:"profile-pane-scroll",
                div {class:"profile-pane-hero",div {class:"profile-pane-avatar",
                    {avatar_fields(&name,&initials,avatar.as_ref(),shell.media_epoch,"profile-pane-avatar-image",ready)}
                    if details.loading {span {class:"profile-avatar-loading"}}
                }}
                section {class:"profile-pane-section profile-main-section",
                    div {class:"profile-pane-name-row",h1 {"{name}"}}
                    if !title.is_empty() {p {class:"profile-title","{title}"}}
                    if !pronouns.is_empty() {p {class:"profile-pronouns","{pronouns}"}}
                    p {class:"profile-pronouns","Read-only profile details"}
                    if details.loading {p {class:"profile-loading",role:"status","Refreshing profile…"}}
                    if let Some(error)=details.error {p {class:"profile-error",role:"alert","{error}"}}
                    if let Some(token)=details.retry {
                        button {class:"chip",onclick:move |_|crate::bootstrap::profile_search::retry(state,token.clone()),"Refresh profile"}
                    }
                }
                for (label,value) in fields.into_iter().filter(|(_,value)|!value.is_empty()) {
                    section {class:"profile-pane-section profile-detail-section",
                        div {class:"profile-field",strong {"{label}"}span {"{value}"}}
                    }
                }
            }
        }
    }
}
