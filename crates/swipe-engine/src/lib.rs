//! Swipe typing prediction engine

pub mod dtw;
pub mod keyboard;
pub mod types;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "wasm")]
pub use wasm::*;

#[cfg(feature = "ffi")]
pub mod ffi;

use dtw::{dtw_banded, DtwScratch, P32};
use keyboard::{get_keyboard_layout, get_word_path, simplify_path};
use std::collections::{BinaryHeap, HashMap, HashSet};
use types::{Point, Prediction};

pub use dtw::{dtw_distance, dtw_distance_fast as dtw_fast};
pub use keyboard::{
    euclidean_dist as euclidean_distance, get_keyboard_layout as keyboard_layout,
    get_word_path as word_path, simplify_path as path_simplify,
};
pub use types::Point as PointType;

/// Tunable scoring parameters.
#[derive(Clone, Copy, Debug)]
pub struct EngineParams {
    /// Weight of the (log-normalized, 0..1) word frequency in the final ranking.
    pub pop_weight: f32,
    /// Extra penalty per key width between the swipe's first key and the word's first letter.
    pub start_weight: f32,
    /// Extra penalty per key width between the swipe's last key and the word's last letter.
    pub end_weight: f32,
    /// Words whose first letter is further than this (in key widths) from the
    /// first swiped key are not considered.
    pub start_radius: f32,
    /// Sakoe-Chiba band half-width as a fraction of the word path length.
    pub band_fraction: f32,
    /// Lower bound on the band half-width.
    pub min_band: usize,
    /// How many candidates from the coarse shape pass are re-scored with DTW.
    pub rerank_count: usize,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            pop_weight: 0.25,
            start_weight: 3.0,
            end_weight: 3.0,
            start_radius: 1.3,
            band_fraction: 0.3,
            min_band: 4,
            rerank_count: 400,
        }
    }
}

const LETTERS: usize = 26;

/// Number of points each path is resampled to for the coarse pass.
const COARSE_POINTS: usize = 16;
/// Coarse points are stored quantized to this resolution (in key widths).
const COARSE_SCALE: f32 = 20.0;

/// Resamples a path to `COARSE_POINTS` points spaced evenly by arc length.
fn resample(path: &[P32]) -> [P32; COARSE_POINTS] {
    let mut out = [path.first().copied().unwrap_or_default(); COARSE_POINTS];
    if path.len() < 2 {
        return out;
    }
    let total: f32 = path.windows(2).map(|w| w[0].dist(w[1])).sum();
    if total <= f32::EPSILON {
        return out;
    }
    let step = total / (COARSE_POINTS - 1) as f32;
    let mut seg = 0;
    let mut seg_start = 0.0f32;
    let mut seg_len = path[0].dist(path[1]);
    for (k, slot) in out.iter_mut().enumerate().skip(1) {
        let target = step * k as f32;
        while seg + 2 < path.len() && seg_start + seg_len < target {
            seg_start += seg_len;
            seg += 1;
            seg_len = path[seg].dist(path[seg + 1]);
        }
        let t = if seg_len > 0.0 {
            ((target - seg_start) / seg_len).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let (a, b) = (path[seg], path[seg + 1]);
        *slot = P32 {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        };
    }
    out[COARSE_POINTS - 1] = *path.last().unwrap();
    out
}

fn quantize(v: f32) -> u8 {
    (v * COARSE_SCALE).round().clamp(0.0, 255.0) as u8
}

fn letter_index(c: char) -> Option<usize> {
    if c.is_ascii_lowercase() {
        Some((c as u8 - b'a') as usize)
    } else {
        None
    }
}

/// Uses a Dynamic Time Warping (DTW) algorithm to compare swipe paths
/// against a dictionary of words.
///
/// Prediction runs in two passes: every word whose first letter is near the
/// first swiped key gets a cheap shape score (paths resampled to a fixed number
/// of points by arc length), and only the best `rerank_count` of those are
/// re-scored with banded DTW on their full paths.
pub struct SwipeEngine {
    layout: HashMap<char, Point>,
    letter_points: [P32; LETTERS],
    params: EngineParams,

    words: Vec<String>,
    freqs: Vec<f32>,
    last_letter: Vec<u8>,
    /// Quantized resampled path of each word, `COARSE_POINTS` (x, y) pairs.
    coarse: Vec<[u8; COARSE_POINTS * 2]>,
    /// Word indices grouped by first letter, in dictionary (frequency) order.
    by_first_letter: [Vec<u32>; LETTERS],
}

#[derive(PartialEq)]
struct Ranked {
    combined: f32,
    score: f32,
    idx: u32,
}

impl Eq for Ranked {}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.combined
            .total_cmp(&other.combined)
            .then(self.idx.cmp(&other.idx))
    }
}

