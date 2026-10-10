use std::collections::{HashSet, VecDeque};
use super_platinum_core::slack::events::RtEvent;
use super_platinum_core::state::{NotificationOptions, cmp_ts, now_secs};

#[cfg(target_os = "macos")]
mod macos;
mod sounds;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    title: String,
    body: String,
    sound: Option<String>,
}

/// Survives socket reconnects, and bounds memory even in busy workspaces.
#[derive(Default)]
pub struct NotificationTracker {
    seen: HashSet<(String, String, String)>,
    order: VecDeque<(String, String, String)>,
}

#[derive(Clone, Copy)]
pub struct NotificationView<'a> {
    pub focused: bool,
    pub thread_root: Option<&'a str>,
}

impl NotificationTracker {
    #[cfg(test)]
    pub fn for_event(
        &mut self,
        core: &super_platinum_core::CoreAppState,
        team: &str,
        generation: u64,
        event: &RtEvent,
    ) -> Option<DesktopNotification> {
        self.for_event_in_view(
            core,
            team,
            generation,
            event,
            NotificationView {
                focused: true,
                thread_root: None,
            },
        )
    }

    pub fn for_event_in_view(
        &mut self,
        core: &super_platinum_core::CoreAppState,
        team: &str,
        generation: u64,
        event: &RtEvent,
        view: NotificationView<'_>,
    ) -> Option<DesktopNotification> {
        let notification = for_event_in_view(core, team, generation, event, view)?;
        let (channel, identity) = match event {
            RtEvent::Message(m) | RtEvent::DesktopNotification(m) => {
                (m.channel.as_ref()?, m.ts.as_ref()?)
            }
            RtEvent::HuddleInvite(invite) => (&invite.channel_id, &invite.call_id),
            _ => return None,
        };
        let key = (team.to_owned(), channel.clone(), identity.clone());
        if !self.seen.insert(key.clone()) {
            return None;
        }
        self.order.push_back(key);
        if self.order.len() > 4096 {
            self.seen.remove(&self.order.pop_front().unwrap());
        }
        if std::env::var_os("SUPER_PLATINUM_RT_TRACE").is_some() {
            eprintln!(
                "notification: team={team} channel={channel} identity={identity} sound={}",
                notification.sound.as_deref().unwrap_or("silent")
            );
        }
        Some(notification)
    }
}

#[cfg(test)]
fn for_event(
    core: &super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<DesktopNotification> {
    for_event_in_view(
        core,
        team,
        generation,
        event,
        NotificationView {
            focused: true,
            thread_root: None,
        },
    )
}

fn for_event_in_view(
    core: &super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: &RtEvent,
    view: NotificationView<'_>,
) -> Option<DesktopNotification> {
    if let RtEvent::HuddleInvite(invite) = event {
        return huddle_invite(core, team, generation, invite);
    }
    let message = match event {
        RtEvent::Message(message) => message,
        RtEvent::DesktopNotification(message)
            if message
                .thread_ts
                .as_deref()
                .is_some_and(|root| message.ts.as_deref() != Some(root)) =>
        {
            message
        }
        _ => return None,
    };
    let workspace = core.workspaces.get(team)?;
    if generation != workspace.rt_generation
        || message.user.as_deref() == Some(workspace.self_user_id.as_str())
    {
        return None;
    }
    let channel_id = message.channel.as_deref()?;
    let ts = message.ts.as_deref()?;
    let thread = message.thread_ts.as_deref().filter(|root| *root != ts);
    if view.focused
        && core.active_team.as_deref() == Some(team)
        && core.active_channel.as_deref() == Some(channel_id)
        && (thread.is_none() || view.thread_root == thread)
    {
        return None;
    }
    let options = workspace
        .notifications
        .message_options(workspace, message, now_secs())?;
    let held = thread
        .and_then(|root| {
            core.threads
                .get(&(team.into(), channel_id.into(), root.into()))
        })
        .or_else(|| workspace.messages.get(channel_id));
    if let Some(held) = held {
        if cmp_ts(Some(ts), held.last_read.as_deref()).is_le() {
            return None;
        }
        // Server notifications can follow the message frame; the tracker
        // dedupes actual deliveries. A replayed message must not re-announce.
        if matches!(event, RtEvent::Message(_))
            && held.messages.iter().any(|m| m.ts.as_deref() == Some(ts))
        {
            return None;
        }
    }
    if thread.is_none()
        && workspace
            .channels
            .get(channel_id)
            .is_some_and(|channel| cmp_ts(Some(ts), channel.last_read.as_deref()).is_le())
    {
        return None;
    }
    let direct = workspace
        .channels
        .get(channel_id)
        .is_some_and(|channel| channel.is_im || channel.is_mpim);
    let author = super_platinum_core::state::message_author_name(workspace, message);
    let title = if direct {
        author
    } else {
        let channel = workspace
            .channels
            .get(channel_id)
            .map(|channel| super_platinum_core::state::channel_display_name(workspace, channel))
            .unwrap_or_else(|| channel_id.to_owned());
        format!("{author} in #{channel}")
    };
    let body = if options.preview {
        preview_text(workspace, message)
    } else {
        "New message".into()
    };
    Some(DesktopNotification {
        title,
        sound: options.sound,
        body: if body.trim().is_empty() {
            "[message]".into()
        } else {
            body
        },
    })
}

fn preview_text(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
) -> String {
    if message.kind.as_deref() == Some("desktop_notification") {
        return message.text.clone().unwrap_or_default();
    }
    // Reuse the transcript's typed text renderer to resolve mentions and emoji.
    // This temporary registry has no disk store and never fetches images.
    let media = crate::media::MediaRegistry::default();
    let context = crate::blocks::BlockCtx::new(workspace, &media);
    let mut nodes = message
        .blocks
        .iter()
        .flat_map(|block| crate::blocks::block_nodes(context, block))
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        nodes = crate::blocks::mrkdwn_inline(
            context,
            &super_platinum_core::state::message_text(message),
        );
    }
    crate::model::RichNode::Paragraph(nodes).plain_text()
}

