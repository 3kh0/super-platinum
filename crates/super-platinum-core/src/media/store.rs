//! On-disk cache for the small, long-lived images the UI paints constantly:
//! profile pictures and custom emoji.
//!
//! Entries are keyed by a *slot* — a stable identity such as a Slack user id —
//! and not by the URL. Slack rotates the avatar URL every time somebody changes
//! their picture, so a URL-keyed cache would miss exactly where a user notices
//! it: the one person whose photo just changed loses their face until the
//! download finishes. Keying by identity lets the previous picture paint
//! immediately while the new bytes arrive in the background.
//!
//! Each entry records the URL it came from, so a lookup can say whether the
//! cached bytes are still authoritative (skip the network entirely) or merely
//! the last known good picture for that slot (paint now, refetch).

use std::path::{Path, PathBuf};

use crate::error::AppError;

const MAGIC: &[u8] = b"SPM1\n";

/// Avatars and emoji run around 10 KiB, so this bounds the directory near
/// 40 MiB — small enough to never matter, large enough that an active
/// workspace's faces effectively never get evicted.
const MAX_ENTRIES: usize = 4_096;

/// This cache exists for images the UI paints at icon size. Refusing larger
/// entries keeps a caller from turning it into a general image cache, whatever
/// slot it asks for.
const MAX_ENTRY_BYTES: usize = 512 * 1024;

/// Bytes recovered from disk for one slot.
#[derive(Debug, Clone)]
pub struct CachedImage {
    pub bytes: Vec<u8>,
    pub mime: String,
    /// Whether these bytes came from exactly the URL that was asked for. When
    /// false they are the slot's previous picture: still worth painting, but the
    /// caller must go on to fetch the new one.
    pub current: bool,
}

#[derive(Debug, Clone)]
pub struct MediaStore {
    root: PathBuf,
}

