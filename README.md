# Rust TUI Synth (rusttuisynth)

A small terminal synthesizer and drum machine written in Rust. This is a pet project that was developed with a lot of automated/AI-assisted code generation and iteration — consider it experimental. If you find bugs or have feature requests, please open an issue on GitHub (https://github.com/a197120/rust-tui-daw) — contributions and ideas are very welcome.

## What this is
- A terminal-based DAW / synth + drum machine that runs in the terminal (TUI).
- Focused on simple, sample-accurate sequencing and synthesized drum voices (no audio samples).
- Designed as a playground for audio programming, UI in the terminal, and experimentation with effects.

## Key features
- Melodic polyphonic synth with:
  - Multiple waveforms and per-voice ADSR.
  - Voice pool and master mix.
- Step sequencer:
  - 16th-note steps, step counts 8/16/24/32.
  - Sample-accurate `tick(bpm)` driven sequencing.
- Drum machine:
  - 8 tracks (kick, snare, hats, clap, toms).
  - Synthesized drum sounds (noise + oscillators), per-track volume and mute.
  - Hi-hat choke (closed hat mutes open hat ringing).
- Effect scaffold:
  - `AudioEffect` trait and `EffectChain` — easy to add insert or bus effects.
- Single master BPM and volume — sequencer and drum machine stay phase-locked.
- Terminal UI implemented with `ratatui`, keyboard-driven controls via `crossterm`.
- Cross-platform audio output via `cpal`.

## Quickstart

Prerequisites:
- Rust toolchain (stable)
- Audio device configured for your OS

Build and run:
- Build: `cargo build --release`
- Run: `cargo run --release`

There are no automated tests yet. If you change the audio thread or the UI, be mindful that the audio callback acquires a mutex on each sample — keep UI lock durations short to avoid dropouts.

## Project notes
- Everything that generates audio lives inside the `Synth` / audio thread.
- To add an effect: implement the `AudioEffect` trait and push it into an existing `EffectChain`.
- To add a new drum sound: add a variant to `DrumKind` and implement synthesis in `DrumVoice`.
- The repository contains a `CLAUDE.md` with developer-facing notes and architecture.

## Contributing & contact
This is a personal pet project and many parts were created with AI assistance. If you:
- See a bug — please open an issue with reproduction steps.
- Want a feature — open an issue labeled `feature-request` or start a discussion.
- Want to contribute code — open a PR and describe the change; small, focused PRs are easiest to review.

You can reach out via GitHub issues: https://github.com/a197120/rust-tui-daw/issues

## License
No explicit license is included in the repo yet. If you'd like a license added (MIT / Apache-2.0 / etc.), open an issue or submit a PR.

Enjoy experimenting — and thanks for looking at the project!