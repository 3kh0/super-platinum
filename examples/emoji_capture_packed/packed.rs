//! Experiment-only process-local snapshot. No canonical mutation or ticket state.
use crate::candidate_catalog::CustomRecord;

const WORD: usize = std::mem::size_of::<usize>();
const HEADER: usize = WORD * 2 + 1;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    SizeOverflow,
    Allocation,
    InputChanged,
    Malformed,
    Utf8,
}

#[derive(Debug)]
pub struct Packed {
    count: usize,
    bytes: Vec<u8>,
}

fn add_record(total: usize, name: usize, value: usize) -> Result<usize, Error> {
    total
        .checked_add(HEADER)
        .and_then(|n| n.checked_add(name))
        .and_then(|n| n.checked_add(value))
        .ok_or(Error::SizeOverflow)
}

impl Packed {
    /// The clone must enumerate the same borrowed input. The caller retains its
    /// canonical read borrow across both passes; no reference leaves this call.
    /// Failure returns before any caller-owned building/ticket state is changed.
    pub fn capture<'a>(
        rows: impl Iterator<Item = (&'a str, &'a str)> + Clone,
    ) -> Result<Self, Error> {
        Self::capture_with_flags(rows.map(|(name, value)| (name, value, false)))
    }

    pub fn capture_with_flags<'a>(
        rows: impl Iterator<Item = (&'a str, &'a str, bool)> + Clone,
    ) -> Result<Self, Error> {
        let (mut count, mut total) = (0usize, 0usize);
        for (name, value, _) in rows.clone() {
            total = add_record(total, name.len(), value.len())?;
            count = count.checked_add(1).ok_or(Error::SizeOverflow)?;
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| Error::Allocation)?;
        let mut written = 0usize;
        for (name, value, is_nonexistent) in rows {
            if written == count || add_record(bytes.len(), name.len(), value.len())? > total {
                return Err(Error::InputChanged);
            }
            bytes.extend_from_slice(&name.len().to_ne_bytes());
            bytes.extend_from_slice(&value.len().to_ne_bytes());
            bytes.push(u8::from(is_nonexistent));
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(value.as_bytes());
            written += 1;
        }
        if written != count || bytes.len() != total {
            return Err(Error::InputChanged);
        }
        Ok(Self { count, bytes })
    }

    fn record<'a>(&'a self, cursor: &mut usize) -> Result<(&'a str, &'a str, bool), Error> {
        let header_end = cursor.checked_add(HEADER).ok_or(Error::Malformed)?;
        let header = self
            .bytes
            .get(*cursor..header_end)
            .ok_or(Error::Malformed)?;
        let name_len =
            usize::from_ne_bytes(header[..WORD].try_into().map_err(|_| Error::Malformed)?);
        let value_len = usize::from_ne_bytes(
            header[WORD..WORD * 2]
                .try_into()
                .map_err(|_| Error::Malformed)?,
        );
        let is_nonexistent = match header[WORD * 2] {
            0 => false,
            1 => true,
            _ => return Err(Error::Malformed),
        };
        let name_end = header_end.checked_add(name_len).ok_or(Error::Malformed)?;
        let end = name_end.checked_add(value_len).ok_or(Error::Malformed)?;
        let name = std::str::from_utf8(
            self.bytes
                .get(header_end..name_end)
                .ok_or(Error::Malformed)?,
        )
        .map_err(|_| Error::Utf8)?;
        let value = std::str::from_utf8(self.bytes.get(name_end..end).ok_or(Error::Malformed)?)
            .map_err(|_| Error::Utf8)?;
        *cursor = end;
        Ok((name, value, is_nonexistent))
    }

    /// Validate count, lengths, UTF-8 and exact EOF before allocating output.
    /// Output count is bounded by actual encoded bytes, never an unchecked header.
    pub fn decode(&self) -> Result<Vec<CustomRecord>, Error> {
        if self.count > self.bytes.len() / HEADER {
            return Err(Error::Malformed);
        }
        let mut cursor = 0;
        for _ in 0..self.count {
            self.record(&mut cursor)?;
        }
        if cursor != self.bytes.len() {
            return Err(Error::Malformed);
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.count)
            .map_err(|_| Error::Allocation)?;
        cursor = 0;
        for _ in 0..self.count {
            let (name, value, is_nonexistent) = self.record(&mut cursor)?;
            result.push(CustomRecord {
                name: copy(name)?,
                value: copy(value)?,
                is_nonexistent,
            });
        }
        Ok(result)
    }

    pub fn len_bytes(&self) -> usize {
        self.bytes.len()
    }
    pub fn capacity_bytes(&self) -> usize {
        self.bytes.capacity()
    }
}

