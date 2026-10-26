//! Snippet leaves reuse the existing title source owners. The syntax adapter
//! below is not a file receipt and never enters a file metadata Store.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(in crate::profile_edit::sections::rich::files) struct Preview {
    source: shared::Observation,
    members: Option<users::Members>,
    channels: Option<channels::Channels>,
    groups: Option<groups::Groups>,
    files: Vec<(Value, Observation)>,
}
fn leaves(source: &shared::Observation) -> Result<Vec<Value>, Error> {
    let Some(block) = source.row().block().filter(|b| !b.is_null()) else {
        return Ok(vec![]);
    };
    let block = file_titles::section_preview_block(block).map_err(|_| unavailable())?;
    let mut leaves = Vec::new();
    for child in block["elements"].as_array().ok_or_else(unavailable)? {
        if !matches!(
            child["type"].as_str(),
            Some(
                "rich_text_section"
                    | "rich_text_quote"
                    | "rich_text_preformatted"
                    | "rich_text_list"
            )
        ) {
            return Err(unavailable());
        }
        let sections = if child["type"] == "rich_text_list" {
            child["elements"]
                .as_array()
                .ok_or_else(unavailable)?
                .iter()
                .collect::<Vec<_>>()
        } else {
            vec![child]
        };
        for section in sections {
            leaves.extend(
                section["elements"]
                    .as_array()
                    .ok_or_else(unavailable)?
                    .iter()
                    .cloned(),
            );
        }
    }
    Ok(leaves)
}
fn scan(
    source: &shared::Observation,
) -> Result<(file_titles::TitleDependencies, Vec<Value>), Error> {
    let mut files = Vec::new();
    for leaf in leaves(source)? {
        if matches!(leaf["type"].as_str(), Some("canvas" | "file")) {
            if leaf["file_id"].as_str().is_none_or(str::is_empty) {
                return Err(unavailable());
            }
            files.push(leaf.clone());
        }
    }
    // Reuse one accepted leaf grammar without granting the adapter any source,
    // kind/status, title or Quip override authority.
    let deps = match source.row().block().filter(|block| !block.is_null()) {
        Some(block) => file_titles::section_dependencies(
            block,
            Some(super::super::super::title_locale::EmojiTitle::english().context()),
        )
        .map_err(|_| unavailable())?,
        None => file_titles::TitleDependencies::default(),
    };
    Ok((deps, files))
}
impl Preview {
    pub(super) fn snapshot(&self) -> Value {
        serde_json::json!({"members":self.members.as_ref().map(|m|m.snapshot()),
            "channels":self.channels.as_ref().map(|c|c.snapshot()),
            "groups":self.groups.as_ref().map(|g|g.snapshot()),
            "files":self.files.iter().map(|(raw,file)|serde_json::json!({"raw":raw,"source":file.row.source(),"effective":file.row.effective(),"receipt":file.row.receipt()})).collect::<Vec<_>>()})
    }
    pub(super) fn capture(
        shell: &ShellState,
        token: &Token,
        source: shared::Observation,
    ) -> Result<Self, Error> {
        let (deps, raw_files) = scan(&source)?;
        let members = users::Members::capture(shell, token, &deps.users)?;
        let channels = channels::Channels::capture(shell, token, &deps.channels)?;
        let groups = groups::Groups::capture(shell, token, &deps.groups)?;
        let files = raw_files
            .into_iter()
            .map(|raw| {
                let observed = Observation::capture(
                    shell,
                    token,
                    raw["file_id"].as_str().ok_or_else(unavailable)?,
                )?;
                observed.label(&raw)?;
                Ok((raw, observed))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Self {
            source,
            members,
            channels,
            groups,
            files,
        })
    }
    pub(super) fn row(&self) -> &super_platinum_core::canvas::section_titles::Observation {
        self.source.row()
    }
    pub(in crate::profile_edit::sections::rich::files) fn current(
        &self,
        shell: &ShellState,
        token: &Token,
    ) -> bool {
        self.source.current(shell)
            && self
                .members
                .as_ref()
                .is_none_or(|m| m.current(shell, token))
            && self
                .channels
                .as_ref()
                .is_none_or(|c| c.current(shell, token))
            && self.groups.as_ref().is_none_or(|g| g.current(shell, token))
            && self.files.iter().all(|(_, f)| f.current(shell))
    }
    pub(in crate::profile_edit::sections::rich::files) fn channel_ids(
        &self,
    ) -> std::vec::IntoIter<&String> {
        self.channels
            .iter()
            .flat_map(|c| c.ids())
            .chain(self.files.iter().flat_map(|(_, f)| f.channel_ids()))
            .collect::<Vec<_>>()
            .into_iter()
    }
    pub(super) fn label(&self, leaf: &Value) -> Result<String, file_titles::Unavailable> {
        if matches!(leaf["type"].as_str(), Some("canvas" | "file")) {
            return self
                .files
                .iter()
                .find(|(raw, _)| raw == leaf)
                .ok_or(file_titles::Unavailable::InvalidMetadata)?
                .1
                .label(leaf)
                .map_err(|_| file_titles::Unavailable::InvalidMetadata);
        }
        file_titles::section_leaf_label(
            leaf,
            Some(super::super::super::title_locale::EmojiTitle::english().context()),
            |id| {
                self.members
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
            |id| {
                self.channels
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
            |id| {
                self.groups
                    .as_ref()
                    .ok_or(file_titles::Unavailable::InvalidMetadata)?
                    .label(id)
            },
        )
    }
}

pub(in crate::profile_edit::sections::rich) struct Required {
    sources: Vec<shared::Observation>,
    syntax: file_titles::TitleDependencies,
    files: Vec<String>,
}
impl Required {
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &ShellState,
        token: &Token,
        blocks: &[Value],
    ) -> Result<Self, Error> {
        let mut sources = Vec::new();
        let mut syntax = file_titles::TitleDependencies::default();
        let mut files = BTreeSet::new();
        for raw in collect(blocks)? {
            let Some(section) = id(raw) else { continue };
            let file = raw["file_id"].as_str().ok_or_else(unavailable)?;
            let source = shared::Observation::capture(shell, &token.owner.team, file, section)?;
            let (deps, nested) = scan(&source)?;
            syntax.users.extend(deps.users);
            syntax.channels.extend(deps.channels);
            syntax.groups.extend(deps.groups);
            files.extend(
                nested
                    .iter()
                    .filter_map(|raw| raw["file_id"].as_str().map(str::to_owned)),
            );
            sources.push(source);
        }
        Ok(Self {
            sources,
            syntax,
            files: files.into_iter().collect(),
        })
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        self.sources.iter().all(|s| s.current(shell))
    }
    pub(in crate::profile_edit::sections::rich) async fn acquire_files(
        &self,
        state: Signal<ShellState>,
        token: &Token,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<(), Error> {
        if self.files.is_empty() {
            return Ok(());
        }
        recover(state, token, &self.files, false, |shell| {
            current(shell) && self.current(shell)
        })
        .await
    }
    pub(in crate::profile_edit::sections::rich) fn dependencies(
        &self,
        shell: &ShellState,
        token: &Token,
    ) -> Result<Dependencies, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let mut dependencies = super::super::dependencies(shell, token, &self.files)?;
        dependencies.ids.extend(self.syntax.users.iter().cloned());
        dependencies.ids.sort();
        dependencies.ids.dedup();
        dependencies
            .channels
            .extend(self.syntax.channels.iter().cloned());
        dependencies
            .groups
            .extend(self.syntax.groups.iter().cloned());
        dependencies.sections.extend(self.sources.iter().cloned());
        Ok(dependencies)
    }
}
