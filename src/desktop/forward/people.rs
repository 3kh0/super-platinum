//! Forwarding shares directory lookup with the new-message picker, without navigating.
use super::*;
use crate::new_dm::{Person, directory::Directory};
use super_platinum_core::{direct_messages::Recipients, slack::models::Channel};

pub(super) fn matches(
    shell: &ShellState,
    query: &str,
    excluded: &[Destination],
    directory: &Directory,
) -> Vec<Destination> {
    let Some(ws) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return vec![];
    };
    let query = query.trim();
    let only_people = query.starts_with('@');
    let only_channels = query.starts_with('#');
    let mut result = if only_people {
        vec![]
    } else {
        destinations(shell, query, excluded)
    };
    result.retain(|d| {
        !ws.channels
            .get(&d.id)
            .and_then(|c| c.user.as_ref())
            .is_some_and(|id| {
                excluded
                    .iter()
                    .any(|e| e.person.as_ref().is_some_and(|p| &p.id == id))
            })
    });
    if only_channels {
        result.retain(|d| !d.dm);
        return result;
    }
    if query.is_empty() {
        return result;
    }
    let selected = excluded
        .iter()
        .filter_map(|d| d.person.clone())
        .collect::<Vec<_>>();
    let people = directory.append(
        shell,
        query,
        &selected,
        crate::new_dm::people(ws, query, &selected),
    );
    let dms = ws
        .channels
        .values()
        .filter(|c| c.is_im && !c.is_archived)
        .filter_map(|c| c.user.as_deref().map(|user| (user, c)))
        .collect::<std::collections::HashMap<_, _>>();
    for person in people {
        let destination = if let Some(channel) = dms.get(person.id.as_str()) {
            Destination {
                id: channel.id.clone(),
                name: person.name.clone(),
                private: true,
                dm: true,
                person: None,
            }
        } else {
            Destination {
                id: format!("person:{}", person.id),
                name: person.name.clone(),
                private: true,
                dm: true,
                person: Some(person),
            }
        };
        if !excluded.iter().any(|d| d.id == destination.id)
            && !result.iter().any(|d| d.id == destination.id)
        {
            result.push(destination);
        }
        if result.len() >= 30 {
            break;
        }
    }
    result.truncate(30);
    result
}

pub(super) fn choose(
    mut state: Signal<ShellState>,
    mut selected: Signal<Vec<Destination>>,
    directory: Signal<Directory>,
    destination: Destination,
    mut error: Signal<String>,
) -> bool {
    if selected.peek().iter().any(|d| d.id == destination.id) {
        return false;
    }
    if let Some(person) = &destination.person {
        if !directory.peek().hydrate(&mut state.write(), &person.id) {
            error.set("That person is no longer available. Search again.".into());
            return false;
        }
    }
    selected.write().push(destination);
    true
}

/// Validate the private DM identity before any message is shared. Never change
/// navigation, read state, or the source conversation while resolving a recipient.
fn remember(
    shell: &mut ShellState,
    mut channel: Channel,
    person: &Person,
) -> Result<Destination, String> {
    if !channel.id.starts_with('D')
        || channel.id.len() < 2
        || !channel.id.bytes().all(|b| b.is_ascii_alphanumeric())
        || channel.is_archived
        || channel.is_mpim
        || channel.user.as_ref().is_some_and(|id| id != &person.id)
    {
        return Err("Slack returned an unexpected direct conversation. Nothing was forwarded to this person.".into());
    }
    let Some(ws) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get_mut(t))
    else {
        return Err("The active workspace changed.".into());
    };
    if ws
        .channels
        .get(&channel.id)
        .is_some_and(|c| !c.is_im || c.user.as_ref().is_some_and(|id| id != &person.id))
    {
        return Err(
            "The direct conversation could not be verified. Nothing was forwarded to this person."
                .into(),
        );
    }
    channel.is_im = true;
    channel.user = Some(person.id.clone());
    let id = channel.id.clone();
    ws.channels
        .entry(id.clone())
        .and_modify(|existing| {
            existing.user = Some(person.id.clone());
            existing.is_im = true;
            existing.is_archived = false;
        })
        .or_insert(channel);
    Ok(Destination {
        id,
        name: person.name.clone(),
        private: true,
        dm: true,
        person: None,
    })
}

