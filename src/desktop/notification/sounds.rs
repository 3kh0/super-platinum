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
        for sound in ["b2.mp3", "knock_brush.mp3", "boop_remix.mp3"] {
            assert!(bytes(sound).is_some_and(|b| b.len() > 100));
        }
        assert!(bytes("none").is_none());
        assert!(bytes("../../outside.mp3").is_none());
    }
}
