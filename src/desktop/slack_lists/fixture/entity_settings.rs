//! Static local settings only; no create/cell transport is invoked.
use super::*;
use crate::slack_lists::rename::text_create::entity::{
    self,
    settings::{Edit, edit_shell},
};
use super_platinum_core::slack_lists::EntityCreateFormat;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let channel = match name {
        "slack-list-people-settings" | "slack-list-people-settings-narrow" => false,
        "slack-list-channel-settings" | "slack-list-channel-settings-narrow" => true,
        _ => panic!("Unrecognized entity settings fixture"),
    };
    text_field_create::install(
        shell,
        if channel {
            "slack-list-channel-field-create"
        } else {
            "slack-list-people-field-create"
        },
    );
    if channel {
        text_field_create::channel::cache::seed(shell).unwrap();
    } else {
        text_field_create::people::cache::seed(shell).unwrap();
    }
    entity::begin(shell, entity::Stamp::capture(shell).unwrap(), String::new()).unwrap();
    let apply = |edit| {
        assert!(edit_shell(
            shell,
            entity::Stamp::capture(shell).unwrap(),
            edit
        ))
    };
    apply(Edit::Format(EntityCreateFormat::Multiple));
    for id in if channel {
        ["G00000002", "C00000001"]
    } else {
        ["U00000002", "U00000001"]
    } {
        apply(Edit::Activate(id.into()));
    }
    if !channel {
        apply(Edit::Format(EntityCreateFormat::Single));
        apply(Edit::ShowNames(false));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_static_settings_only_retain_real_ordered_configuration_and_population() {
        for name in [
            "slack-list-people-settings",
            "slack-list-people-settings-narrow",
            "slack-list-channel-settings",
            "slack-list-channel-settings-narrow",
        ] {
            let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            let p = entity::presentation(&s).unwrap();
            let channel = name.starts_with("slack-list-channel");
            assert_eq!(
                p.options.selected_ids(),
                if channel {
                    ["G00000002", "C00000001"]
                } else {
                    ["U00000002", "U00000001"]
                }
            );
            assert_eq!(
                p.options.format(),
                if channel {
                    EntityCreateFormat::Multiple
                } else {
                    EntityCreateFormat::Single
                }
            );
            assert_eq!(p.options.show_names(), channel);
            assert!(s.core.session.is_none() && s.core.transport.is_none());
            assert_eq!(super::super::attempt_count(&s), 0);
            assert_eq!(
                s.slack_lists
                    .view
                    .as_ref()
                    .unwrap()
                    .loaded
                    .as_ref()
                    .unwrap()
                    .records
                    .raw["records"]
                    .as_array()
                    .unwrap()
                    .len(),
                73
            );
            assert!(entity::settings::close_test(&s));
        }
    }
}
