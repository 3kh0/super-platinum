use super_platinum_core::slack::events::RtEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    title: String,
    body: String,
}

pub fn for_event(
    core: &super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<DesktopNotification> {
    let RtEvent::Message(message) = event else {
        return None;
    };
    let workspace = core.workspaces.get(team)?;
    if generation != workspace.rt_generation
        || message.user.as_deref() == Some(workspace.self_user_id.as_str())
    {
        return None;
    }
    let channel_id = message.channel.as_deref()?;
    if core.active_team.as_deref() == Some(team)
        && core.active_channel.as_deref() == Some(channel_id)
    {
        return None;
    }
    let direct = workspace
        .channels
        .get(channel_id)
        .is_some_and(|channel| channel.is_im || channel.is_mpim);
    if !direct
        && !mentions(message, &workspace.self_user_id)
        && !workspace
            .usergroups
            .values()
            .any(|group| group.includes(&workspace.self_user_id) && group.mentioned_in(message))
    {
        return None;
    }
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
    let body = super_platinum_core::state::message_text(message);
    Some(DesktopNotification {
        title,
        body: if body.trim().is_empty() {
            "[message]".into()
        } else {
            body
        },
    })
}

fn mentions(message: &super_platinum_core::slack::models::Message, user: &str) -> bool {
    let encoded = format!("<@{user}>");
    message
        .text
        .as_deref()
        .is_some_and(|text| text.contains(&encoded))
        || message
            .blocks
            .iter()
            .any(|value| value_mentions(value, user, &encoded))
}

fn value_mentions(value: &serde_json::Value, user: &str, encoded: &str) -> bool {
    match value {
        serde_json::Value::String(value) => value == user || value.contains(encoded),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| value_mentions(value, user, encoded)),
        serde_json::Value::Object(map) => {
            matches!(map.get("user_id").or_else(|| map.get("user")).and_then(serde_json::Value::as_str), Some(found) if found == user)
                || map
                    .values()
                    .any(|value| value_mentions(value, user, encoded))
        }
        _ => false,
    }
}

pub async fn show(notification: DesktopNotification) {
    if let Err(error) = tokio::task::spawn_blocking(move || show_blocking(&notification)).await {
        eprintln!("super-platinum: notification worker stopped: {error}");
    }
}

#[cfg(target_os = "macos")]
fn show_blocking(notification: &DesktopNotification) {
    let status = std::process::Command::new("osascript")
        .args(["-e", "on run argv", "-e", "display notification (item 2 of argv) with title (item 1 of argv) sound name \"default\"", "-e", "end run", "--"])
        .arg(&notification.title)
        .arg(&notification.body)
        .status();
    report_status(status);
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
mod tests {
    use super::*;

    #[test]
    fn detects_mentions_in_text_and_blocks() {
        let mut message = super_platinum_core::slack::models::Message {
            text: Some("hello <@U1>".into()),
            ..Default::default()
        };
        assert!(mentions(&message, "U1"));
        message.text = None;
        message.blocks = vec![serde_json::json!({"type":"user","user_id":"U1"})];
        assert!(mentions(&message, "U1"));
        assert!(!mentions(&message, "U2"));
    }
}
