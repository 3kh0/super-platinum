//! Selected standard order plus a positive complete custom receipt. All costly
//! parsing/projection runs on a worker; image registration remains in the host.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    composer_emoji::standard::{Catalog, Entry, Version},
    slack_lists::comments::edit_seed::prepare::StandardEmoji,
};

#[derive(Clone)]
enum Image {
    Glyph(String),
    Url(String),
}
struct Row {
    name: String,
    single: bool,
    multiple: bool,
    base: Image,
    variants: Vec<(String, Image)>,
}
pub(super) struct Prepared {
    names: Vec<String>,
    unicode: Vec<String>,
    reverse: Vec<(String, String)>,
    rows: Vec<Row>,
}

fn glyph(unicode: &str) -> Result<String, Error> {
    if unicode.is_empty() {
        return Err(unavailable());
    }
    unicode
        .split('-')
        .map(|part| {
            u32::from_str_radix(part, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(unavailable)
        })
        .collect()
}

// JS Object.values enumerates integer properties before string properties.
// Retained multi-tone keys are in lexical insertion order (checked separately
// against both original ordered JSON assets), unlike a plain BTreeMap walk.
fn variants(
    entry: &Entry,
) -> impl Iterator<
    Item = (
        &String,
        &super_platinum_core::composer_emoji::standard::Variant,
    ),
> {
    entry
        .skin_variations
        .iter()
        .filter(|(k, _)| !k.contains('-'))
        .chain(
            entry
                .skin_variations
                .iter()
                .filter(|(k, _)| k.contains('-')),
        )
}

fn resolved<'a>(
    standard: &'a Catalog,
    custom: &'a serde_json::Map<String, Value>,
    name: &'a str,
) -> Option<Result<(Image, Vec<(String, Image)>), Error>> {
    let mut name = name;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(name) {
            return None;
        }
        // UHpe.W tries standard before the current team's custom record.
        if let Some(entry) = standard.get(name) {
            return Some((|| {
                Ok((
                    Image::Glyph(glyph(&entry.unicode)?),
                    variants(entry)
                        .map(|(tone, value)| {
                            Ok((tone.clone(), Image::Glyph(glyph(&value.unicode)?)))
                        })
                        .collect::<Result<Vec<_>, Error>>()?,
                ))
            })());
        }
        let value = custom.get(name)?.as_str()?;
        if let Some(alias) = value.strip_prefix("alias:") {
            name = alias;
            continue;
        }
        let url = url::Url::parse(value).ok()?;
        if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
            return None;
        }
        return Some(Ok((Image::Url(value.into()), Vec::new())));
    }
}

fn tones(standard: &Catalog, custom: &serde_json::Map<String, Value>, name: &str) -> (bool, bool) {
    // UHpe.G canonicalizes once, then Fm/pA look only in standard data. This is
    // intentionally not the recursive appearance alias walk above.
    let canonical = if let Some(entry) = standard.get(name) {
        entry.alias_of.as_deref().unwrap_or(name)
    } else {
        custom
            .get(name)
            .and_then(Value::as_str)
            .and_then(|v| v.strip_prefix("alias:"))
            .unwrap_or(name)
    };
    standard.get(canonical).map_or((false, false), |entry| {
        (!entry.skin_variations.is_empty(), entry.multi_skin_tone)
    })
}

impl Prepared {
    pub(super) fn build(version: Version, complete: &Complete) -> Result<Self, Error> {
        if complete.names().len() > 32768 || complete.raw().to_string().len() > 8 * 1024 * 1024 {
            return Err(unavailable());
        }
        let standard = Catalog::build(version).map_err(|_| unavailable())?;
        let reverse = StandardEmoji::build(version).map_err(|_| unavailable())?;
        let custom = complete.raw()["emoji"]
            .as_object()
            .ok_or_else(unavailable)?;
        let mut unicode = Vec::new();
        let mut names = Vec::new();
        let mut seen = BTreeSet::new();
        let mut reversed = BTreeMap::new();
        for entry in &standard.entries {
            names.push(entry.name.clone());
            seen.insert(entry.name.clone());
            for raw in
                std::iter::once(&entry.unicode).chain(variants(entry).map(|(_, v)| &v.unicode))
            {
                unicode.push(glyph(raw)?);
                if let Some(name) = reverse.unicode_name(raw) {
                    reversed.insert(raw.clone(), name.to_owned());
                }
            }
        }
        for name in complete.names() {
            if seen.insert(name.clone()) {
                names.push(name.clone());
            }
        }
        let mut rows = Vec::new();
        for name in &names {
            // Unknown/cyclic aliases remain complete names but have no positive
            // appearance. The consumer must leave reached text untouched.
            if let Some(value) = resolved(&standard, custom, name) {
                let (base, variants) = value?;
                let (single, multiple) = tones(&standard, custom, name);
                rows.push(Row {
                    name: name.clone(),
                    single,
                    multiple,
                    base,
                    variants,
                });
            }
        }
        Ok(Self {
            names,
            unicode,
            reverse: reversed.into_iter().collect(),
            rows,
        })
    }

    pub(super) fn project(
        self,
        locale: String,
        current_locale: String,
        mut image: impl FnMut(&str, &str) -> String,
    ) -> LiveEmoji {
        let rows = self
            .rows
            .into_iter()
            .map(|row| {
                let mut appearance = |value: Image| match value {
                    Image::Glyph(text) => Appearance { text, media: None },
                    Image::Url(url) => Appearance {
                        text: format!(":{}:", row.name),
                        media: Some(image(&row.name, &url)),
                    },
                };
                EmojiRow {
                    name: row.name.clone(),
                    single: row.single,
                    multiple: row.multiple,
                    appearance: appearance(row.base),
                    variants: row
                        .variants
                        .into_iter()
                        .map(|(tone, value)| (tone, appearance(value)))
                        .collect(),
                }
            })
            .collect();
        LiveEmoji {
            names: self.names,
            unicode: self.unicode,
            unicode_names: self.reverse,
            locale,
            current_locale,
            rows,
        }
    }
}

#[cfg(test)]
mod tests;
