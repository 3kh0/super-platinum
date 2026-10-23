//! Finite keyboard mapping appearances under the original opening policy.
//! Not the provisional picker or a complete custom-emoji observation.
use super::*;
use crate::directory_member_fetches::Context;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::composer_emoji::standard::{Catalog, Version};

pub(super) struct Observed {
    context: Context,
    revision: u64,
    pub(super) appearances: Vec<delta_editor::EmojiAppearance>,
}
impl Observed {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.revision == crate::app_home::autocomplete::preferences_revision(shell)
    }
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
    policy: &own_edit_policy::Observation,
) -> Result<Observed, Error> {
    let (context, revision, version) = {
        let shell = state.peek();
        if !source.current(&shell)
            || !source.owner.reading(&shell, serial)
            || !policy.current(&shell)
        {
            return Err(unavailable());
        }
        let proof = policy.proof();
        // Keyboard mapping and base Unicode glyphs are locale-independent.
        // Locale/name translation remains a separate reached Save dependency.
        (
            Context::capture(&shell, &source.owner.scope.team)?,
            crate::app_home::autocomplete::preferences_revision(&shell),
            plain_observations::emoji_version(&proof["assignments"])?,
        )
    };
    let appearances = tokio::task::spawn_blocking(move || appearances(version))
        .await
        .map_err(|_| unavailable())??;
    let observed = Observed {
        context,
        revision,
        appearances,
    };
    let shell = state.peek();
    if !source.current(&shell)
        || !source.owner.reading(&shell, serial)
        || !policy.current(&shell)
        || !observed.current(&shell)
    {
        return Err(unavailable());
    }
    Ok(observed)
}

fn appearances(version: Version) -> Result<Vec<delta_editor::EmojiAppearance>, Error> {
    let names: BTreeMap<String, String> =
        serde_json::from_str(delta_editor::EMOTICON_TABLE).map_err(|_| unavailable())?;
    let catalog = Catalog::build(version).map_err(|_| unavailable())?;
    names
        .into_values()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| {
            // No preferred tone: the source keyboard mapping inserts only its
            // mapped base shortcode, without adding a tone or catalog alias.
            let insertion = catalog.insertion(&name, None).ok_or_else(unavailable)?;
            Ok(delta_editor::EmojiAppearance {
                name: format!(":{name}:"),
                text: insertion.glyph,
                media: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_emoticon_catalog_retires_with_preferences_socket_or_account() {
        let (mut shell, _, _) = crate::slack_lists::comments::own_edit::tests::setup();
        let observed = Observed {
            context: Context::capture(&shell, "T1").unwrap(),
            revision: crate::app_home::autocomplete::preferences_revision(&shell),
            appearances: appearances(Version::V16).unwrap(),
        };
        assert!(observed.current(&shell));
        let generation = shell.core.workspaces["T1"].rt_generation;
        crate::app_home::realtime(
            &mut shell,
            "T1",
            generation,
            &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                name: "convert_emoticons".into(),
                value: serde_json::json!(false),
            },
        );
        assert!(!observed.current(&shell));
        let observed = Observed {
            revision: crate::app_home::autocomplete::preferences_revision(&shell),
            ..observed
        };
        assert!(observed.current(&shell));
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        assert!(!observed.current(&shell));
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation -= 1;
        assert!(observed.current(&shell));
        shell.core.active_account = Some("replacement".into());
        assert!(!observed.current(&shell));
    }
    #[test]
    fn finite_emoticon_appearances_are_positive_selected_catalog_rows() {
        for version in [Version::V16, Version::V17] {
            let rows = appearances(version).unwrap();
            let names: BTreeSet<_> = rows.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(names.len(), rows.len());
            assert!(rows.iter().all(|r| !r.text.is_empty() && r.media.is_none()));
            assert_eq!(
                rows.iter()
                    .find(|r| r.name == ":slightly_smiling_face:")
                    .unwrap()
                    .text,
                "\u{1f642}"
            );
            assert_eq!(
                rows.iter().find(|r| r.name == ":heart:").unwrap().text,
                Catalog::build(version)
                    .unwrap()
                    .insertion("heart", None)
                    .unwrap()
                    .glyph
            );
        }
    }
}
