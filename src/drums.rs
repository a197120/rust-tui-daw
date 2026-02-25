use std::f32::consts::PI;
use crate::effects::EffectChain;

// ── Drum kind ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrumKind {
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
    Clap,
    LowTom,
    MidTom,
    HighTom,
}

impl DrumKind {
    pub const ALL: [DrumKind; 8] = [
        DrumKind::Kick,
        DrumKind::Snare,
        DrumKind::ClosedHat,
        DrumKind::OpenHat,
        DrumKind::Clap,
        DrumKind::LowTom,
        DrumKind::MidTom,
        DrumKind::HighTom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Kick      => "Kick ",
            Self::Snare     => "Snare",
            Self::ClosedHat => "C-Hat",
            Self::OpenHat   => "O-Hat",
            Self::Clap      => "Clap ",
            Self::LowTom    => "L.Tom",
            Self::MidTom    => "M.Tom",
            Self::HighTom   => "H.Tom",
        }
    }

    /// Maximum duration (seconds) – the voice is dropped after this.
    fn duration(self) -> f32 {
        match self {
            Self::Kick      => 0.50,
            Self::Snare     => 0.20,
            Self::ClosedHat => 0.06,
            Self::OpenHat   => 0.38,
            Self::Clap      => 0.22,
            Self::LowTom    => 0.62,
            Self::MidTom    => 0.42,
            Self::HighTom   => 0.30,
        }
    }
}

// ── Noise ─────────────────────────────────────────────────────────────────────

/// Fast XOR-shift PRNG.  Returns values uniformly in [-1, 1].
#[inline(always)]
fn xorshift(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as i32 as f32) * (1.0 / i32::MAX as f32)
}

// ── Single drum voice ─────────────────────────────────────────────────────────

/// One triggered drum hit.  Generates samples until it naturally decays.
/// Multiple voices of the same (or different) kind run in parallel inside
/// `DrumMachine::voices`, giving full polyphony.
struct DrumVoice {
    kind: DrumKind,
    sample_pos: u64,
    dur_samples: u64,
    /// Phase accumulator for tonal components (0..1 normalised).
    phase: f32,
    /// XOR-shift state — unique per voice so simultaneous hits differ.
    noise: u32,
    sample_rate: f32,
    volume: f32,
}

impl DrumVoice {
    fn new(kind: DrumKind, sample_rate: f32, seed: u32, volume: f32) -> Self {
        Self {
            kind,
            sample_pos: 0,
            dur_samples: (kind.duration() * sample_rate).ceil() as u64,
            phase: 0.0,
            noise: seed | 1, // xorshift must never be 0
            sample_rate,
            volume,
        }
    }

    #[inline]
    fn is_finished(&self) -> bool {
        self.sample_pos >= self.dur_samples
    }

    fn next_sample(&mut self) -> f32 {
        if self.is_finished() {
            return 0.0;
        }
        let t = self.sample_pos as f32 / self.sample_rate;
        let raw = match self.kind {
            DrumKind::Kick      => self.kick(t),
            DrumKind::Snare     => self.snare(t),
            DrumKind::ClosedHat => self.closed_hat(t),
            DrumKind::OpenHat   => self.open_hat(t),
            DrumKind::Clap      => self.clap(t),
            DrumKind::LowTom    => self.tom(t, 110.0,  52.0, 0.55),
            DrumKind::MidTom    => self.tom(t, 195.0,  90.0, 0.38),
            DrumKind::HighTom   => self.tom(t, 275.0, 140.0, 0.26),
        };
        self.sample_pos += 1;
        (raw * self.volume).clamp(-1.0, 1.0)
    }

    // ── Synthesis helpers ─────────────────────────────────────────────────

    #[inline]
    fn noise(&mut self) -> f32 {
        xorshift(&mut self.noise)
    }

    /// Advance the phase accumulator and return a sine value.
    #[inline]
    fn sine(&mut self, freq: f32) -> f32 {
        self.phase += freq / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        (self.phase * 2.0 * PI).sin()
    }

    // ── Individual drum synthesisers ──────────────────────────────────────

    fn kick(&mut self, t: f32) -> f32 {
        // Exponential pitch sweep 150 → 50 Hz, fast transient click
        let freq = 50.0 + 100.0 * (-t * 32.0_f32).exp();
        let tone = self.sine(freq);
        let amp  = (-t * 11.0_f32).exp();
        let click = if t < 0.004 { self.noise() * 0.38 } else { 0.0 };
        (tone * 0.88 + click) * amp
    }

