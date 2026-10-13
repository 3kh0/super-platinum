//! Disconnected review of button visibility, identity and accessible names.

pub(crate) fn apply(state: &mut crate::state::ShellState) {
    use serde_json::json;
    let groups = [
        (
            "Release approval",
            "release-a",
            json!([
                {"type":"button","text":{"type":"plain_text","text":"Approve"},
                 "action_id":"approve","value":"release-a","visible_to_user_ids":["U0"],
                 "accessibility_label":"Approve release A", "style":"primary",
                 "confirm":{"title":{"type":"plain_text","text":"Approve release?"},
                 "text":{"type":"mrkdwn","text":"This approves *release A*."},
                 "confirm":{"type":"plain_text","text":"Approve"},
                 "deny":{"type":"plain_text","text":"Cancel"},"style":"primary"}},
                {"type":"button","text":{"type":"plain_text","text":"Hidden other user"},
                 "action_id":"private","visible_to_user_ids":["U1"]},
                {"type":"button","text":{"type":"plain_text","text":"Hidden malformed audience"},
                 "action_id":"invalid","visible_to_user_ids":["U0",false]},
                {"type":"button","text":{"type":"plain_text","text":"Release notes"},
                 "action_id":"notes","url":"https://example.test/release-notes",
                 "accessibility_label":"Read release A notes"}
            ]),
        ),
        (
            "Second release",
            "release-b",
            json!([
                {"type":"button","text":{"type":"plain_text","text":"Approve"},
                 "action_id":"approve","value":"release-b","accessibility_label":"Approve release B"},
                {"type":"button","text":{"type":"plain_text","text":"Details"},
                 "action_id":"details"},
                {"type":"static_select","action_id":"status",
                 "initial_option":{"text":{"type":"plain_text","text":"Ready"},"value":"ready"}}
            ]),
        ),
    ];
    let workspace = &state.core.workspaces["T1"];
    let messages = groups
        .into_iter()
        .enumerate()
        .map(|(index, (title, block_id, elements))| {
            let message = serde_json::from_value(json!({
                "ts":format!("1719800{:03}.000000",index * 60), "user":"U1", "bot_id":"B1",
                "blocks":[{"type":"section","text":{"type":"plain_text","text":title}},
                           {"type":"actions","block_id":block_id,"elements":elements}]
            }))
            .expect("button metadata fixture");
            crate::message_vm::message_vm(workspace, &message, &state.media)
        })
        .collect();
    state.install_fixture_messages(messages);
}

/// Exact named-fixture gate; this checker only reads the mounted DOM and never
/// clicks a button, opens a URL, or enters the Slack interaction path.
pub(crate) fn check_native(desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_BUTTON_METADATA_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("button-metadata" | "button-metadata-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        for attempt in 1..=20 {
            let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
            let result =
                desktop
                    .webview
                    .evaluate_script_with_callback(CHECK_SCRIPT, move |value| {
                        let _ = send.send(value);
                    });
            if let Err(error) = result {
                eprintln!(
                    "button-metadata-check: {}",
                    serde_json::json!({
                        "ok":false,"error":format!("evaluation request failed: {error}"),"attempt":attempt
                    })
                );
                return;
            }
            let raw = tokio::time::timeout(std::time::Duration::from_secs(1), receive.recv()).await;
            let receipt = match raw {
                Ok(Some(raw)) => serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|value| match value {
                        serde_json::Value::String(encoded) => serde_json::from_str(&encoded).ok(),
                        value => Some(value),
                    }),
                _ => None,
            };
            if let Some(mut receipt) = receipt.filter(serde_json::Value::is_object) {
                let ready = receipt["ready"] == true;
                if ready || attempt == 20 {
                    receipt["attempt"] = serde_json::json!(attempt);
                    eprintln!("button-metadata-check: {receipt}");
                    return;
                }
            } else if attempt == 20 {
                eprintln!(
                    "button-metadata-check: {}",
                    serde_json::json!({
                        "ok":false,"error":"No valid native DOM receipt","attempt":attempt
                    })
                );
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    });
}

const CHECK_SCRIPT: &str = r#"(() => {
    try {
        const buttons = [...document.querySelectorAll('.block-button')];
        const rows = [...document.querySelectorAll('.block-actions')];
        const controls = buttons.map(button => ({
            text: button.textContent.trim(), aria: button.getAttribute('aria-label'),
            disabled: button.disabled, width: button.getBoundingClientRect().width,
            height: button.getBoundingClientRect().height,
            marginTop: getComputedStyle(button).marginTop,
            radius: getComputedStyle(button).borderTopLeftRadius
        }));
        const expected = [
            ['Approve', 'Approve release A', true],
            ['Release notes', 'Read release A notes', false],
            ['Approve', 'Approve release B', true],
            ['Details', 'Details', true], ['Ready', 'Ready', true]
        ];
        const groups = rows.map(row => {
            const bounds = row.getBoundingClientRect();
            const children = [...row.querySelectorAll('.block-button')];
            return {width: bounds.width, clientWidth: row.clientWidth, scrollWidth: row.scrollWidth,
                overflow: row.scrollWidth > row.clientWidth + 1,
                childrenFit: children.every(button => {
                    const rect = button.getBoundingClientRect();
                    return rect.width > 0 && rect.left >= bounds.left - 1 && rect.right <= bounds.right + 1;
                })};
        });
        const ready = rows.length === 2 && buttons.length >= 5;
        const labelsMatch = controls.length === expected.length && expected.every((want, index) => {
            const got = controls[index];
            return got.text === want[0] && got.aria === want[1] && got.disabled === want[2];
        });
        const hiddenAbsent = !document.body.textContent.includes('Hidden other user')
            && !document.body.textContent.includes('Hidden malformed audience');
        const noOverflow = groups.length === 2 && groups.every(group => !group.overflow && group.childrenFit);
        const controlHeight = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--control-height'));
        const controlRadius = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--radius-md'));
        const compactHeights = controls.every(button => Math.abs(button.height - controlHeight) < 1 && button.marginTop === '0px' && parseFloat(button.radius) === controlRadius);
        return {ok: ready && labelsMatch && hiddenAbsent && noOverflow && compactHeights, ready, labelsMatch,
            hiddenAbsent, noOverflow, compactHeights, controls, groups, viewport: [innerWidth, innerHeight]};
    } catch (error) {
        return {ok: false, ready: true, error: String(error)};
    }
})()"#;
