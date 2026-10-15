//! Shared exact-output check for the core regression suite and pure benchmark.
use super::{Catalog, frozen_catalog};

pub fn assert_same(old: &frozen_catalog::Catalog, new: &Catalog) {
    assert_eq!(old.names, new.names);
    assert_eq!(old.search_entries.len(), new.search_entries.len());
    for (a, b) in old.search_entries.iter().zip(&new.search_entries) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.is_emoji, b.is_emoji);
        assert_eq!(a.primary.canonical, b.primary.canonical);
        assert_eq!(a.primary.display, b.primary.display);
        assert_eq!(a.keywords, b.keywords);
        assert_eq!(a.aliases.len(), b.aliases.len());
        for (x, y) in a.aliases.iter().zip(&b.aliases) {
            assert_eq!(x.canonical, y.canonical);
            assert_eq!(x.display, y.display);
        }
    }
    for name in old.names.iter().map(String::as_str).chain([
        "",
        "missing",
        "invalid:name",
        "<bad>",
        "bad\nname",
        "rocket",
        "+1",
    ]) {
        for tone in [
            None,
            Some("2"),
            Some("3"),
            Some("4"),
            Some("5"),
            Some("6"),
            Some("bad"),
        ] {
            let a = old.resolve(name, tone).map(|x| {
                (
                    x.name,
                    x.shortcode,
                    x.glyph,
                    x.image_url,
                    x.custom,
                    x.usage_id,
                )
            });
            let b = new.resolve(name, tone).map(|x| {
                (
                    x.name,
                    x.shortcode,
                    x.glyph,
                    x.image_url,
                    x.custom,
                    x.usage_id,
                )
            });
            assert_eq!(a, b, "resolve {name:?} tone {tone:?}");
        }
    }
}
