//! Cached channel row boundaries. Rebuilt at the shell's geometry mutation
//! boundary so unrelated renders never walk the complete cached transcript.

pub(crate) const DEFAULT_ROW_HEIGHT: f64 = 66.0;

#[derive(Debug, Clone)]
pub(crate) struct Geometry {
    /// Includes the leading zero boundary, even for an empty transcript.
    offsets: Vec<f64>,
}

impl Default for Geometry {
    fn default() -> Self {
        Self { offsets: vec![0.0] }
    }
}

impl Geometry {
    pub(crate) fn uniform(count: usize) -> Self {
        Self::from_heights(std::iter::repeat_n(DEFAULT_ROW_HEIGHT, count))
    }

    pub(crate) fn from_heights(heights: impl IntoIterator<Item = f64>) -> Self {
        let heights = heights.into_iter();
        let mut offsets = Vec::with_capacity(heights.size_hint().0.saturating_add(1));
        offsets.push(0.0);
        let mut offset = 0.0;
        for height in heights {
            debug_assert!(height.is_finite() && height > 0.0);
            offset += height;
            offsets.push(offset);
        }
        Self { offsets }
    }

    pub(crate) fn spacer_heights(&self, start: usize, end: usize) -> (f64, f64) {
        let count = self.offsets.len() - 1;
        let start = start.min(count);
        let end = end.max(start).min(count);
        (self.offsets[start], self.offsets[count] - self.offsets[end])
    }

    pub(crate) fn estimated_window(&self, top: f64, height: f64) -> (usize, usize) {
        let top = if top.is_finite() { top.max(0.0) } else { 0.0 };
        let bottom = top
            + if height.is_finite() {
                height.max(0.0)
            } else {
                0.0
            };
        let ends = &self.offsets[1..];
        let final_row = ends.len().saturating_sub(1);
        let last = ends.partition_point(|end| *end < bottom).min(final_row);
        // The linear implementation stops at the bottom boundary first. A
        // zero-height viewport exactly on a row boundary selects that row.
        let first = ends.partition_point(|end| *end <= top).min(last);
        (first, last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_window(heights: &[f64], top: f64, height: f64) -> (usize, usize) {
        let top = if top.is_finite() { top.max(0.0) } else { 0.0 };
        let bottom = top
            + if height.is_finite() {
                height.max(0.0)
            } else {
                0.0
            };
        let last = heights.len().saturating_sub(1);
        let mut first = None;
        let mut offset = 0.0;
        for (index, height) in heights.iter().enumerate() {
            offset += height;
            if first.is_none() && offset > top {
                first = Some(index);
            }
            if offset >= bottom {
                return (first.unwrap_or(index), index);
            }
        }
        (first.unwrap_or(last), last)
    }

    #[test]
    fn indexed_windows_match_linear_boundaries() {
        for count in [0, 1, 2, 17, 200, 10_000] {
            let heights = (0..count)
                .map(|index| match index % 4 {
                    0 => DEFAULT_ROW_HEIGHT,
                    1 => 31.25,
                    2 => 712.5,
                    _ => 84.0,
                })
                .collect::<Vec<_>>();
            let geometry = Geometry::from_heights(heights.iter().copied());
            let mut tops = vec![
                -1.0,
                0.0,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::MAX,
            ];
            // Include exact row ends and both sides, across the full transcript.
            for &boundary in geometry.offsets.iter().step_by((count / 37).max(1)) {
                tops.extend([boundary - 0.125, boundary, boundary + 0.125]);
            }
            for top in tops {
                for height in [
                    -1.0,
                    0.0,
                    0.125,
                    66.0,
                    132.0,
                    901.5,
                    f64::NAN,
                    f64::INFINITY,
                    f64::MAX,
                ] {
                    assert_eq!(
                        geometry.estimated_window(top, height),
                        linear_window(&heights, top, height),
                        "count={count} top={top} height={height}"
                    );
                }
            }
        }
    }

    #[test]
    fn spacer_boundaries_match_slices_and_clamp_empty_ranges() {
        let heights = [66.0, 31.25, 712.5, 84.0, 66.0];
        let geometry = Geometry::from_heights(heights);
        for start in 0..=heights.len() + 1 {
            for end in 0..=heights.len() + 1 {
                let clamped_start = start.min(heights.len());
                let clamped_end = end.max(clamped_start).min(heights.len());
                assert_eq!(
                    geometry.spacer_heights(start, end),
                    (
                        heights[..clamped_start].iter().sum(),
                        heights[clamped_end..].iter().sum()
                    )
                );
            }
        }
        assert_eq!(Geometry::default().spacer_heights(10, 20), (0.0, 0.0));
        assert_eq!(Geometry::uniform(10).spacer_heights(2, 7), (132.0, 198.0));
    }
}
