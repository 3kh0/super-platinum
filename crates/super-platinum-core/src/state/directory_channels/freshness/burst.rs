//! Actual WjOl eviction order: ensureLimit precedes existing-key replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Burst {
    Allowed,
    Detected,
    Ongoing,
}
#[derive(Clone)]
struct Entry {
    count: u64,
    at: u64,
    reported: bool,
}
#[derive(Default)]
pub struct InvalidationBursts {
    entries: Vec<(String, Entry)>,
}
impl InvalidationBursts {
    #[cfg(test)]
    pub(super) fn len_for_test(&self) -> usize {
        self.entries.len()
    }
    #[cfg(test)]
    pub(super) fn contains_for_test(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }
    fn read(&mut self, key: &str) -> Option<Entry> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        let item = self.entries.remove(index);
        let value = item.1.clone();
        self.entries.push(item);
        Some(value)
    }
    fn write(&mut self, key: &str, entry: Entry) {
        if self.entries.len() == 100 {
            self.entries.remove(0);
        }
        self.entries.retain(|(k, _)| k != key);
        self.entries.push((key.into(), entry));
    }
    /// Pure source burst accounting; the native CFM owns keys and original event admission.
    pub fn check(&mut self, key: &str, now: u64) -> Burst {
        let old = self.read(key);
        let Some(mut entry) = old.filter(|e| now <= e.at + 10_000) else {
            self.write(
                key,
                Entry {
                    count: 1,
                    at: now,
                    reported: false,
                },
            );
            return Burst::Allowed;
        };
        entry.count += 1;
        self.write(key, entry.clone());
        if entry.count <= 5 {
            Burst::Allowed
        } else if entry.reported {
            Burst::Ongoing
        } else {
            entry.reported = true;
            self.write(key, entry);
            Burst::Detected
        }
    }
}