/// Keeps the `limit` lowest-scoring entries.
struct TopK {
    limit: usize,
    heap: BinaryHeap<Ranked>,
}

impl TopK {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::with_capacity(limit + 1),
        }
    }

    /// Score an entry must beat to be kept.
    fn threshold(&self) -> f32 {
        if self.heap.len() < self.limit {
            f32::INFINITY
        } else {
            self.heap.peek().map_or(f32::INFINITY, |r| r.combined)
        }
    }

    fn push(&mut self, entry: Ranked) {
        if self.heap.len() < self.limit {
            self.heap.push(entry);
        } else if entry.combined < self.threshold() {
            self.heap.pop();
            self.heap.push(entry);
        }
    }

    fn into_sorted(self) -> Vec<Ranked> {
        self.heap.into_sorted_vec()
    }
}

impl SwipeEngine {
    /// Initializes with default QWERTY layout.
    pub fn new() -> Self {
        let layout = get_keyboard_layout();
        let mut letter_points = [P32::default(); LETTERS];
        for (i, slot) in letter_points.iter_mut().enumerate() {
            let c = (b'a' + i as u8) as char;
            *slot = layout[&c].into();
        }
        Self {
            layout,
            letter_points,
            params: EngineParams::default(),
            words: Vec::new(),
            freqs: Vec::new(),
            last_letter: Vec::new(),
            coarse: Vec::new(),
            by_first_letter: Default::default(),
        }
    }

    /// Higher values favor common words more heavily in the scoring function.
    pub fn set_pop_weight(&mut self, weight: f64) {
        self.params.pop_weight = weight.max(0.0) as f32;
    }

    pub fn params(&self) -> EngineParams {
        self.params
    }

    pub fn set_params(&mut self, params: EngineParams) {
        self.params = params;
    }

