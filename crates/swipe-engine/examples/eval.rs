//! Accuracy / latency evaluation on simulated physical-keyboard swipes.
//!
//! Usage: cargo run --release --example eval [path/to/word_freq.txt] [samples]
//!
//! A simulated swipe drags a "finger" along the polyline through a word's key
//! centers on a real (Mac ANSI) staggered keyboard, with per-letter aim noise,
//! and records every key the finger crosses. Optional key skips and neighbor
//! presses mimic fast or sloppy sweeps.

use std::time::Instant;
use swipe_engine::SwipeEngine;

const ROWS: [(&str, f64); 3] = [
    ("qwertyuiop[]", 0.0),
    ("asdfghjkl;'", 0.25),
    ("zxcvbnm,./", 0.75),
];

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-12);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

fn key_center(c: char) -> Option<(f64, f64)> {
    for (row, (keys, offset)) in ROWS.iter().enumerate() {
        if let Some(col) = keys.find(c) {
            return Some((col as f64 + offset, row as f64));
        }
    }
    None
}

fn key_at(x: f64, y: f64) -> Option<char> {
    let row = y.round();
    if !(0.0..=2.0).contains(&row) {
        return None;
    }
    let (keys, offset) = ROWS[row as usize];
    let col = (x - offset).round();
    if col < 0.0 {
        return None;
    }
    keys.chars().nth(col as usize)
}

struct Noise {
    aim: f64,
    skip: f64,
    neighbor: f64,
}

fn simulate(word: &str, noise: &Noise, rng: &mut Rng) -> String {
    let targets: Vec<(f64, f64)> = word
        .chars()
        .filter_map(key_center)
        .map(|(x, y)| {
            (
                x + rng.normal() * noise.aim,
                y + rng.normal() * noise.aim * 0.8,
            )
        })
        .collect();
    let mut out: Vec<char> = Vec::new();
    let push = |c: char, out: &mut Vec<char>| {
        if out.last() != Some(&c) {
            out.push(c);
        }
    };
    if let Some(c) = targets.first().and_then(|p| key_at(p.0, p.1)) {
        push(c, &mut out);
    }
    for w in targets.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let steps = (len / 0.05).ceil().max(1.0) as usize;
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if let Some(c) = key_at(x, y) {
                if out.last() == Some(&c) {
                    continue;
                }
                let is_endpoint = s == steps;
                if !is_endpoint && rng.uniform() < noise.skip {
                    continue;
                }
                push(c, &mut out);
                if rng.uniform() < noise.neighbor {
                    let dy = if rng.uniform() < 0.5 { -1.0 } else { 1.0 };
                    if let Some(n) = key_at(x, y + dy) {
                        push(n, &mut out);
                        push(c, &mut out);
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "word_freq.txt".to_string());
    let samples: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600);

    let text = std::fs::read_to_string(&path).expect("dictionary not found");
    let words: Vec<&str> = text.lines().filter_map(|l| l.split('\t').next()).collect();

    let t0 = Instant::now();
    let mut engine = SwipeEngine::new();
    engine.load_dictionary(&text);
    println!(
        "loaded {} words in {:.2?}",
        engine.word_count(),
        t0.elapsed()
    );
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        if let Some(rss) = status.lines().find(|l| l.starts_with("VmRSS")) {
            println!("{rss}");
        }
    }

    // Optional overrides for tuning, e.g. SWIPE_PARAMS="pop=0.3,end=4".
    let mut params = engine.params();
    if let Ok(spec) = std::env::var("SWIPE_PARAMS") {
        for kv in spec.split(',').filter(|s| !s.is_empty()) {
            let (k, v) = kv.split_once('=').expect("expected key=value");
            let v: f32 = v.parse().expect("expected a number");
            match k {
                "pop" => params.pop_weight = v,
                "start" => params.start_weight = v,
                "end" => params.end_weight = v,
                "radius" => params.start_radius = v,
                "band" => params.band_fraction = v,
                "minband" => params.min_band = v as usize,
                "rerank" => params.rerank_count = v as usize,
                _ => panic!("unknown parameter {k}"),
            }
        }
        println!("{params:?}");
    }
    engine.set_params(params);

    let sets: [(&str, usize, usize); 3] = [
        ("top 1k", 0, 1_000),
        ("1k-10k", 1_000, 10_000),
        ("10k-50k", 10_000, 50_000),
    ];
    let noises = [
        (
            "clean",
            Noise {
                aim: 0.12,
                skip: 0.0,
                neighbor: 0.0,
            },
        ),
        (
            "sloppy",
            Noise {
                aim: 0.28,
                skip: 0.08,
                neighbor: 0.06,
            },
        ),
    ];

    for (noise_name, noise) in &noises {
        for (set_name, lo, hi) in &sets {
            let mut rng = Rng(0x9E3779B97F4A7C15 ^ (*lo as u64 + 1));
            let mut top1 = 0;
            let mut top3 = 0;
            let mut n = 0;
            let mut latencies = Vec::with_capacity(samples);
            while n < samples {
                let idx = lo + (rng.next_u64() as usize) % (hi - lo);
                let word = words[idx];
                if word.len() < 2 {
                    continue;
                }
                let swipe = simulate(word, noise, &mut rng);
                if swipe.is_empty() {
                    continue;
                }
                let start = Instant::now();
                let preds = engine.predict(&swipe, 5);
                latencies.push(start.elapsed().as_secs_f64() * 1000.0);
                n += 1;
                if preds.first().map(|p| p.word.as_str()) == Some(word) {
                    top1 += 1;
                }
                if preds.iter().take(3).any(|p| p.word == word) {
                    top3 += 1;
                }
            }
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = latencies.iter().sum::<f64>() / latencies.len() as f64;
            let p95 = latencies[(latencies.len() as f64 * 0.95) as usize - 1];
            println!(
                "{:<7} {:<8} top1 {:5.1}%  top3 {:5.1}%  mean {:6.2}ms  p95 {:6.2}ms",
                noise_name,
                set_name,
                100.0 * top1 as f64 / n as f64,
                100.0 * top3 as f64 / n as f64,
                mean,
                p95
            );
        }
    }
}