impl MediaStore {
    /// Opens the shared cache under the platform data directory.
    pub fn open_default() -> Result<Self, AppError> {
        Self::open(crate::config::data_dir()?.join("media"))
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self, AppError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Cached bytes for `slot`, if any. `url` decides only whether the result is
    /// reported as `current`; a slot hit under a different URL is still returned.
    pub fn load(&self, slot: &str, url: &str) -> Option<CachedImage> {
        let raw = std::fs::read(self.path(slot)).ok()?;
        let (stored_slot, stored_url, bytes) = split_entry(&raw)?;
        // The file name is a non-cryptographic hash of the slot. Verifying the
        // identity stored inside the entry means a collision reads as a miss
        // rather than painting somebody else's face.
        if stored_slot != slot {
            return None;
        }
        Some(CachedImage {
            mime: super::detect_image_mime(bytes)
                .unwrap_or("image/png")
                .to_owned(),
            bytes: bytes.to_vec(),
            current: stored_url == url,
        })
    }

    /// Replaces the entry for `slot`. Writes through a temporary file so a crash
    /// mid-write cannot leave a truncated image behind.
    pub fn store(&self, slot: &str, url: &str, bytes: &[u8]) -> Result<(), AppError> {
        // The header is newline delimited, so a slot or URL containing one would
        // be unreadable. Neither Slack ids nor URLs can, but refuse rather than
        // write an entry that parses back as something else.
        if slot.contains('\n')
            || url.contains('\n')
            || bytes.is_empty()
            || bytes.len() > MAX_ENTRY_BYTES
        {
            return Ok(());
        }
        let mut entry = Vec::with_capacity(MAGIC.len() + slot.len() + url.len() + bytes.len() + 2);
        entry.extend_from_slice(MAGIC);
        entry.extend_from_slice(slot.as_bytes());
        entry.push(b'\n');
        entry.extend_from_slice(url.as_bytes());
        entry.push(b'\n');
        entry.extend_from_slice(bytes);

        let path = self.path(slot);
        let staging = path.with_extension(format!("tmp{}", uuid::Uuid::new_v4().simple()));
        std::fs::write(&staging, &entry)?;
        if let Err(error) = std::fs::rename(&staging, &path) {
            let _ = std::fs::remove_file(&staging);
            return Err(error.into());
        }
        Ok(())
    }

    /// Drops the oldest entries once the directory grows past [`MAX_ENTRIES`].
    pub fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        let mut files = entries
            .flatten()
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .collect::<Vec<_>>();
        if files.len() <= MAX_ENTRIES {
            return;
        }
        files.sort_by_key(|(modified, _)| *modified);
        for (_, path) in files.iter().take(files.len() - MAX_ENTRIES) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn path(&self, slot: &str) -> PathBuf {
        self.root.join(format!("{:016x}", slot_hash(slot)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn split_entry(raw: &[u8]) -> Option<(&str, &str, &[u8])> {
    let body = raw.strip_prefix(MAGIC)?;
    let split = body.iter().position(|byte| *byte == b'\n')?;
    let slot = std::str::from_utf8(&body[..split]).ok()?;
    let rest = &body[split + 1..];
    let split = rest.iter().position(|byte| *byte == b'\n')?;
    let url = std::str::from_utf8(&rest[..split]).ok()?;
    Some((slot, url, &rest[split + 1..]))
}

/// FNV-1a. A `DefaultHasher` is explicitly not stable across Rust releases, and
/// a hash change here would orphan every cached picture on a toolchain bump.
fn slot_hash(slot: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in slot.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> MediaStore {
        let root = std::env::temp_dir().join(format!(
            "super-platinum-media-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        MediaStore::open(root).expect("temp media store")
    }

    #[test]
    fn a_slot_hit_at_a_new_url_still_returns_the_previous_picture() {
        let store = temp_store("slot");
        store
            .store("avatar/U1", "https://cdn.test/U1-old-48", b"old-bytes")
            .expect("store");

        let same = store
            .load("avatar/U1", "https://cdn.test/U1-old-48")
            .expect("exact hit");
        assert!(same.current, "an unchanged URL needs no refetch");
        assert_eq!(same.bytes, b"old-bytes");

        // The picture changed: paint the old face, but keep the source pending.
        let rotated = store
            .load("avatar/U1", "https://cdn.test/U1-new-48")
            .expect("slot hit");
        assert!(!rotated.current);
        assert_eq!(rotated.bytes, b"old-bytes");

        assert!(
            store
                .load("avatar/U2", "https://cdn.test/U1-old-48")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn stored_entries_carry_their_own_identity() {
        // The file name is only a 64-bit hash; the entry has to prove which slot
        // it belongs to so a collision cannot paint the wrong face.
        let store = temp_store("identity");
        store
            .store("avatar/U1", "https://cdn.test/a", b"\x89PNG\r\n\x1a\nbody")
            .expect("store");
        let raw = std::fs::read(store.path("avatar/U1")).expect("entry file");
        let (slot, url, bytes) = split_entry(&raw).expect("parse");
        assert_eq!(slot, "avatar/U1");
        assert_eq!(url, "https://cdn.test/a");
        assert!(bytes.starts_with(b"\x89PNG"));

        // Mime comes from the bytes, not from the URL's extension.
        let hit = store.load("avatar/U1", "https://cdn.test/a").expect("hit");
        assert_eq!(hit.mime, "image/png");
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_second_write_replaces_the_entry_and_leaves_no_staging_file() {
        let store = temp_store("replace");
        store
            .store("emoji/party", "https://cdn.test/1", b"one")
            .expect("first");
        store
            .store("emoji/party", "https://cdn.test/2", b"two")
            .expect("second");
        let hit = store
            .load("emoji/party", "https://cdn.test/2")
            .expect("hit");
        assert!(hit.current);
        assert_eq!(hit.bytes, b"two");
        assert_eq!(
            std::fs::read_dir(store.root()).expect("read dir").count(),
            1,
            "staging files must not accumulate"
        );
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn oversized_entries_are_refused_rather_than_cached() {
        let store = temp_store("oversize");
        let big = vec![0u8; MAX_ENTRY_BYTES + 1];
        store
            .store("avatar/U1", "https://cdn.test/a", &big)
            .expect("store");
        assert!(store.load("avatar/U1", "https://cdn.test/a").is_none());
        assert_eq!(
            std::fs::read_dir(store.root()).expect("read dir").count(),
            0
        );
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn truncated_or_foreign_files_read_as_a_miss() {
        let store = temp_store("garbage");
        std::fs::write(store.path("avatar/U1"), b"not a media entry").expect("write");
        assert!(store.load("avatar/U1", "https://cdn.test/a").is_none());
        std::fs::write(store.path("avatar/U2"), MAGIC).expect("write");
        assert!(store.load("avatar/U2", "https://cdn.test/a").is_none());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn slot_hashes_are_stable_across_builds() {
        // Pinned so a future refactor cannot silently orphan every cached image.
        assert_eq!(slot_hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(
            format!("{:016x}", slot_hash("avatar/U1")),
            "cd2038f301dc7f07"
        );
    }
}