pub(super) struct Failure {
    pub message: String,
    pub retry_after: Option<u64>,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

pub(super) async fn resolve(
    mut state: Signal<ShellState>,
    ctx: &Context,
    destination: Destination,
) -> Result<Destination, Failure> {
    let Some(person) = destination.person.as_ref() else {
        return Ok(destination);
    };
    {
        let shell = state.peek();
        let ws = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or("The workspace is unavailable.")?;
        if !ws.users.get(&person.id).is_some_and(|u| !u.deleted) {
            return Err("That person is no longer available. Search again.".into());
        }
        if let Some(channel) = ws
            .channels
            .values()
            .find(|c| c.is_im && !c.is_archived && c.user.as_ref() == Some(&person.id))
        {
            return Ok(Destination {
                id: channel.id.clone(),
                name: person.name.clone(),
                private: true,
                dm: true,
                person: None,
            });
        }
    }
    let recipients =
        Recipients::new(&[person.id.clone()], &ctx.session.user_id).map_err(str::to_owned)?;
    let channel = api::open_direct_conversation(&ctx.transport,&ctx.client,&ctx.session,&recipients).await.map_err(|error| {
        if let super_platinum_core::slack::Error::RateLimited {retry_after_secs}=error {
            let seconds=retry_after_secs.unwrap_or(30);
            Failure {message:format!("Slack asked you to wait {seconds} seconds before opening more DMs. Your remaining recipients are still selected."),retry_after:Some(seconds)}
        } else {Failure::from(format!("Could not open a DM with {}. Nothing was forwarded to this person. Try again after reconnecting or checking workspace permissions.",person.name))}
    })?;
    if !ctx.current(&state.peek()) {
        return Err("The active account changed.".into());
    }
    remember(&mut state.write(), channel, person).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "forward")
    }
    #[test]
    fn people_without_dms_are_found_and_known_dms_are_not_duplicated() {
        let mut shell = shell();
        let team = shell.core.active_team.clone().unwrap();
        shell.core.workspaces.get_mut(&team).unwrap().users.insert(
            "UNEW".into(),
            super_platinum_core::slack::models::User {
                id: "UNEW".into(),
                name: Some("new_handle".into()),
                real_name: Some("New Person".into()),
                ..Default::default()
            },
        );
        let directory = Directory::default();
        let result = matches(&shell, "@new_handle", &[], &directory);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].person.as_ref().unwrap().id, "UNEW");
        assert!(matches(&shell, "@new_handle", &result, &directory).is_empty());
        assert_eq!(
            matches(&shell, "maya", &[], &directory)
                .iter()
                .filter(|d| d.id == "D1")
                .count(),
            1
        );
        let maya = matches(&shell, "@maya", &[], &directory);
        assert_eq!(maya.len(), 1);
        assert!(maya[0].person.is_none());
        assert!(matches(&shell, "@maya", &maya, &directory).is_empty());
        assert!(matches(&shell, "#maya", &[], &directory).is_empty());
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .get_mut("UNEW")
            .unwrap()
            .deleted = true;
        assert!(matches(&shell, "new_handle", &[], &directory).is_empty());
    }
    #[test]
    fn dm_resolution_preserves_navigation_and_rejects_wrong_recipient() {
        let mut shell = shell();
        let active = shell.core.active_channel.clone();
        let target = shell.forward_target.as_ref().unwrap().id.clone();
        let person = Person {
            id: "UNEW".into(),
            name: "New Person".into(),
            handle: "new".into(),
        };
        for channel in [
            Channel {
                id: "CPUBLIC".into(),
                ..Default::default()
            },
            Channel {
                id: "DWRONG".into(),
                user: Some("OTHER".into()),
                ..Default::default()
            },
        ] {
            assert!(remember(&mut shell, channel, &person).is_err());
        }
        let dest = remember(
            &mut shell,
            Channel {
                id: "DNEW".into(),
                ..Default::default()
            },
            &person,
        )
        .unwrap();
        assert_eq!(dest.id, "DNEW");
        assert!(dest.person.is_none());
        assert_eq!(shell.core.active_channel, active);
        assert_eq!(shell.forward_target.as_ref().unwrap().id, target);
        assert_eq!(shell.overlay, Some(Overlay::Forward));
        let ws = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
        assert_eq!(ws.channels["DNEW"].user.as_deref(), Some("UNEW"));
    }
}
