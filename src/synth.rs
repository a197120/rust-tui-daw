use std::collections::HashMap;
use std::f32::consts::PI;

use crate::drums::DrumMachine;
use crate::effects::{AudioEffect, Delay, Distortion, EffectChain, Reverb};
use crate::sequencer::Sequencer;

// ── Waveform ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaveType { Sine, Square, Sawtooth, Triangle }

impl WaveType {
    pub fn next(self) -> Self {
        match self {
            Self::Sine => Self::Square, Self::Square => Self::Sawtooth,
            Self::Sawtooth => Self::Triangle, Self::Triangle => Self::Sine,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Sine => "Sine", Self::Square => "Square",
            Self::Sawtooth => "Sawtooth", Self::Triangle => "Triangle",
        }
    }
}

// ── ADSR envelope ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvelopeStage { Attack, Decay, Sustain, Release, Off }

// ── Melodic voice ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Voice {
    pub frequency:     f32,
    pub phase:         f32,
    pub stage:         EnvelopeStage,
    pub level:         f32,
    pub release_level: f32,
}

impl Voice {
    pub fn new(note: u8) -> Self {
        Self { frequency: note_to_freq(note), phase: 0.0,
               stage: EnvelopeStage::Attack, level: 0.0, release_level: 0.0 }
    }

    pub fn release(&mut self) {
        if self.stage != EnvelopeStage::Off {
            self.release_level = self.level;
            self.stage = EnvelopeStage::Release;
        }
    }

    pub fn is_finished(&self) -> bool { self.stage == EnvelopeStage::Off }

    pub fn next_sample(&mut self, sr: f32, wave: WaveType,
                       attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
        let dt = 1.0 / sr;
        match self.stage {
            EnvelopeStage::Attack => {
                self.level += dt / attack;
                if self.level >= 1.0 { self.level = 1.0; self.stage = EnvelopeStage::Decay; }
            }
            EnvelopeStage::Decay => {
                self.level -= dt * (1.0 - sustain) / decay;
                if self.level <= sustain { self.level = sustain; self.stage = EnvelopeStage::Sustain; }
            }
            EnvelopeStage::Sustain => { self.level = sustain; }
            EnvelopeStage::Release => {
                self.level -= dt * self.release_level / release;
                if self.level <= 0.0 { self.level = 0.0; self.stage = EnvelopeStage::Off; }
            }
            EnvelopeStage::Off => return 0.0,
        }

        let sample = match wave {
            WaveType::Sine     => (self.phase * 2.0 * PI).sin(),
            WaveType::Square   => if (self.phase * 2.0 * PI).sin() >= 0.0 { 1.0 } else { -1.0 },
            WaveType::Sawtooth => 2.0 * self.phase - 1.0,
            WaveType::Triangle => {
                if self.phase < 0.5 { 4.0 * self.phase - 1.0 } else { 3.0 - 4.0 * self.phase }
            }
        };

        self.phase += self.frequency / sr;
        if self.phase >= 1.0 { self.phase -= 1.0; }
        sample * self.level
    }
}

// ── Per-instrument FX send routing ────────────────────────────────────────────

/// Send levels (0.0–1.0) from each instrument bus to each master effect.
/// Dry signal always passes through; routing additionally sends a weighted
/// copy into the effect's wet bus.
pub struct FxRouting {
    pub s1_reverb: f32, pub s1_delay: f32, pub s1_dist: f32,
    pub s2_reverb: f32, pub s2_delay: f32, pub s2_dist: f32,
    pub dr_reverb: f32, pub dr_delay: f32, pub dr_dist: f32,
}

impl FxRouting {
    pub fn new() -> Self {
        Self {
            s1_reverb: 0.0, s1_delay: 0.0, s1_dist: 0.0,
            s2_reverb: 0.0, s2_delay: 0.0, s2_dist: 0.0,
            dr_reverb: 0.0, dr_delay: 0.0, dr_dist: 0.0,
        }
    }
}

// ── Synth ─────────────────────────────────────────────────────────────────────

pub struct Synth {
    pub sample_rate: f32,
    pub bpm:         f32,       // master clock shared by all sequencers
    pub master_clock: u64,      // incremented every sample

    // ── Synth 1 ───────────────────────────────────────────────────────────
    pub wave_type:   WaveType,
    pub voices:      HashMap<u8, Voice>,
    pub attack:  f32,
    pub decay:   f32,
    pub sustain: f32,
    pub release: f32,
    pub volume:  f32,
    pub sequencer:    Sequencer,
    /// Insert effects applied to the melodic synth 1 bus.
    pub fx: EffectChain,

    // ── Synth 2 (sequencer-driven) ────────────────────────────────────────
    pub wave_type2:  WaveType,
    pub voices2:     HashMap<u8, Voice>,
    pub attack2:  f32,
    pub decay2:   f32,
    pub sustain2: f32,
    pub release2: f32,
    pub volume2:  f32,
    pub sequencer2:   Sequencer,
    /// Insert effects applied to the melodic synth 2 bus.
    pub fx2: EffectChain,

    // ── Drum machine ──────────────────────────────────────────────────────
    pub drum_machine: DrumMachine,

    // ── Master effects (parallel aux-send, wet-only output) ───────────────
    pub reverb:     Reverb,
    pub delay:      Delay,
    pub distortion: Distortion,

    // ── Per-instrument send routing ───────────────────────────────────────
    pub fx_routing: FxRouting,
}