    /// Dictionary format: tab-separated "word\tcount" per line, ideally sorted
    /// by descending count. Words containing characters other than a-z are
    /// skipped, as are duplicates (the first occurrence wins).
    pub fn load_dictionary(&mut self, freq_text: &str) {
        let entries: Vec<(&str, Option<f64>)> = freq_text
            .lines()
            .filter_map(|line| {
                let (word, count) = match line.split_once('\t') {
                    Some((w, c)) => (w.trim(), c.trim().parse::<f64>().ok()),
                    None => (line.trim(), None),
                };
                (!word.is_empty()).then_some((word, count))
            })
            .collect();

        let max_count = entries.iter().filter_map(|e| e.1).fold(0.0f64, f64::max);
        let log_max = max_count.ln();

        self.words.clear();
        self.freqs.clear();
        self.last_letter.clear();
        self.coarse.clear();
        for bucket in self.by_first_letter.iter_mut() {
            bucket.clear();
        }
        self.words.reserve(entries.len());
        self.freqs.reserve(entries.len());
        self.last_letter.reserve(entries.len());
        self.coarse.reserve(entries.len());

        let mut seen = HashSet::with_capacity(entries.len());
        for (raw_word, count) in entries {
            let word = raw_word.to_ascii_lowercase();
            let (Some(first), Some(last)) = (
                word.chars().next().and_then(letter_index),
                word.chars().last().and_then(letter_index),
            ) else {
                continue;
            };
            if !word.chars().all(|c| c.is_ascii_lowercase()) || !seen.insert(word.clone()) {
                continue;
            }

            let freq = match count {
                Some(c) if c > 0.0 && log_max > 0.0 => ((c.ln() - 1.0) / log_max).max(0.0),
                _ => 0.0,
            };

            let mut packed = [0u8; COARSE_POINTS * 2];
            for (k, pt) in resample(&self.path_of(&word)).iter().enumerate() {
                packed[2 * k] = quantize(pt.x);
                packed[2 * k + 1] = quantize(pt.y);
            }

            self.by_first_letter[first].push(self.words.len() as u32);
            self.words.push(word);
            self.freqs.push(freq as f32);
            self.last_letter.push(last as u8);
            self.coarse.push(packed);
        }
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    fn path_of(&self, text: &str) -> Vec<P32> {
        simplify_path(&get_word_path(text, &self.layout))
            .into_iter()
            .map(P32::from)
            .collect()
    }

    /// Input string should be the sequence of characters the swipe path passes through.
    /// Returns predictions sorted by score.
    pub fn predict(&self, swipe_input: &str, limit: usize) -> Vec<Prediction> {
        if limit == 0 || self.words.is_empty() {
            return vec![];
        }

        let input_path = self.path_of(swipe_input);
        let (Some(&first_pt), Some(&last_pt)) = (input_path.first(), input_path.last()) else {
            return vec![];
        };
        let input_len = input_path.len() as f32;
        let p = self.params;

        // Pass 1: shape score on resampled paths for every word whose first
        // letter is the swiped key or one of its neighbors.
        let input_coarse = resample(&input_path);
        let mut buckets: Vec<(f32, usize)> = self
            .letter_points
            .iter()
            .enumerate()
            .map(|(i, pt)| (first_pt.dist(*pt), i))
            .filter(|(d, _)| *d <= p.start_radius)
            .collect();
        buckets.sort_by(|a, b| a.0.total_cmp(&b.0));

        let inv_scale = 1.0 / COARSE_SCALE;
        let inv_points = 1.0 / COARSE_POINTS as f32;
        let mut coarse_top = TopK::new(p.rerank_count.max(limit));
        for (start_dist, letter) in buckets {
            for &idx in &self.by_first_letter[letter] {
                let i = idx as usize;
                let end_dist = last_pt.dist(self.letter_points[self.last_letter[i] as usize]);
                let penalty = p.start_weight * start_dist + p.end_weight * end_dist;
                let base = penalty / input_len - p.pop_weight * self.freqs[i];
                let budget = (coarse_top.threshold() - base) * COARSE_POINTS as f32;
                if budget <= 0.0 {
                    continue;
                }

                let packed = &self.coarse[i];
                let mut sum = 0.0f32;
                for (k, q) in input_coarse.iter().enumerate() {
                    let x = packed[2 * k] as f32 * inv_scale;
                    let y = packed[2 * k + 1] as f32 * inv_scale;
                    sum += q.dist(P32 { x, y });
                    if sum >= budget {
                        break;
                    }
                }
                if sum >= budget {
                    continue;
                }
                coarse_top.push(Ranked {
                    combined: sum * inv_points + base,
                    score: start_dist,
                    idx,
                });
            }
        }

        // Pass 2: banded DTW on the full paths of the surviving candidates,
        // best coarse matches first so the cutoff tightens quickly.
        let mut top = TopK::new(limit);
        let mut scratch = DtwScratch::default();
        for cand in coarse_top.into_sorted() {
            let i = cand.idx as usize;
            let start_dist = cand.score;
            let path = self.path_of(&self.words[i]);
            let end_dist = last_pt.dist(*path.last().unwrap());
            let penalty = p.start_weight * start_dist + p.end_weight * end_dist;
            let freq = self.freqs[i];

            // combined = (dtw + penalty) / n - pop_weight * freq must beat the
            // current k-th best, which bounds the DTW distance worth computing.
            let budget = (top.threshold() + p.pop_weight * freq) * input_len - penalty;
            if budget <= 0.0 {
                continue;
            }
            let band = ((path.len() as f32 * p.band_fraction).ceil() as usize).max(p.min_band);
            let dist = dtw_banded(&input_path, &path, band, budget, &mut scratch);
            if !dist.is_finite() {
                continue;
            }

            let score = (dist + penalty) / input_len;
            top.push(Ranked {
                combined: score - p.pop_weight * freq,
                score,
                idx: cand.idx,
            });
        }

        top.into_sorted()
            .into_iter()
            .map(|r| Prediction {
                word: self.words[r.idx as usize].clone(),
                score: r.score as f64,
                freq: self.freqs[r.idx as usize] as f64,
            })
            .collect()
    }
}

impl Default for SwipeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(dict: &str) -> SwipeEngine {
        let mut engine = SwipeEngine::new();
        engine.load_dictionary(dict);
        engine
    }

