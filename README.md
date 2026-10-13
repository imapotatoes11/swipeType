# swipeType

Swipe typing engine and macOS application.

- **Rust engine** (`crates/swipe-engine`)
- [Web interface](https://swipetype.zimengxiong.com) (`apps/web`)
- **macOS app** (`apps/mac`)

The engine treats a swipe as the sequence of keys your finger crosses on a physical (Mac/ANSI-staggered) keyboard, turns it into a path through key centers, and compares it with each dictionary word's ideal path. Matching runs in two passes over the 300k+ word dictionary:

1. **Shape pass**: every word whose first letter is the first swiped key or one of its neighbors gets a cheap score on paths resampled to 16 points by arc length (stored quantized, ~10 MB total), plus start/end-key penalties and a word-frequency bonus. Partial sums are abandoned as soon as a word can't make the shortlist.
2. **DTW pass**: the best 400 are re-scored with [Dynamic Time Warping](https://en.wikipedia.org/wiki/Dynamic_time_warping) on their full paths, using a Sakoe-Chiba band that follows the slope between paths of different lengths, O(n) memory, and early abandoning against a frequency-aware cutoff.

`cargo run --release --example eval -p swipe-engine -- word_freq.txt` simulates physical swipes (clean and sloppy) and reports top-1/top-3 accuracy and latency.

## Installation

```bash
brew install zimengxiong/tools/swipetype
```

## Building

<details>
<summary>Rust Engine</summary>

```bash
cd crates/swipe-engine

# Build native library
cargo build

# Build WASM for web
cargo build --features wasm

# Build staticlib for macOS
cargo build --release --features ffi
```

```rust
use swipe_engine::{SwipeEngine, Point};

let mut engine = SwipeEngine::new();
engine.load_dictionary("hello\t1000\nworld\t500\n");

let predictions = engine.predict("hello", 5);
// Returns: [(word: String, score: f64, freq: f64), ...]
```

</details>

<details>
<summary>Web</summary>

```bash
make web-build
make web-serve
```

</details>

<details>
<summary>macOS App</summary>

Requires Xcode, [XcodeGen](https://github.com/yonaskolb/XcodeGen) and Rust (via rustup, for the `aarch64`/`x86_64-apple-darwin` targets).

```bash
make mac-dmg
# Creates `apps/mac/build/SwipeType.dmg`
make mac-run
```

Builds are ad-hoc signed by default, so macOS treats every rebuild as a new app and forgets its Accessibility grant. To keep it between builds, sign with a stable identity:

```bash
make mac-build SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM=<your team id>
```

</details>

## Setup & Troubleshooting (macOS)

On first launch SwipeType opens a setup guide that walks through permissions, choosing a shortcut, and a practice field. You can reopen it from the menu bar icon (**Setup Guide…**) or by launching SwipeType again.

- **"SwipeType is damaged" / can't be opened**: the DMG isn't notarized. Open **System Settings › Privacy & Security** and click **Open Anyway**, or run `xattr -dr com.apple.quarantine /Applications/SwipeType.app`.
- **Shortcut does nothing**: SwipeType needs **Accessibility** access (called **Device Control and Data Access** on macOS 27) under System Settings › Privacy & Security. If it's already switched on but the menu bar icon is dimmed, the entry belongs to an older build: use **Reset & Re-register** in Settings or the setup guide, or run `tccutil reset Accessibility com.swipetype.SwipeTypeMac` and grant it again.
- **Run it from Finder, `open`, or `make mac-run`**, not by executing the binary from a terminal. Otherwise macOS checks the terminal's permissions instead of SwipeType's.
- **Nothing happens in a password field or Terminal**: secure input hides keystrokes from every app. The setup guide and Settings show when it's on.
- **Words don't appear in a particular app**: turn on **Insert by pasting** in Settings.

## Screenshots

| Main                          | Help                          |
| ----------------------------- | ----------------------------- |
| ![Main](screenshots/main.png) | ![Help](screenshots/help.png) |

| Settings                              |
| ------------------------------------- |
| ![Settings](screenshots/settings.png) |