/// A ring gets a notification wherever the reader is: unlike a message, it
/// is gone in half a minute, and the app may be behind another window.
fn huddle_invite(
    core: &super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    invite: &super_platinum_core::slack::models::HuddleInvite,
) -> Option<DesktopNotification> {
    let workspace = core.workspaces.get(team)?;
    if generation != workspace.rt_generation || core.huddle.is_in(team, &invite.channel_id) {
        return None;
    }
    let NotificationOptions { sound, .. } =
        workspace
            .notifications
            .huddle_options(workspace, &invite.channel_id, now_secs())?;
    let caller = invite
        .sender_user_id
        .as_deref()
        .map(|user| workspace.display_name(user))
        .unwrap_or_else(|| "Someone".into());
    let place = workspace
        .channels
        .get(&invite.channel_id)
        .filter(|channel| !channel.is_im)
        .map(|channel| {
            let name = super_platinum_core::state::channel_display_name(workspace, channel);
            if channel.is_mpim {
                format!(" with {name}")
            } else {
                format!(" in #{name}")
            }
        })
        .unwrap_or_default();
    Some(DesktopNotification {
        title: format!("{caller} is inviting you to a huddle"),
        body: format!("Join the huddle{place} from Super Platinum"),
        sound,
    })
}

pub async fn show(notification: DesktopNotification) {
    if let Err(error) = tokio::task::spawn_blocking(move || show_blocking(&notification)).await {
        eprintln!("super-platinum: notification worker stopped: {error}");
    }
}

pub async fn show_preview() {
    show(DesktopNotification {
        title: "Super Platinum".into(),
        body: "Notifications now come from Super Platinum.".into(),
        sound: None,
    })
    .await;
}

#[cfg(target_os = "macos")]
fn show_blocking(notification: &DesktopNotification) {
    macos::show(notification);
}

#[cfg(target_os = "linux")]
fn show_blocking(notification: &DesktopNotification) {
    report_status(
        std::process::Command::new("notify-send")
            .args([
                "--app-name=Super Platinum",
                "--icon=super-platinum",
                "--category=im.received",
            ])
            .arg(&notification.title)
            .arg(&notification.body)
            .status(),
    );
}

#[cfg(target_os = "windows")]
fn show_blocking(notification: &DesktopNotification) {
    let title = xml_escape(&notification.title);
    let body = xml_escape(&notification.body);
    let xml = format!(
        "<toast><visual><binding template='ToastGeneric'><text>{title}</text><text>{body}</text></binding></visual><audio src='ms-winsoundevent:Notification.IM'/></toast>"
    );
    let script = "$xml=New-Object Windows.Data.Xml.Dom.XmlDocument;$xml.LoadXml($args[0]);$toast=[Windows.UI.Notifications.ToastNotification]::new($xml);[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('com.echonet.superplatinum').Show($toast)";
    report_status(
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script, &xml])
            .status(),
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn show_blocking(_notification: &DesktopNotification) {}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn report_status(status: std::io::Result<std::process::ExitStatus>) {
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("super-platinum: notification helper exited with {status}"),
        Err(error) => eprintln!("super-platinum: notification helper failed: {error}"),
    }
}

#[cfg(target_os = "windows")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn ensure_identity() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    macos::initialize()?;
    #[cfg(target_os = "windows")]
    {
        let key = r"HKCU\SOFTWARE\Classes\AppUserModelId\com.echonet.superplatinum";
        let commands = [
            [
                "add",
                key,
                "/v",
                "DisplayName",
                "/t",
                "REG_SZ",
                "/d",
                "Super Platinum",
                "/f",
            ],
            [
                "add",
                key,
                "/v",
                "IconBackgroundColor",
                "/t",
                "REG_SZ",
                "/d",
                "0",
                "/f",
            ],
        ];
        for args in commands {
            let status = std::process::Command::new("reg.exe")
                .args(args)
                .status()
                .map_err(|error| error.to_string())?;
            if !status.success() {
                return Err(format!("reg.exe exited with {status}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