impl Synth {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            bpm:          120.0,
            master_clock: 0,

            wave_type:  WaveType::Sine,
            voices:     HashMap::new(),
            attack:  0.01, decay: 0.1, sustain: 0.7, release: 0.3,
            volume:  0.5,
            sequencer:    Sequencer::new(sample_rate),
            fx:           EffectChain::new(),

            wave_type2: WaveType::Sine,
            voices2:    HashMap::new(),
            attack2: 0.01, decay2: 0.1, sustain2: 0.7, release2: 0.3,
            volume2: 0.5,
            sequencer2:   Sequencer::new(sample_rate),
            fx2:          EffectChain::new(),

            drum_machine: DrumMachine::new(sample_rate),

            reverb:      Reverb::new(),
            delay:       Delay::new(sample_rate),
            distortion:  Distortion::new(),

            fx_routing:  FxRouting::new(),
        }
    }

    // ── Synth 1 note control ──────────────────────────────────────────────

    pub fn note_on(&mut self, note: u8) {
        self.voices.insert(note, Voice::new(note));
    }

    pub fn note_off(&mut self, note: u8) {
        if let Some(v) = self.voices.get_mut(&note) { v.release(); }
    }

    pub fn active_notes(&self) -> Vec<u8> {
        self.voices.keys().copied().collect()
    }

    // ── Synth 2 note control ──────────────────────────────────────────────

    #[allow(dead_code)]
    pub fn note_on2(&mut self, note: u8) {
        self.voices2.insert(note, Voice::new(note));
    }

    pub fn note_off2(&mut self, note: u8) {
        if let Some(v) = self.voices2.get_mut(&note) { v.release(); }
    }

    #[allow(dead_code)]
    pub fn active_notes2(&self) -> Vec<u8> {
        self.voices2.keys().copied().collect()
    }

    // ── Audio render ──────────────────────────────────────────────────────

    pub fn generate_sample(&mut self) -> f32 {
        let clock = self.master_clock;
        self.master_clock += 1;

        // ── Sequencer 1 ───────────────────────────────────────────────────
        if let Some(ev) = self.sequencer.tick(self.bpm, clock) {
            if let Some(n) = ev.note_off { if let Some(v) = self.voices.get_mut(&n) { v.release(); } }
            if let Some(n) = ev.note_on  { self.voices.insert(n, Voice::new(n)); }
        }

        // ── Sequencer 2 ───────────────────────────────────────────────────
        if let Some(ev) = self.sequencer2.tick(self.bpm, clock) {
            if let Some(n) = ev.note_off { if let Some(v) = self.voices2.get_mut(&n) { v.release(); } }
            if let Some(n) = ev.note_on  { self.voices2.insert(n, Voice::new(n)); }
        }

        // ── Melodic bus 1 ─────────────────────────────────────────────────
        let sr   = self.sample_rate;
        let wave = self.wave_type;
        let (a, d, s, r) = (self.attack, self.decay, self.sustain, self.release);
        let mut mel1 = 0.0f32;
        for v in self.voices.values_mut() { mel1 += v.next_sample(sr, wave, a, d, s, r); }
        self.voices.retain(|_, v| !v.is_finished());
        let mel1_out = self.fx.process(mel1 * self.volume / (self.voices.len().max(1) as f32).sqrt());

        // ── Melodic bus 2 ─────────────────────────────────────────────────
        let wave2 = self.wave_type2;
        let (a2, d2, s2, r2) = (self.attack2, self.decay2, self.sustain2, self.release2);
        let mut mel2 = 0.0f32;
        for v in self.voices2.values_mut() { mel2 += v.next_sample(sr, wave2, a2, d2, s2, r2); }
        self.voices2.retain(|_, v| !v.is_finished());
        let mel2_out = self.fx2.process(mel2 * self.volume2 / (self.voices2.len().max(1) as f32).sqrt());

        // ── Drum bus ──────────────────────────────────────────────────────
        let drum_out = self.drum_machine.generate_sample(self.bpm, clock) * self.volume;

        // ── Master mix (always dry) ───────────────────────────────────────
        let dry = (mel1_out + mel2_out + drum_out).tanh();

        // ── FX sends (wet-only, parallel) ─────────────────────────────────
        // Copy routing values out to avoid split-borrow conflicts.
        let (s1_rev, s1_dly, s1_dst,
             s2_rev, s2_dly, s2_dst,
             dr_rev, dr_dly, dr_dst) = {
            let rt = &self.fx_routing;
            (rt.s1_reverb, rt.s1_delay, rt.s1_dist,
             rt.s2_reverb, rt.s2_delay, rt.s2_dist,
             rt.dr_reverb, rt.dr_delay, rt.dr_dist)
        };

        let rev_wet = self.reverb.process(
            s1_rev * mel1_out + s2_rev * mel2_out + dr_rev * drum_out);
        let dly_wet = self.delay.process(
            s1_dly * mel1_out + s2_dly * mel2_out + dr_dly * drum_out);
        let dst_wet = self.distortion.process(
            (s1_dst * mel1_out + s2_dst * mel2_out + dr_dst * drum_out).tanh());

        (dry + rev_wet + dly_wet + dst_wet).tanh()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn note_to_freq(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

pub fn note_name(note: u8) -> String {
    let names = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
    format!("{}{}", names[(note % 12) as usize], (note / 12) as i32 - 1)
}