    fn snare(&mut self, t: f32) -> f32 {
        let noise = self.noise();
        let tone  = self.sine(195.0);
        let amp   = (-t * 24.0_f32).exp();
        (noise * 0.72 + tone * 0.28) * amp
    }

    fn closed_hat(&mut self, t: f32) -> f32 {
        // Very short burst of high-frequency noise
        self.noise() * (-t * 85.0_f32).exp()
    }

    fn open_hat(&mut self, t: f32) -> f32 {
        self.noise() * (-t * 8.5_f32).exp()
    }

    fn clap(&mut self, t: f32) -> f32 {
        let noise = self.noise();
        let t_ms  = t * 1000.0;
        // Three staggered transient bursts that mimic a physical hand clap
        let burst = if      t_ms <  4.0 { 1.00 }
                    else if t_ms <  9.0 { 0.00 }
                    else if t_ms < 13.0 { 0.82 }
                    else if t_ms < 17.0 { 0.00 }
                    else if t_ms < 21.0 { 0.62 }
                    else                { 0.00 };
        // Decaying body that starts after the transients
        let body = if t > 0.024 { (-(t - 0.024) * 22.0_f32).exp() * 0.42 } else { 0.0 };
        noise * (burst + body)
    }

    fn tom(&mut self, t: f32, start_hz: f32, end_hz: f32, decay_s: f32) -> f32 {
        let freq  = end_hz + (start_hz - end_hz) * (-t * 22.0_f32).exp();
        let tone  = self.sine(freq);
        let noise = self.noise();
        let amp   = (-t / decay_s).exp();
        (tone * 0.80 + noise * 0.20) * amp
    }
}

// ── Drum track ────────────────────────────────────────────────────────────────

/// One row in the drum machine: a drum instrument, its step pattern,
/// and a per-track effects insert chain.
pub struct DrumTrack {
    pub kind:  DrumKind,
    pub steps: Vec<u8>,
    pub muted: bool,
    pub volume: f32,
    /// Per-track insert effects (e.g. compression, EQ). Empty = passthrough.
    #[allow(dead_code)]
    pub fx: EffectChain,
}

impl DrumTrack {
    fn new(kind: DrumKind, num_steps: usize) -> Self {
        Self {
            kind,
            steps: vec![0u8; num_steps],
            muted: false,
            volume: 0.85,
            fx: EffectChain::new(),
        }
    }
}

// ── Drum machine ──────────────────────────────────────────────────────────────

/// 8-track polyphonic step sequencer with synthesised drum voices.
///
/// BPM is supplied externally from `Synth::bpm` so the drum machine always
/// stays locked to the melodic sequencer without a separate clock.
///
/// Each track owns a per-insert `EffectChain`; the whole drum bus also has a
/// master `EffectChain` — both are ready for reverb, compression, etc. later.
pub struct DrumMachine {
    pub tracks:       Vec<DrumTrack>,
    pub num_steps:    usize,
    pub current_step: usize,
    pub playing:      bool,
    pub swing:        f32,  // 0.0 = straight, ~0.33 = shuffle, 0.5 = maximum
    /// Master insert effects applied to the summed drum bus output.
    pub fx: EffectChain,

    sample_rate: f32,
    /// Polyphonic voice pool — all currently sounding drum hits.
    voices: Vec<DrumVoice>,
    /// Seed advanced before each trigger so every hit has a distinct noise flavour.
    seed: u32,
    /// Separate XOR-shift seed used only for probability rolls.
    prob_seed: u32,
    /// Set to true each sample that a kick fires; cleared by Synth::generate_sample.
    pub kick_triggered: bool,
}

impl DrumMachine {
    pub fn new(sample_rate: f32) -> Self {
        let num_steps = 16;
        let tracks = DrumKind::ALL.iter().map(|&k| DrumTrack::new(k, num_steps)).collect();
        Self {
            tracks,
            num_steps,
            current_step: 0,
            playing: false,
            swing: 0.0,
            fx: EffectChain::new(),
            sample_rate,
            voices: Vec::with_capacity(32),
            seed: 0xBEEF_CAFE,
            prob_seed: 0xDEAD_BEEF,
            kick_triggered: false,
        }
    }

    fn samples_per_step(&self, bpm: f32) -> u64 {
        ((self.sample_rate * 60.0) / (bpm * 4.0)).round() as u64
    }

