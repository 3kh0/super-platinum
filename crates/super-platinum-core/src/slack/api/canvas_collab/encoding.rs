//! RFC 4648 standard alphabet used by captured Quip pbutil.toBinaryString.
use super::super::super::Error;
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or_default();
        let c = chunk.get(2).copied().unwrap_or_default();
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

pub fn decode(value: &str) -> Result<Vec<u8>, Error> {
    let invalid = || Error::Api("canvas_base64_invalid".into());
    let bytes = value.as_bytes();
    let limit = crate::canvas::wire::MAX_BYTES;
    if bytes.len() > limit.div_ceil(3) * 4 || !bytes.len().is_multiple_of(4) {
        return Err(invalid());
    }
    let digit = |byte: u8| {
        ALPHABET
            .iter()
            .position(|b| *b == byte)
            .map(|n| n as u8)
            .ok_or_else(invalid)
    };
    let mut output = Vec::with_capacity((bytes.len() / 4 * 3).min(limit));
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = (index + 1) * 4 == bytes.len();
        let a = digit(chunk[0])?;
        let b = digit(chunk[1])?;
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 15 != 0 {
                return Err(invalid());
            }
            output.push((a << 2) | (b >> 4));
        } else {
            let c = digit(chunk[2])?;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 3 != 0 {
                    return Err(invalid());
                }
            } else {
                output.push((c << 6) | digit(chunk[3])?);
            }
        }
        if output.len() > limit {
            return Err(invalid());
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_vectors_and_noncanonical_padding() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        }
        let all = (0..=255).collect::<Vec<u8>>();
        assert_eq!(decode(&encode(&all)).unwrap(), all);
        for invalid in ["Zg", "Zh==", "Zm9=", "Zg==AAAA", "====", "_w==", "Zm9v\n"] {
            assert!(decode(invalid).is_err());
        }
    }
}