    #[test]
    fn test_resample_is_even_and_keeps_endpoints() {
        let path = [
            P32 { x: 0.0, y: 0.0 },
            P32 { x: 3.0, y: 0.0 },
            P32 { x: 3.0, y: 1.5 },
        ];
        let out = resample(&path);
        assert_eq!(out[0], path[0]);
        assert_eq!(out[COARSE_POINTS - 1], path[2]);
        let step = 4.5 / (COARSE_POINTS - 1) as f32;
        for w in out.windows(2) {
            // Spacing is even along the path (the corner shortens one chord).
            assert!(w[0].dist(w[1]) <= step + 1e-4);
            assert!(w[0].dist(w[1]) >= step * 0.7);
        }
        assert_eq!(resample(&path[..1]), [path[0]; COARSE_POINTS]);
    }

    #[test]
    fn test_engine_creation() {
        let engine = SwipeEngine::new();
        assert_eq!(engine.word_count(), 0);
    }

    #[test]
    fn test_dictionary_loading() {
        let engine = engine("hello\t1000\nworld\t500\n");
        assert_eq!(engine.word_count(), 2);
    }

    #[test]
    fn test_dictionary_dedupes_and_skips_junk() {
        let engine = engine("hello\t1000\r\nhello\t1000\nHeLLo\t5\nit's\t10\n\t4\nworld\t500\n");
        assert_eq!(engine.word_count(), 2);
    }

    #[test]
    fn test_prediction() {
        let engine = engine("hello\t1000\nhello\t1000\nhelp\t800\nhell\t600\n");

        let predictions = engine.predict("hello", 5);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].word, "hello");
        let unique: HashSet<_> = predictions.iter().map(|p| p.word.clone()).collect();
        assert_eq!(unique.len(), predictions.len());
    }

    #[test]
    fn test_swipe_with_intermediate_keys() {
        let engine = engine("the\t1000\ntype\t500\nthey\t400\nhello\t300\n");
        // q-w-e-r-t ... a sloppy drag still resolves to the intended word.
        assert_eq!(engine.predict("tghjhgre", 3)[0].word, "the");
        assert_eq!(engine.predict("hgfdertyuiklo", 3)[0].word, "hello");
    }

    #[test]
    fn test_neighbor_first_key_is_tolerated() {
        let engine = engine("the\t1000\nhello\t300\n");
        // Swipe starts on 'r' (left of 't') instead of 't'.
        assert_eq!(engine.predict("rtghjhgre", 3)[0].word, "the");
    }

    #[test]
    fn test_punctuation_in_input_is_handled() {
        let engine = engine("hello\t1000\nhelp\t10\n");
        assert_eq!(engine.predict("hgfdertyuiklo.", 3)[0].word, "hello");
        assert!(engine.predict(".,;", 3).iter().all(|p| p.word != "hello"));
        assert!(engine.predict("123", 3).is_empty());
        assert!(engine.predict("", 3).is_empty());
        assert!(engine.predict("hello", 0).is_empty());
    }

    #[test]
    fn test_results_sorted_and_limited() {
        let dict: String = [
            "the", "then", "them", "they", "there", "these", "three", "thee",
        ]
        .iter()
        .enumerate()
        .map(|(i, w)| format!("{w}\t{}\n", 1000 - i * 10))
        .collect();
        let engine = engine(&dict);
        let preds = engine.predict("thrre", 4);
        assert_eq!(preds.len(), 4);
        let p = engine.params();
        let combined: Vec<f64> = preds
            .iter()
            .map(|x| x.score - p.pop_weight as f64 * x.freq)
            .collect();
        assert!(combined.windows(2).all(|w| w[0] <= w[1] + 1e-6));
    }

    /// Pruning must not change the result relative to exhaustively scoring
    /// every candidate.
    #[test]
    fn test_pruning_is_exact() {
        let words = [
            "hello", "help", "hell", "held", "helm", "jello", "yellow", "hollow", "halo", "hero",
            "here", "her", "gel", "jell", "bell", "belly", "hull", "hilly",
        ];
        let dict: String = words
            .iter()
            .enumerate()
            .map(|(i, w)| format!("{w}\t{}\n", 5000 / (i + 1)))
            .collect();
        let engine = engine(&dict);
        for input in [
            "hgfdertyuiklo",
            "jhgtrewsdfgh",
            "hytrertyuiop",
            "gtrewsdfghjkl",
            "bvcxzasd",
        ] {
            let top = engine.predict(input, 3);
            let all = engine.predict(input, words.len());
            let expected: Vec<_> = all.iter().take(3).map(|p| p.word.clone()).collect();
            let got: Vec<_> = top.iter().map(|p| p.word.clone()).collect();
            assert_eq!(got, expected, "input {input}");
        }
    }
}
