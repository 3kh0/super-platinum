//! Closed delay around one genuine final native capture, never a replacement.
use super::*;

const WRAPPER: &str = include_str!("late_capture.js");

pub(in crate::canvas::comments::reply) fn script(
    shell: &ShellState,
    binding: &Binding,
    options: &Value,
    bridge: &str,
) -> Option<String> {
    if !(cfg!(test) || cfg!(feature = "protected-native-fixture"))
        || !crate::fixture::is_fixture()
        || std::env::var("SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK").as_deref() != Ok("1")
        || std::env::var("SUPER_PLATINUM_CANVAS_DESTINATION_LATE_CAPTURE_CHECK").as_deref()
            != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-canonical-native-canvas"
                | "canvas-canonical-native-canvas-narrow"
                | "canvas-canonical-native-list"
                | "canvas-canonical-native-list-narrow")
        )
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_account.as_deref() != Some(ACCOUNT)
        || shell.core.active_team.as_deref() != Some("T1")
        || shell.self_account.user_id != "U0"
        || !binding.owner.current(shell)
        || binding.owner.file != "Ffixture"
        || binding.root != TS
        || options["id"] != "canvas-comment-reply"
        || options["scope"] != binding.field_scope()
        || options["generation"].as_u64() != Some(binding.read)
    {
        return None;
    }
    let private = path(shell)?;
    let e = entry(shell, &binding.id)?;
    let native = e.native.as_ref()?;
    if e.disk.path.as_ref() != Some(&private)
        || e.disk.account != ACCOUNT
        || e.disk.scope.workspace != "T1"
        || e.disk.scope.user != "U0"
        || e.disk.scope.file != "Ffixture"
        || e.root != TS
        || !native.matches(binding)
        || options["mount"].as_str().is_none_or(str::is_empty)
        || options["mount"] != native.fixture_owner(None)["mount"]
    {
        return None;
    }
    Some(format!(
        "{bridge}\n{WRAPPER}\nconst late=canvasLateCaptureBridge({options},dioxus);\
         try{{return await canvasRichReplyBridge({options},late);}}finally{{late.close();}}"
    ))
}
