# RustTuiSynth — Claude context

Terminal synthesizer and drum machine written in Rust.
No tests exist yet. Build with `cargo build`, run with `cargo run`.

## Dependencies
- `ratatui 0.29` — TUI rendering
- `crossterm 0.28` — terminal I/O, keyboard events
- `cpal 0.15` — cross-platform audio output
- `anyhow 1.0` — error handling

## Module map

| File | Purpose |
|------|---------|
| `main.rs` | Terminal setup, event loop, key routing |
| `app.rs` | All application state; keyboard→action methods |
| `audio.rs` | CPAL audio stream; calls `Synth::generate_sample()` per frame |
| `synth.rs` | Melodic polyphonic voices, ADSR, waveforms, master mix |
| `sequencer.rs` | Melodic step sequencer (sample-accurate) |
| `drums.rs` | 8-track drum machine with synthesized voices |
| `effects.rs` | `AudioEffect` trait + `EffectChain`; also `BiquadFilter` + `FilterMode` |
| `ui.rs` | All Ratatui rendering; one function per panel |

## Architecture

### Audio thread
`AudioEngine` holds a CPAL stream. The callback locks `Arc<Mutex<Synth>>` and calls
`Synth::generate_sample()` once per sample. **Everything audio-generating lives inside
`Synth`** and runs in this thread.

```
CPAL callback
  └─ Synth::generate_sample()
       ├─ Sequencer::tick(bpm)          → note_on/note_off into voices
       ├─ melodic bus 1: voice mix → BiquadFilter (filter1) → EffectChain (fx)
       ├─ melodic bus 2: voice mix → BiquadFilter (filter2) → EffectChain (fx2)
       ├─ DrumMachine::generate_sample(bpm)
       │    ├─ fire_step() → DrumVoice pool (polyphonic)
       │    └─ DrumMachine::fx (EffectChain, empty)
       └─ (melodic + drums).tanh()      → master output
```

### UI / event thread
`main::run()` polls crossterm events at 16 ms. Key events call methods on `App`, which
locks the synth mutex only for the duration of each method call.

### Shared state
```
Arc<Mutex<Synth>>
  ├─ bpm: f32              ← single master clock for both sequencers
  ├─ volume: f32           ← master volume (applied to both buses)
  ├─ voices: HashMap<u8,Voice>
  ├─ sequencer: Sequencer
  ├─ filter1: BiquadFilter ← per-bus filter for S1 (before EffectChain)
  ├─ filter2: BiquadFilter ← per-bus filter for S2 (before EffectChain)
  ├─ drum_machine: DrumMachine
  └─ fx: EffectChain       ← melodic bus effects (empty)
```

### BPM
`Synth::bpm` is the **one** master tempo. Both `Sequencer::tick(bpm)` and
`DrumMachine::generate_sample(bpm)` receive it as a parameter so they are always
phase-locked. Changing BPM in any mode affects both sequencers immediately.

## Layout (all panels always visible)

```
Title bar (3 lines)   — focus indicator, seq/drum play status
Keyboard panel (12)   — piano + note highlights
Synth Seq panel (8)   — step grid (up to 32 steps)
Synth Seq 2 panel (8) — second melodic sequencer
Drum Machine (12)     — 8 track rows with volume
Effects panel (8)     — reverb, delay, distortion, sidechain, filter S1/S2 + routing
Status (4)            — wave, BPM, master vol, active notes
Scope (6)             — braille oscilloscope
Help (remaining)      — context-sensitive key hints
```

Active focus is shown with a **cyan border** on the focused panel.
Inactive panels have a dim border but are always rendered.

## Focus (`AppMode` enum, cycle with Tab or F2)

| Focus | `↑/↓` | `←/→` | `Space` | piano keys |
|-------|--------|--------|---------|------------|
| `Play` (Keyboard) | volume | octave | — | play notes |
| `SynthSeq` | BPM | cursor | play/pause | set step note |
| `SynthSeq2` | BPM | cursor | play/pause | set step note |
| `Drums` | select track | move step | toggle step | preview drums |
| `Effects` | select effect | select param | route 0↔100% | — |

