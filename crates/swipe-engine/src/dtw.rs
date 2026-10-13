use crate::keyboard::euclidean_dist;
use crate::types::Point;

/// Compact point used for the precomputed dictionary paths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P32 {
    pub x: f32,
    pub y: f32,
}

impl P32 {
    #[inline(always)]
    pub fn dist(self, other: P32) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl From<Point> for P32 {
    fn from(p: Point) -> Self {
        P32 {
            x: p.x as f32,
            y: p.y as f32,
        }
    }
}

/// Reusable row buffers so a prediction pass does not allocate per candidate.
#[derive(Default)]
pub struct DtwScratch {
    prev: Vec<f32>,
    curr: Vec<f32>,
}

/// Band half-width actually used for an `n` x `m` alignment: at least one
/// full slope step, so consecutive rows stay connected.
#[inline(always)]
pub fn effective_band(n: usize, m: usize, band: usize) -> usize {
    let slope = if n > 1 { (m - 1).div_ceil(n - 1) } else { m };
    band.max(slope).max(1)
}

#[inline(always)]
fn band_range(i: usize, n: usize, m: usize, band: usize) -> (usize, usize) {
    // Center of the band follows the diagonal from (1, 1) to (n, m), so paths
    // of different lengths are still compared along their natural alignment.
    let center = if n <= 1 {
        m as f32
    } else {
        1.0 + (i - 1) as f32 * (m - 1) as f32 / (n - 1) as f32
    };
    let lo = (center - band as f32).floor().max(1.0) as usize;
    let hi = ((center + band as f32).ceil() as usize).min(m);
    (lo, hi)
}

/// DTW constrained to a Sakoe-Chiba band that follows the slope between the
/// two sequences, with early abandoning once every cell in a row exceeds
/// `cutoff`. Returns `f32::INFINITY` when abandoned.
///
/// Because every warping path crosses every row and costs are non-negative,
/// the row minimum is a lower bound on the final distance, so abandoning never
/// discards a result that would have come in under `cutoff`.
pub fn dtw_banded(s: &[P32], t: &[P32], band: usize, cutoff: f32, scratch: &mut DtwScratch) -> f32 {
    let n = s.len();
    let m = t.len();
    if n == 0 || m == 0 {
        return f32::INFINITY;
    }
    let band = effective_band(n, m, band);

    let prev = &mut scratch.prev;
    let curr = &mut scratch.curr;
    prev.clear();
    prev.resize(m + 1, f32::INFINITY);
    curr.clear();
    curr.resize(m + 1, f32::INFINITY);
    prev[0] = 0.0;

    for i in 1..=n {
        let (lo, hi) = band_range(i, n, m, band);
        let si = s[i - 1];
        curr[lo - 1] = f32::INFINITY;

        let mut row_min = f32::INFINITY;
        let mut left = f32::INFINITY;
        for j in lo..=hi {
            let best = prev[j].min(prev[j - 1]).min(left);
            let v = si.dist(t[j - 1]) + best;
            curr[j] = v;
            left = v;
            row_min = row_min.min(v);
        }

        if row_min > cutoff {
            return f32::INFINITY;
        }

        if i < n {
            // Cells the next row may read that this row did not write.
            let (_, next_hi) = band_range(i + 1, n, m, band);
            for v in curr.iter_mut().take(next_hi + 1).skip(hi + 1) {
                *v = f32::INFINITY;
            }
        }
        std::mem::swap(prev, curr);
    }

    prev[m]
}

/// Legacy f64 banded DTW (diagonal band, no slope correction).
pub fn dtw_distance_fast(s: &[Point], t: &[Point], window: usize, cutoff: f64) -> f64 {
    let n = s.len();
    let m = t.len();
    if n == 0 || m == 0 {
        return f64::INFINITY;
    }

    let len_diff = (n as i64 - m as i64).unsigned_abs() as usize;
    if len_diff > window {
        return f64::INFINITY;
    }

    let mut prev = vec![f64::INFINITY; m + 1];
    let mut curr = vec![f64::INFINITY; m + 1];
    prev[0] = 0.0;

    for i in 1..=n {
        curr[0] = f64::INFINITY;
        let j_start = if i > window { i - window } else { 1 };
        let j_end = (i + window).min(m);

        if j_start > 1 {
            curr[j_start - 1] = f64::INFINITY;
        }

        let mut row_min = f64::INFINITY;
        for j in j_start..=j_end {
            let cost = euclidean_dist(&s[i - 1], &t[j - 1]);
            let prev_min = prev[j].min(curr[j - 1]).min(prev[j - 1]);
            curr[j] = cost + prev_min;
            row_min = row_min.min(curr[j]);
        }

        if row_min > cutoff {
            return f64::INFINITY;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[m]
}

/// Full O(n·m) DTW, used as a reference in tests.
pub fn dtw_distance(s: &[Point], t: &[Point]) -> f64 {
    let n = s.len();
    let m = t.len();
    if n == 0 || m == 0 {
        return f64::INFINITY;
    }

    let mut dtw = vec![vec![f64::INFINITY; m + 1]; n + 1];
    dtw[0][0] = 0.0;

    for i in 1..=n {
        for j in 1..=m {
            let cost = euclidean_dist(&s[i - 1], &t[j - 1]);
            let prev_min = dtw[i - 1][j].min(dtw[i][j - 1]).min(dtw[i - 1][j - 1]);
            dtw[i][j] = cost + prev_min;
        }
    }

    dtw[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(seed: u64, len: usize) -> Vec<Point> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let x = (state >> 40) as f64 / (1u64 << 24) as f64 * 10.0;
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let y = (state >> 40) as f64 / (1u64 << 24) as f64 * 2.0;
                Point { x, y }
            })
            .collect()
    }

    #[test]
    fn wide_band_matches_full_dtw() {
        let mut scratch = DtwScratch::default();
        for seed in 0..50u64 {
            let a = pts(seed, 3 + (seed as usize % 17));
            let b = pts(seed + 1000, 1 + (seed as usize * 7 % 23));
            let full = dtw_distance(&a, &b) as f32;
            let a32: Vec<P32> = a.iter().copied().map(P32::from).collect();
            let b32: Vec<P32> = b.iter().copied().map(P32::from).collect();
            let banded = dtw_banded(&a32, &b32, 64, f32::INFINITY, &mut scratch);
            assert!(
                (full - banded).abs() < 1e-3 * full.max(1.0),
                "seed {seed}: {full} vs {banded}"
            );
        }
    }

    #[test]
    fn narrow_band_is_never_below_full_dtw_and_cutoff_is_safe() {
        let mut scratch = DtwScratch::default();
        for seed in 0..50u64 {
            let a = pts(seed, 5 + (seed as usize % 30));
            let b = pts(seed + 77, 5 + (seed as usize * 3 % 40));
            let full = dtw_distance(&a, &b) as f32;
            let a32: Vec<P32> = a.iter().copied().map(P32::from).collect();
            let b32: Vec<P32> = b.iter().copied().map(P32::from).collect();
            let banded = dtw_banded(&a32, &b32, 2, f32::INFINITY, &mut scratch);
            assert!(banded.is_finite());
            assert!(banded + 1e-3 >= full);
            // A cutoff just above the true value must not abandon.
            let again = dtw_banded(&a32, &b32, 2, banded + 1e-3, &mut scratch);
            assert_eq!(banded, again);
            // A cutoff below it must abandon or return something above it.
            let cut = dtw_banded(&a32, &b32, 2, banded * 0.5, &mut scratch);
            assert!(cut.is_infinite() || cut > banded * 0.5);
        }
    }
}
