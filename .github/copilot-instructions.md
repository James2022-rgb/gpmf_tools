# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build / run / test

This is a Cargo workspace. The CLI binary lives in `crates/gpmf_tools`.

```bash
# Build CLI (default features: gpx + mp4)
cargo build -p gpmf_tools

# Run CLI
cargo run -p gpmf_tools -- extract-gpx -i path/to/video.mp4 -o out.gpx
cargo run -p gpmf_tools -- -v extract-gpx -i path/to/video.mp4 --stdout

# Tests — only the parser crate has tests today (a single `klv::tests::it_works`
# that round-trips bundled binary samples and dumps KLV structure to stdout)
cargo test -p gpmf_parser
cargo test -p gpmf_parser -- --nocapture   # see the dumped KLVs

# Format (parser crate has a rustfmt.toml — 4-space soft tabs, preserve import order)
cargo fmt
```

The release CI (`.github/workflows/ci.yml`) only fires on GitHub release publish and cross-builds `gpmf_tools` for `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`, and `x86_64-apple-darwin`. There is no PR-time CI.

## Workspace layout and dependencies

Three crates, layered bottom-up:

- **`gpmf_parser`** — pure parser for GoPro's GPMF byte format. No MP4/GPX/IO-format dependencies. Optional `time` feature (default on) adds `Gps9::to_datetime`.
- **`gpmf_util`** — bridges the parser to MP4 demuxing and GPX serialization. Features `mp4` and `gpx` (both default on) gate those integrations independently — the crate is usable in either combination, neither, or both.
- **`gpmf_tools`** — the CLI. Re-exposes `gpx` and `mp4` features by forwarding to `gpmf_util`. The `extract-gpx` subcommand is `#[cfg(all(feature = "gpx", feature = "mp4"))]`-gated, so it disappears entirely if either feature is off.

**Critical external dependency**: `mp4 = { git = "https://github.com/James2022-rgb/mp4-rust" }` — a fork that recognizes the GPMF (`gpmd`) handler in addition to upstream's `Cxyz`/`hvc1`. Without this fork the GPMF track cannot be located. Both `gpmf_util` and `gpmf_tools` pin this fork; if you swap to a local checkout, update both `Cargo.toml`s and keep them in sync (the path-version line is already commented in each).

## How a GPMF MP4 becomes a GPX

The pipeline is short but layered — understanding it requires reading across all three crates:

1. **`main.rs`** opens the MP4, walks `mp4_reader.tracks()` looking for the track whose handler is `FourCC("meta")` and whose name contains `"GoPro MET"`. That track's id is passed to `gpmf_util`.
2. **`GpmfTrack::from_mp4_reader`** iterates every sample on that track (sample ids are 1-indexed in mp4-rust), feeding each sample's bytes through `gpmf_parser::Klv::from_reader`.
3. **`Klv::from_reader`** decodes the KLV stream — fixed 8-byte header (`Fourcc` + `ValueType` + `sample_size` + `repeat`), value payload padded to 4-byte boundary. `ValueType::Nested` recurses; values pad to `(sample_size * repeat).next_multiple_of(4)`. Termination is detected via either `KlvError::ZeroFourcc` (an all-zeros FourCC) or `UnexpectedEof`.
4. **`GpmfSample::new`** receives a single `DEVC` (Device) KLV and pulls out the `GPS9` data: it locates `STRM` (stream) children, picks the one containing `GPS9`, validates the sibling `TYPE` ASCII is exactly `"lllllllSS"` (7 i32 + 2 u16), reads the raw complex bytes big-endian, and scales each field by the corresponding entry in the sibling `SCAL` array. This shape — DEVC → STRM[GPS9 + TYPE + SCAL] — is hard-coded; HERO11+ format, asserts on shape mismatch.
5. **`GpmfTrack::write_gpx`** skips samples with `gps9.fix == 0` (no fix), converts `days_since_2000` + `seconds_since_midnight` to a UTC `OffsetDateTime`, and emits a single GPX 1.0 track with one segment of waypoints.

The parser keeps full KLV trees on every `GpmfSample` (`klvs: Vec<Klv>`), not just the parsed `Gps9` — anything else (accelerometer, gyro, etc.) is already in memory and reachable via `GpmfSample::klvs()`, just not exposed through dedicated accessors yet.

## Things to know before changing the parser

- `Value::read_numeric` uses `Vec::with_capacity` + raw-pointer write + `set_len` (unsafe) to fill numeric arrays. If you change the read loop, preserve the invariant that `values_from_reader` writes exactly `value_count` elements before `set_len` runs.
- ASCII and DateTime values are decoded as Latin-1 → UTF-8 (each byte mapped to a `char`). This is intentional — GPMF strings are Latin-1, not UTF-8 — so don't "fix" it to `str::from_utf8`.
- `Fourcc::as_str` calls `expect`, so any FourCC with non-UTF-8 bytes (other than the zero-terminator handled by `ZeroFourcc`) will panic. The bundled `gpmf.hexpat` (ImHex pattern) and `test_files/*.bin` are useful when investigating malformed input.
- `GpmfSample::new` asserts repeatedly on structural assumptions (DEVC contains nested, STRM contains nested, TYPE == `"lllllllSS"`, SCAL has 9 entries). Pre-HERO11 footage that uses `GPS5` instead of `GPS9` will hit these asserts — extending support means a new code path, not a tweak.

## References

The parser is implemented against:
- https://github.com/gopro/gpmf-parser (official format spec)
- https://exiftool.org/TagNames/GoPro.html (FourCC reference)