**Global keys** (any focus): Tab/F2 cycle focus, F1 waveform,
F3 drum play/stop, PageUp/PageDown BPM ±5, Esc quit.

In **Drums focus**:
- `-`/`=` adjust per-track volume (0–100%)
- `p`/`[` adjust step probability (+/-25%)
- `<`/`>` adjust global swing (-/+5%)
- `\` mute/unmute track, `]` cycle step count, `e` euclidean fill

## Per-track drum volume

Each `DrumTrack` has a `volume: f32` (default 0.85, range 0.0–1.0).
`DrumMachine::track_volume_up/down(track)` adjust it by ±0.05.
The volume is displayed in the drum grid as `VVV%` beside the mute indicator.
`App::drum_vol_up/down()` call through and update `status_msg`.

## Drum machine swing

`DrumMachine` has a `swing: f32` field (default 0.0, range 0.0–0.5).

In `generate_sample()`, odd-indexed steps (1, 3, 5 …) are delayed by
`swing * samples_per_step` samples relative to their step boundary. Even steps
fire at phase 0 as before. This creates the laid-back groove of hip-hop/jazz/funk.

Musical reference points:
- `0.00` → straight (no change from previous behaviour)
- `0.17` → light groove
- `0.33` → classic triplet/shuffle (step fires at the 2/3 point of an 8th-note window)
- `0.50` → maximum late feel

`App::drum_swing_up/down()` step by ±0.05 and update `status_msg`.
The current swing percentage is shown live in the drum panel header (`Swing: XX%`,
yellow+bold when non-zero, gray at 0%).
Keys `<`/`>` in Drums focus (press and repeat).

## Drum machine (`drums.rs`)

8 tracks, each a `DrumTrack`:
- `kind: DrumKind` — Kick / Snare / ClosedHat / OpenHat / Clap / LowTom / MidTom / HighTom
- `steps: Vec<u8>` — 8/16/24/32 steps; value is trigger probability 0–100 (0=off, 100=always)
- `muted: bool`, `volume: f32`
- `fx: EffectChain` — per-track insert effects (currently empty)

`DrumMachine` maintains:
- A polyphonic `Vec<DrumVoice>` pool — all currently sounding hits
- A master `fx: EffectChain` for the summed drum bus
- `swing: f32` — global swing/shuffle amount (0.0–0.5)
- Hi-hat choke: triggering ClosedHat kills all ringing OpenHat voices

All drum sounds are synthesized with XOR-shift noise and phase-accumulated oscillators
(no samples). Key parameters per sound:

| Sound | Technique |
|-------|-----------|
| Kick | Sine pitch sweep 150→50 Hz + transient click |
| Snare | Noise + 195 Hz body tone |
| C-Hat | Very short noise burst (~60 ms) |
| O-Hat | Longer noise decay (~380 ms), choked by C-Hat |
| Clap | 3 staggered noise bursts (0/9/17 ms) + decaying body |
| Toms | Sine pitch sweep + noise; different freq/decay per tom |

## Effects (`effects.rs`)

### EffectChain / AudioEffect trait

```rust
pub trait AudioEffect: Send {
    fn process(&mut self, sample: f32) -> f32;
    fn name(&self) -> &'static str;
    fn reset(&mut self);
}

