//! One avatar-only People renderer for Table, Board and record details.
use super::*;
use crate::{media::MediaUri, slack_lists::PeopleReference};
fn ready_avatar(shell: &ShellState, person: &PeopleReference, epoch: u64) -> Option<String> {
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    let url = workspace.avatar_url(&person.user_id)?;
    let asset = shell.media.register_avatar(&person.user_id, &url);
    shell
        .media
        .is_ready(&asset)
        .then(|| asset.webview_uri_at(epoch))
}
fn initial(label: &str) -> String {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}
#[component]
pub(super) fn PeopleAvatars(
    state: Signal<ShellState>,
    people: Vec<PeopleReference>,
    media_epoch: u64,
) -> Element {
    // Subscribe to current metadata rather than caching image URLs in row Arcs.
    // Avatar-only changes can leave the name/Find projection entirely unchanged.
    let shell = state.read();
    rsx! {div{class:"slack-list-people-avatars",
        {people.into_iter().enumerate().map(|(index,person)|{
            let label=person.label.clone();let image=ready_avatar(&shell,&person,media_epoch);let fallback=initial(&label);
            let face=rsx!{if let Some(src)=image{img{src,alt:"","aria-hidden":"true",draggable:"false"}}else{span{"aria-hidden":"true","{fallback}"}}};
            let user=person.user_id.clone();let hover=user.clone();
            rsx!{div{key:"{index}-{user}",class:"slack-list-people-avatar-slot",
                if person.known {button{r#type:"button",class:"slack-list-people-avatar",title:"{label}","aria-label":"{label}",
                    onmouseenter:move|event:MouseEvent|{let point=event.data().client_coordinates();crate::profile::show_profile_hover(state,hover.clone(),point.x,point.y);},
                    onmouseleave:move|_|crate::profile::schedule_profile_hover_close(state),
                    onclick:move|_|{spawn(crate::bootstrap::open_profile(state,user.clone()));},{face}}}
                else{span{class:"slack-list-people-avatar",role:"img",title:"{label}","aria-label":"{label}",{face}}}
            }}
        })}
    }}
}
#[cfg(test)]
mod tests;