    /// Generate the next audio sample.  Called once per sample from the audio
    /// thread inside `Synth::generate_sample`, using the shared master clock.
    pub fn generate_sample(&mut self, bpm: f32, clock: u64) -> f32 {
        let sps = self.samples_per_step(bpm).max(1);
        let step_idx = (clock / sps) as usize % self.num_steps;
        let phase_in = clock % sps;

        // Odd steps are delayed by swing fraction of one step width
        let swing_offset = if step_idx % 2 == 1 {
            (self.swing * sps as f32).round() as u64
        } else {
            0
        };

        if self.playing && phase_in == swing_offset {
            self.current_step = step_idx;
            self.fire_step();
        } else {
            self.current_step = step_idx;
        }

        // Mix all active drum voices, apply per-track fx, then sum
        let mut mix = 0.0f32;
        for v in &mut self.voices {
            mix += v.next_sample();
        }
        self.voices.retain(|v| !v.is_finished());

        // Master bus fx chain (empty = passthrough)
        let out = self.fx.process(mix);

        // Gentle headroom scaling + soft clip
        (out * 0.22).tanh()
    }

    fn fire_step(&mut self) {
        // Hi-hat choke: kill any ringing open hat when a closed hat fires.
        let closed_fires = self.tracks.iter().any(|t| {
            t.kind == DrumKind::ClosedHat
                && !t.muted
                && t.steps.get(self.current_step).copied().unwrap_or(0) > 0
        });
        if closed_fires {
            self.voices.retain(|v| v.kind != DrumKind::OpenHat);
        }

        for track in &mut self.tracks {
            if track.muted { continue; }
            let prob = track.steps.get(self.current_step).copied().unwrap_or(0);
            if prob == 0 { continue; }

            // Probability roll
            if prob < 100 {
                self.prob_seed ^= self.prob_seed << 13;
                self.prob_seed ^= self.prob_seed >> 17;
                self.prob_seed ^= self.prob_seed << 5;
                let roll = (self.prob_seed % 100) as u8;
                if roll >= prob { continue; }
            }

            // Unique noise seed per trigger for timbral variation
            self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if track.kind == DrumKind::Kick {
                self.kick_triggered = true;
            }
            self.voices.push(DrumVoice::new(track.kind, self.sample_rate, self.seed, track.volume));
        }
    }

    /// Immediately trigger a drum track (live preview / keyboard playing).
    /// Fully polyphonic — does not stop any already-playing voices.
    pub fn trigger_now(&mut self, track_idx: usize) {
        let Some(track) = self.tracks.get(track_idx) else { return };
        if track.muted { return; }

        if track.kind == DrumKind::ClosedHat {
            self.voices.retain(|v| v.kind != DrumKind::OpenHat);
        }

        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.voices.push(DrumVoice::new(track.kind, self.sample_rate, self.seed, track.volume));
    }

    pub fn toggle_play(&mut self) {
        self.playing = !self.playing;
        if !self.playing {
            self.voices.clear();
        }
    }

    pub fn toggle_step(&mut self, track: usize, step: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            if let Some(s) = t.steps.get_mut(step) {
                if *s == 0 { *s = 100; } else { *s = 0; }
            }
        }
    }

    pub fn clear_step(&mut self, track: usize, step: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            if let Some(s) = t.steps.get_mut(step) {
                *s = 0;
            }
        }
    }

    pub fn toggle_mute(&mut self, track: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.muted = !t.muted;
        }
    }

    pub fn track_volume_up(&mut self, track: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.volume = (t.volume + 0.05).min(1.0);
        }
    }

    pub fn track_volume_down(&mut self, track: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.volume = (t.volume - 0.05).max(0.0);
        }
    }

    pub fn cycle_num_steps(&mut self) {
        let next = match self.num_steps {
            8  => 16,
            16 => 24,
            24 => 32,
            _  => 8,
        };
        self.num_steps = next;
        for t in &mut self.tracks {
            t.steps.resize(next, 0);
        }
        if self.current_step >= next {
            self.current_step = 0;
        }
    }

    pub fn step_prob_up(&mut self, track: usize, step: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            if let Some(s) = t.steps.get_mut(step) {
                *s = (*s + 25).min(100);
                if *s == 0 { *s = 25; }
            }
        }
    }

    pub fn step_prob_down(&mut self, track: usize, step: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            if let Some(s) = t.steps.get_mut(step) {
                if *s <= 25 { *s = 0; } else { *s -= 25; }
            }
        }
    }

    pub fn euclidean_fill(&mut self, track: usize, k: usize) {
        let n = self.num_steps;
        if let Some(t) = self.tracks.get_mut(track) {
            let k = k.min(n);
            t.steps = vec![0u8; n];
            let mut bucket = 0usize;
            for i in 0..n {
                bucket += k;
                if bucket >= n { bucket -= n; t.steps[i] = 100; }
            }
        }
    }
}