pub struct EffectChain { pub effects: Vec<Box<dyn AudioEffect>> }
```

`EffectChain::process()` short-circuits to a direct return when empty (zero overhead).
Every instrument bus (`Synth::fx`, `DrumMachine::fx`) and every track (`DrumTrack::fx`)
already owns an `EffectChain`. To add an effect, implement the trait and push an instance.

### BiquadFilter

Two-pole biquad filter (RBJ Audio EQ Cookbook). **Not** part of `EffectChain` — applied
directly on each melodic bus before the chain, so it sits between the voice mix and any
send effects.

```rust
pub struct BiquadFilter {
    pub enabled: bool,
    pub mode:    FilterMode,   // LowPass / HighPass / BandPass
    pub cutoff:  f32,          // Hz, 80–18 000
    pub q:       f32,          // 0.5–10.0
    // internal: cached coefficients, Direct Form I state
}
```

- `FilterMode::next()` / `prev()` cycle LP→HP→BP.
- Coefficients are cached and only recomputed when `cutoff`, `q`, or `mode` changes.
- `reset_state()` clears the delay elements; called automatically when toggling ON to
  prevent pops.
- `process()` returns the input sample unchanged when `enabled = false` (zero cost).

**Signal path per bus:**
```
voice mix (polyphony-normalised) → BiquadFilter → EffectChain → FX sends
```

**Controls (Effects panel, rows 5–6):**

| Param col | Action |
|-----------|--------|
| 0 (Type)   | `=` / `-` cycle LP / HP / BP |
| 1 (Cutoff) | `=` / `-` ×÷ 1.0595 (one semitone); holds down for smooth sweep |
| 2 (Q)      | `=` / `-` ±0.1 |

`[Enter]` toggles on/off. Rows 5–6 have no routing sends (filter is a bus insert, not a parallel send). Active filters show `▶F1` / `▶F2` in the title bar.

## Melodic sequencer (`sequencer.rs`)

- `steps: Vec<Option<u8>>` — MIDI note per step (`None` = rest)
- 16th-note steps; step count cycles 8→16→24→32→8
- `tick(bpm)` called once per audio sample; returns `StepEvent{note_on, note_off}` at
  step boundaries
- Removing `bpm` from `Sequencer` and passing it at call-site was deliberate so BPM is
  controlled from one place (`Synth::bpm`)

## UI (`ui.rs`)

```
Title (3 lines)
Piano panel
SynthSeq grid
SynthSeq2 grid
Drum grid
Effects panel
Status (4 lines)   — wave, BPM, volume, playing notes
Scope (6 lines)    — braille oscilloscope
Help (remaining)   — mode-specific key hints
```

`draw_drums()` renders: 1 header line (BPM / Steps / play status / Swing%) +
1 step-number row + 8 track rows. Step cells use probability shading:
`·` (0%), `░` (1–33%), `▒` (34–66%), `▓` (67–99%), `█` (100%).
Beat groups of 4 are separated by `┆`.
Playhead = green bg, cursor = yellow bg, playhead+cursor = cyan bg.

## Key things to know for future work

- **Adding a new send effect**: implement `AudioEffect`, push onto the relevant `EffectChain`.
  No other changes needed — the chain is already wired into every bus/track.
- **Adding a filter to the drum bus**: add a `BiquadFilter` field to `DrumMachine` and apply
  it in `generate_sample()` before `self.fx.process()`. Same pattern as `filter1`/`filter2`
  on `Synth`. Expose it in the Effects panel as a new row (extend `effects_sel` to 7).
- **Adding a new drum sound**: add variant to `DrumKind::ALL`, implement a synthesis
  function in `DrumVoice`, add a `DrumTrack` in `DrumMachine::new()`.
- **Adding a new waveform**: extend `WaveType` enum in `synth.rs`.
- **Swing for melodic sequencers**: `DrumMachine::swing` pattern is self-contained —
  add `swing: f32` to `Sequencer` and apply the same odd-step offset in `tick()`.
- **MIDI/OSC input**: would hook into `app.rs` methods (`key_press`, `seq_set_note`,
  `drum_toggle_step`, etc.) — all side-effects go through `Arc<Mutex<Synth>>`.
- **Stereo**: `AudioEngine` already writes the same mono sample to all channels. A stereo
  `EffectChain` would need a new trait or a paired mono-chain approach.
- **The audio callback acquires the mutex on every frame.** If the UI thread holds the
  lock for too long, you will get audio dropouts. Keep lock durations short.
