use crate::types::Point;
use std::collections::HashMap;

/// Key rows with their horizontal offset (in key widths) relative to the top row.
///
/// The offsets follow a physical Mac/ANSI keyboard (A row shifted 0.25, Z row
/// shifted 0.75), which is what users actually drag across. Punctuation keys
/// are included so that keys hit mid-swipe still contribute to the path shape.
pub const KEY_ROWS: [(&str, f64); 3] = [
    ("qwertyuiop[]", 0.0),
    ("asdfghjkl;'", 0.25),
    ("zxcvbnm,./", 0.75),
];

pub fn get_keyboard_layout() -> HashMap<char, Point> {
    let mut layout = HashMap::new();
    for (y, (chars, x_offset)) in KEY_ROWS.iter().enumerate() {
        for (i, c) in chars.chars().enumerate() {
            layout.insert(
                c,
                Point {
                    x: i as f64 + x_offset,
                    y: y as f64,
                },
            );
        }
    }
    layout
}

/// Spacing (in key widths) between interpolated points on a path.
pub const PATH_STEP: f64 = 0.5;

pub fn get_word_path(word: &str, layout: &HashMap<char, Point>) -> Vec<Point> {
    let key_points: Vec<Point> = word
        .chars()
        .filter_map(|c| layout.get(&c.to_ascii_lowercase()).cloned())
        .collect();

    if key_points.is_empty() {
        return vec![];
    }

    let mut full_path = vec![key_points[0]];

    for i in 1..key_points.len() {
        let p1 = key_points[i - 1];
        let p2 = key_points[i];

        let dist = euclidean_dist(&p1, &p2);
        if dist > PATH_STEP {
            let num_steps = (dist / PATH_STEP).round().max(1.0) as i32;
            let dx = (p2.x - p1.x) / num_steps as f64;
            let dy = (p2.y - p1.y) / num_steps as f64;

            for s in 1..num_steps {
                let new_x = p1.x + dx * s as f64;
                let new_y = p1.y + dy * s as f64;
                full_path.push(Point { x: new_x, y: new_y });
            }
        }
        full_path.push(p2);
    }

    full_path
}

pub fn euclidean_dist(p1: &Point, p2: &Point) -> f64 {
    ((p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2)).sqrt()
}

pub fn simplify_path(path: &[Point]) -> Vec<Point> {
    if path.is_empty() {
        return vec![];
    }

    let mut new_path = vec![path[0]];
    for p in path.iter().skip(1) {
        if euclidean_dist(p, new_path.last().unwrap()) > 0.01 {
            new_path.push(*p);
        }
    }
    new_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_staggered_like_a_physical_keyboard() {
        let layout = get_keyboard_layout();
        assert_eq!(layout[&'q'].x, 0.0);
        assert_eq!(layout[&'a'].x, 0.25);
        assert_eq!(layout[&'z'].x, 0.75);
        assert_eq!(layout[&'m'].y, 2.0);
        assert!(layout.contains_key(&';'));
    }

    #[test]
    fn path_is_evenly_sampled() {
        let layout = get_keyboard_layout();
        let path = get_word_path("qp", &layout);
        assert_eq!(path.len(), 19);
        for w in path.windows(2) {
            assert!((euclidean_dist(&w[0], &w[1]) - 0.5).abs() < 1e-9);
        }
    }
}
