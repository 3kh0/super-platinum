//! Browser file names are inert metadata, separate from the private disk path.
pub(super) fn display_name(raw: Option<&str>) -> Option<String> {
    let leaf = raw.unwrap_or("").rsplit(['/', '\\']).next().unwrap_or("");
    // Same byte bound as the existing upload recovery record, not a Slack API limit.
    if leaf.len() > 1024 {
        return None;
    }
    if leaf.trim().is_empty() {
        return Some("pasted-file".into());
    }
    let name: String = leaf
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    Some(
        if name.trim().is_empty() || matches!(name.as_str(), "." | "..") {
            "pasted-file".into()
        } else {
            name
        },
    )
}

pub(super) fn disk_name(name: &str) -> String {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some(extension)
            if !extension.is_empty()
                && extension.len() <= 16
                && extension.bytes().all(|c| c.is_ascii_alphanumeric()) =>
        {
            format!("pasted.{extension}")
        }
        _ => "pasted".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_names_preserve_unicode_and_are_platform_independent() {
        for name in [
            "日本語 🦊.png",
            "résumé 🎹.txt",
            "report:1?.txt",
            "CON",
            "<b>name.txt",
        ] {
            assert_eq!(display_name(Some(name)).as_deref(), Some(name));
        }
        for name in [
            "../../日本語 🦊.png",
            r"C:\fakepath\日本語 🦊.png",
            r"folder\mixed/path/日本語 🦊.png",
        ] {
            assert_eq!(display_name(Some(name)).as_deref(), Some("日本語 🦊.png"));
        }
        assert_eq!(
            display_name(Some("a\0b\nc\td.txt")).as_deref(),
            Some("a_b_c_d.txt")
        );
        for name in [
            None,
            Some(""),
            Some("  "),
            Some("\n\t"),
            Some("."),
            Some(".."),
            Some("dir/"),
        ] {
            assert_eq!(display_name(name).as_deref(), Some("pasted-file"));
        }
        assert!(display_name(Some(&"界".repeat(342))).is_none());
        assert_eq!(display_name(Some(&"a".repeat(1024))).unwrap().len(), 1024);
    }

    #[test]
    fn disk_names_keep_only_a_short_ascii_extension() {
        assert_eq!(disk_name("日本語 🦊.PNG"), "pasted.PNG");
        for name in [
            "CON",
            "a.日本語",
            "a.longextensionover16characters",
            "a.txt:stream",
            "a.",
        ] {
            assert_eq!(disk_name(name), "pasted");
        }
    }
}
