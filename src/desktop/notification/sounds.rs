//! Slack's sound names map to bundled bytes; no network work at delivery time.

fn bytes(name: &str) -> Option<&'static [u8]> {
    macro_rules! sound {
        ($name:literal) => {
            include_bytes!(concat!("../../../assets/notification-sounds/", $name)).as_slice()
        };
    }
    Some(match name {
        "b2.mp3" => sound!("b2.mp3"),
        "animal_stick.mp3" => sound!("animal_stick.mp3"),
        "been_tree.mp3" => sound!("been_tree.mp3"),
        "complete_quest_requirement.mp3" => sound!("complete_quest_requirement.mp3"),
        "confirm_delivery.mp3" => sound!("confirm_delivery.mp3"),
        "flitterbug.mp3" => sound!("flitterbug.mp3"),
        "here_you_go_lighter.mp3" => sound!("here_you_go_lighter.mp3"),
        "hi_flowers_hit.mp3" => sound!("hi_flowers_hit.mp3"),
        "knock_brush.mp3" => sound!("knock_brush.mp3"),
        "save_and_checkout.mp3" => sound!("save_and_checkout.mp3"),
        "item_pickup.mp3" => sound!("item_pickup.mp3"),
        "hummus.mp3" => sound!("hummus.mp3"),
        "boop.mp3" => sound!("boop.mp3"),
        "boop_remix.mp3" => sound!("boop_remix.mp3"),
        _ => return None,
    })
}

#[cfg(any(target_os = "linux", target_os = "windows", test))]
pub(super) fn is_bundled(name: &str) -> bool {
    bytes(name).is_some()
}

/// Explicit local previews work in the ordinary dev WebView, without requesting
/// OS notification permission or contacting Slack. Only bundled names reach JS.
pub(crate) fn preview_sound(name: &str) {
    if bytes(name).is_none() {
        return;
    }
    let name = serde_json::to_string(name).expect("sound names serialize");
    dioxus::document::eval(&format!(
        "{};window.__notificationSoundPreview.play({name});",
        include_str!("sounds/preview.js")
    ));
}

pub(crate) fn stop_sound_preview() {
    dioxus::document::eval("window.__notificationSoundPreview?.stop();");
}

pub(crate) fn serve_sound_asset(
    request: dioxus::desktop::AssetRequest,
    responder: dioxus::desktop::wry::RequestAsyncResponder,
) {
    responder.respond(sound_response(
        request.method().as_str(),
        request.uri().path(),
    ));
}

fn sound_response(
    method: &str,
    path: &str,
) -> dioxus::desktop::wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let asset = path.strip_prefix("/notification-sounds/").and_then(bytes);
    let (status, body) = match (method, asset) {
        ("GET", Some(body)) => (200, body),
        ("HEAD", Some(_)) => (200, b"".as_slice()),
        ("GET" | "HEAD", None) => (404, b"".as_slice()),
        _ => (405, b"".as_slice()),
    };
    dioxus::desktop::wry::http::Response::builder()
        .status(status)
        .header("Content-Type", "audio/mpeg")
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "private, max-age=3600")
        .body(std::borrow::Cow::Borrowed(body))
        .expect("static audio response is valid")
}

/// Only fixed, bundled names are passed to the native sound API.
#[cfg(target_os = "macos")]
pub(super) fn native_name(name: &str) -> Option<String> {
    bytes(name)?;
    Some(format!("{}.caf", name.strip_suffix(".mp3")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_sound_choices_are_bundled_and_unknown_names_are_silent() {
        for (sound, _) in super_platinum_core::slack::api::NOTIFICATION_SOUNDS {
            assert!(bytes(sound).is_some_and(|b| b.len() > 100));
        }
        assert!(bytes("boop_remix.mp3").is_some_and(|b| b.len() > 100));
        assert!(bytes("none").is_none());
        assert!(bytes("../../outside.mp3").is_none());
    }

    #[test]
    fn local_preview_route_only_serves_bundled_audio() {
        let response = sound_response("GET", "/notification-sounds/b2.mp3");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["Content-Type"], "audio/mpeg");
        assert_eq!(response.body().as_ref(), bytes("b2.mp3").unwrap());
        for path in [
            "/notification-sounds/../b2.mp3",
            "/notification-sounds/none",
            "/notification-sounds/%62%32.mp3",
            "/other/b2.mp3",
        ] {
            assert_eq!(sound_response("GET", path).status(), 404);
        }
        assert_eq!(
            sound_response("POST", "/notification-sounds/b2.mp3").status(),
            405
        );
        assert!(
            sound_response("HEAD", "/notification-sounds/b2.mp3")
                .body()
                .is_empty()
        );
    }
}
