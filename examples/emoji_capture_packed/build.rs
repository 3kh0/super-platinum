use super::{Arc, Catalog, CustomRecord, HashMap, PickerChoice, standard};
use standard::Version;
pub(super) fn build(
    mut custom: Vec<CustomRecord>,
) -> Result<(Arc<Catalog>, Arc<Vec<PickerChoice>>), String> {
    // emoji.list and the existing Workspace map have already lost server object
    // order. Sort explicitly rather than exposing random HashMap iteration ties.
    custom.sort_by(|a, b| a.name.cmp(&b.name));
    let catalog = Arc::new(Catalog::build_records(Version::V16, custom)?);
    let groups: HashMap<_, _> = catalog
        .search_entries
        .iter()
        .map(|entry| (entry.primary.canonical.as_str(), entry))
        .collect();
    let mut picker: Vec<_> = catalog
        .names
        .iter()
        .filter_map(|name| {
            let resolved = catalog.resolve(name, None)?;
            let aliases = groups
                .get(name.as_str())
                .map(|e| {
                    std::iter::once(name.as_str())
                        .chain(e.aliases.iter().map(|a| a.canonical.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| name.clone());
            Some(PickerChoice {
                name: name.clone(),
                glyph: resolved.glyph.unwrap_or_default(),
                url: resolved.image_url,
                aliases: aliases.to_lowercase(),
            })
        })
        .collect();
    const COMMON: &[&str] = &[
        "+1",
        "heart",
        "eyes",
        "joy",
        "tada",
        "fire",
        "rocket",
        "white_check_mark",
    ];
    let rank = |name: &str| {
        COMMON
            .iter()
            .position(|n| *n == name)
            .unwrap_or(COMMON.len())
    };
    picker.sort_by(|a, b| {
        rank(&a.name)
            .cmp(&rank(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((catalog, Arc::new(picker)))
}