fn copy(value: &str) -> Result<String, Error> {
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| Error::Allocation)?;
    result.push_str(value);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_roundtrip_handles_empty_unicode_nul_and_long_values() {
        let long = "x".repeat(100_000);
        let rows = [
            ("", ""),
            ("\0:\n", "🦊İΣς\0"),
            ("repeat", long.as_str()),
            ("repeat", "alias:repeat"),
        ];
        let p = Packed::capture(rows.iter().copied()).unwrap();
        let actual = p.decode().unwrap();
        assert_eq!(
            actual
                .iter()
                .map(|x| (x.name.as_str(), x.value.as_str()))
                .collect::<Vec<_>>(),
            rows
        );
        assert_eq!(
            p.len_bytes(),
            rows.iter()
                .map(|(a, b)| HEADER + a.len() + b.len())
                .sum::<usize>()
        );
        assert!(
            Packed::capture(std::iter::empty())
                .unwrap()
                .decode()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn malformed_count_lengths_utf8_truncation_and_trailing_bytes_are_refused() {
        assert_eq!(
            Packed {
                count: usize::MAX,
                bytes: vec![]
            }
            .decode(),
            Err(Error::Malformed)
        );
        let p = Packed::capture([("a", "b")].into_iter()).unwrap();
        for end in 0..p.bytes.len() {
            assert!(
                Packed {
                    count: 1,
                    bytes: p.bytes[..end].to_vec()
                }
                .decode()
                .is_err()
            );
        }
        let mut tail = p.bytes.clone();
        tail.push(0);
        assert_eq!(
            Packed {
                count: 1,
                bytes: tail
            }
            .decode(),
            Err(Error::Malformed)
        );
        let mut overflow = p.bytes.clone();
        overflow[..WORD].copy_from_slice(&usize::MAX.to_ne_bytes());
        assert_eq!(
            Packed {
                count: 1,
                bytes: overflow
            }
            .decode(),
            Err(Error::Malformed)
        );
        let mut invalid = p.bytes.clone();
        invalid[HEADER] = 0xff;
        assert_eq!(
            Packed {
                count: 1,
                bytes: invalid
            }
            .decode(),
            Err(Error::Utf8)
        );
        assert_eq!(
            Packed {
                count: 0,
                bytes: p.bytes
            }
            .decode(),
            Err(Error::Malformed)
        );
    }
    #[test]
    fn sizing_overflow_is_fallible_before_allocation() {
        assert_eq!(add_record(usize::MAX, 0, 0), Err(Error::SizeOverflow));
        assert_eq!(add_record(0, usize::MAX, 1), Err(Error::SizeOverflow));
    }
    #[test]
    fn capture_is_owned_after_source_changes() {
        let mut source = String::from("before");
        let p = Packed::capture([("name", source.as_str())].into_iter()).unwrap();
        source.clear();
        source.push_str("after");
        assert_eq!(p.decode().unwrap()[0].value, "before");
    }

    #[test]
    fn missing_emoji_flag_survives_packed_snapshot_and_invalid_flag_is_rejected() {
        let mut packed = Packed::capture_with_flags(
            [
                ("gone", "https://example.invalid/gone.png", true),
                ("alias", "alias:gone", false),
            ]
            .into_iter(),
        )
        .unwrap();
        let rows = packed.decode().unwrap();
        assert!(rows[0].is_nonexistent);
        assert!(!rows[1].is_nonexistent);
        packed.bytes[WORD * 2] = 2;
        assert_eq!(packed.decode(), Err(Error::Malformed));
    }
}
