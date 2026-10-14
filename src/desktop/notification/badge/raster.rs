//! Rasterize the existing Material Symbols Rounded Circle path for native HICONs.
//! This deliberately accepts only the M/Q/T/Z vocabulary used by that asset;
//! unsupported source changes fail instead of silently inventing a replacement.
type Point = (f64, f64);

fn contours(path: &str) -> Result<Vec<Vec<Point>>, String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let bytes = path.as_bytes();
    while start < bytes.len() {
        if bytes[start].is_ascii_whitespace() || bytes[start] == b',' {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        if !bytes[start].is_ascii_alphabetic() {
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                end += 1;
            }
        }
        tokens.push(&path[start..end]);
        start = end;
    }
    let mut output = Vec::new();
    let mut polygon = Vec::new();
    let mut current = (0.0, 0.0);
    let mut control = current;
    let mut command = ' ';
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].as_bytes()[0].is_ascii_alphabetic() {
            command = tokens[index].chars().next().unwrap();
            index += 1;
        }
        if matches!(command, 'Z' | 'z') {
            current = *polygon.first().ok_or("Empty SVG contour")?;
            control = current;
            output.push(std::mem::take(&mut polygon));
            command = ' ';
            continue;
        }
        let relative = command.is_ascii_lowercase();
        let pair = |offset: usize| -> Result<Point, String> {
            let x = tokens
                .get(index + offset)
                .ok_or("Truncated SVG path")?
                .parse::<f64>()
                .map_err(|_| "Invalid SVG coordinate")?;
            let y = tokens
                .get(index + offset + 1)
                .ok_or("Truncated SVG path")?
                .parse::<f64>()
                .map_err(|_| "Invalid SVG coordinate")?;
            Ok(if relative {
                (current.0 + x, current.1 + y)
            } else {
                (x, y)
            })
        };
        match command.to_ascii_uppercase() {
            'M' => {
                current = pair(0)?;
                control = current;
                polygon.push(current);
                index += 2;
            }
            'Q' | 'T' => {
                let (next_control, end, consumed) = if command.eq_ignore_ascii_case(&'Q') {
                    (pair(0)?, pair(2)?, 4)
                } else {
                    (
                        (2.0 * current.0 - control.0, 2.0 * current.1 - control.1),
                        pair(0)?,
                        2,
                    )
                };
                for step in 1..=16 {
                    let t = f64::from(step) / 16.0;
                    let u = 1.0 - t;
                    polygon.push((
                        u * u * current.0 + 2.0 * u * t * next_control.0 + t * t * end.0,
                        u * u * current.1 + 2.0 * u * t * next_control.1 + t * t * end.1,
                    ));
                }
                current = end;
                control = next_control;
                index += consumed;
            }
            _ => return Err(format!("Unsupported Material SVG command {command}")),
        }
    }
    if !polygon.is_empty() {
        return Err("Unclosed Material SVG path".into());
    }
    Ok(output)
}

pub(super) fn symbol(size: usize) -> Result<Vec<u8>, String> {
    let polygons = contours(crate::icons::Icon::Circle.path())?;
    let mut pixels = vec![0; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut coverage = 0_u16;
            for sy in 0..4 {
                for sx in 0..4 {
                    let point = (
                        (x as f64 + (f64::from(sx) + 0.5) / 4.0) * 960.0 / size as f64,
                        (y as f64 + (f64::from(sy) + 0.5) / 4.0) * 960.0 / size as f64 - 960.0,
                    );
                    let mut inside = false;
                    for polygon in &polygons {
                        for (a, b) in polygon
                            .iter()
                            .zip(polygon.iter().cycle().skip(1))
                            .take(polygon.len())
                        {
                            if (a.1 > point.1) != (b.1 > point.1)
                                && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
                            {
                                inside = !inside;
                            }
                        }
                    }
                    coverage += u16::from(inside);
                }
            }
            let alpha = (coverage * 255 / 16) as u8;
            let pixel = &mut pixels[(y * size + x) * 4..][..4];
            // Premultiplied BGRA for a 32-bit Windows icon.
            pixel.copy_from_slice(&[alpha * 1 / 5, alpha * 1 / 5, alpha, alpha]);
        }
    }
    Ok(pixels)
}

pub(super) fn display_count(label: &str) -> String {
    if label.parse::<u64>().unwrap_or(u64::MAX) > 99 {
        "99+".into()
    } else {
        label.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_circle_is_transparent_centered_and_premultiplied() {
        let pixels = symbol(32).unwrap();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert_eq!(pixels[3], 0);
        assert_eq!(pixels[(16 * 32 + 16) * 4 + 3], 0); // official outline, not an invented dot
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
        assert!(
            pixels
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|v| *v <= p[3]))
        );
        assert!(contours("M0 0L1 1Z").is_err());
    }
    #[test]
    fn visual_count_cap_does_not_change_canonical_label() {
        assert_eq!(display_count("42"), "42");
        assert_eq!(display_count("99"), "99");
        assert_eq!(display_count("100"), "99+");
    }
}
