use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::drums::DrumKind;
use crate::effects::FilterMode;
use crate::save::{DelaySave, DistSave, DrumsSave, FilterSave, ReverbSave, RoutingSave,
                  SaveFile, SeqSave, SidechainSave, TrackSave};
use crate::scale::{Scale, ScaleQuantizer};
use crate::synth::{Synth, WaveType, note_name};

const FALLBACK_RELEASE_THRESHOLD: Duration = Duration::from_millis(600);

// ── Key → MIDI note mapping ───────────────────────────────────────────────────

pub fn key_to_note(key: char, base_octave: i32) -> Option<u8> {
    let (st, oct): (i32, i32) = match key {
        // Lower row – white keys
        'z' => (0,0), 'x' => (2,0), 'c' => (4,0), 'v' => (5,0),
        'b' => (7,0), 'n' => (9,0), 'm' => (11,0),
        ',' => (12,0), '.' => (14,0), '/' => (16,0),
        // Lower row – black keys
        's' => (1,0), 'd' => (3,0), 'g' => (6,0),
        'h' => (8,0), 'j' => (10,0), 'l' => (13,0), ';' => (15,0),
        // Upper row – white keys
        'q' => (0,1), 'w' => (2,1), 'e' => (4,1), 'r' => (5,1),
        't' => (7,1), 'y' => (9,1), 'u' => (11,1),
        'i' => (12,1), 'o' => (14,1), 'p' => (16,1),
        // Upper row – black keys
        '2' => (1,1), '3' => (3,1), '5' => (6,1),
        '6' => (8,1), '7' => (10,1), '9' => (13,1), '0' => (15,1),
        _ => return None,
    };
    let note = (base_octave + oct) * 12 + 12 + st;
    if (0..=127).contains(&note) { Some(note as u8) } else { None }
}

// ── App mode ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    /// Live keyboard play.
    Play,
    /// Edit the melodic step sequencer 1.
    SynthSeq,
    /// Edit the melodic step sequencer 2.
    SynthSeq2,
    /// Edit the drum machine.
    Drums,
    /// Adjust master output effects.
    Effects,
}

// ── Input mode (file path prompt) ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    None,
    Save,
    Load,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub synth:        Arc<Mutex<Synth>>,
    pub base_octave:  i32,
    pub pressed_keys: HashSet<char>,
    key_last_seen:    HashMap<char, Instant>,
    pub active_notes: Vec<u8>,
    pub should_quit:  bool,
    pub status_msg:   String,

    pub mode: AppMode,

    // Melodic sequencer 1 cursor
    pub seq_cursor: usize,

    // Melodic sequencer 2 cursor
    pub seq2_cursor: usize,

    // Drum machine cursors
    pub drum_track: usize,  // selected track (row)
    pub drum_step:  usize,  // selected step (column)

    // Effects panel cursors
    pub effects_sel:   usize,  // 0=Reverb 1=Delay 2=Distortion
    pub effects_param: usize,  // 0-2 = effect param; 3-5 = S1/S2/DR send level

    // Scale quantizer (input layer — no audio thread involvement)
    pub scale_q: ScaleQuantizer,

    // File path prompt state
    pub input_mode: InputMode,
    pub input_buf:  String,
}

impl App {
    pub fn new(synth: Arc<Mutex<Synth>>) -> Self {
        Self {
            synth,
            base_octave:  4,
            pressed_keys: HashSet::new(),
            key_last_seen: HashMap::new(),
            active_notes: Vec::new(),
            should_quit:  false,
            status_msg:   String::new(),
            mode:         AppMode::Play,
            seq_cursor:   0,
            seq2_cursor:  0,
            drum_track:   0,
            drum_step:    0,
            effects_sel:   0,
            effects_param: 0,
            scale_q:       ScaleQuantizer::new(),
            input_mode:    InputMode::None,
            input_buf:     String::new(),
        }
    }

    // ── Keyboard / note playback ──────────────────────────────────────────

    pub fn key_press(&mut self, key: char) {
        if self.pressed_keys.contains(&key) { return; }
        self.pressed_keys.insert(key);
        if let Some(note) = key_to_note(key, self.base_octave) {
            self.synth.lock().unwrap().note_on(self.scale_q.quantize(note));
        }
    }

    pub fn key_release(&mut self, key: char) {
        if !self.pressed_keys.remove(&key) { return; }
        if let Some(note) = key_to_note(key, self.base_octave) {
            self.synth.lock().unwrap().note_off(self.scale_q.quantize(note));
        }
    }

    pub fn key_press_fallback(&mut self, key: char) {
        self.key_last_seen.insert(key, Instant::now());
        if self.pressed_keys.contains(&key) { return; }
        self.pressed_keys.insert(key);
        if let Some(note) = key_to_note(key, self.base_octave) {
            self.synth.lock().unwrap().note_on(self.scale_q.quantize(note));
        }
    }

    pub fn tick_fallback_release(&mut self) {
        let now = Instant::now();
        let stale: Vec<char> = self.pressed_keys.iter().copied()
            .filter(|k| {
                key_to_note(*k, self.base_octave).is_some()
                    && self.key_last_seen.get(k)
                        .map(|t| now.duration_since(*t) >= FALLBACK_RELEASE_THRESHOLD)
                        .unwrap_or(true)
            })
            .collect();
        for k in stale { self.key_last_seen.remove(&k); self.key_release(k); }
    }

    pub fn release_all(&mut self) {
        let keys: Vec<char> = self.pressed_keys.iter().copied().collect();
        for k in keys { self.key_release(k); }
        self.key_last_seen.clear();
    }

    // ── Global controls ───────────────────────────────────────────────────

    pub fn octave_up(&mut self) {
        if self.base_octave < 8 {
            self.release_all();
            self.base_octave += 1;
            self.status_msg = format!("Octave: {}", self.base_octave);
        }
    }

    pub fn octave_down(&mut self) {
        if self.base_octave > 0 {
            self.release_all();
            self.base_octave -= 1;
            self.status_msg = format!("Octave: {}", self.base_octave);
        }
    }

    pub fn cycle_wave(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.wave_type = s.wave_type.next();
        self.status_msg = format!("Wave: {}", s.wave_type.name());
    }

    pub fn cycle_wave2(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.wave_type2 = s.wave_type2.next();
        self.status_msg = format!("Synth2 Wave: {}", s.wave_type2.name());
    }

    pub fn volume_up(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.volume = (s.volume + 0.05).min(1.0);
        self.status_msg = format!("Vol: {:.0}%", s.volume * 100.0);
    }

    pub fn volume_down(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.volume = (s.volume - 0.05).max(0.0);
        self.status_msg = format!("Vol: {:.0}%", s.volume * 100.0);
    }

    pub fn synth2_vol_up(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.volume2 = (s.volume2 + 0.05).min(1.0);
        self.status_msg = format!("Synth2 Vol: {:.0}%", s.volume2 * 100.0);
    }

    pub fn synth2_vol_down(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.volume2 = (s.volume2 - 0.05).max(0.0);
        self.status_msg = format!("Synth2 Vol: {:.0}%", s.volume2 * 100.0);
    }

    /// Shared master BPM — affects both the melodic and drum sequencers.
    pub fn bpm_up(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.bpm = (s.bpm + 5.0).clamp(30.0, 300.0);
        self.status_msg = format!("BPM: {:.0}", s.bpm);
    }

    pub fn bpm_down(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.bpm = (s.bpm - 5.0).clamp(30.0, 300.0);
        self.status_msg = format!("BPM: {:.0}", s.bpm);
    }

    pub fn cycle_scale(&mut self) {
        self.release_all();
        self.scale_q.scale = self.scale_q.scale.next();
        self.status_msg = if self.scale_q.scale == Scale::Off {
            "Scale: Off".to_string()
        } else {
            format!("Scale: {} {}", self.scale_q.root_name(), self.scale_q.scale.name())
        };
    }

    pub fn cycle_scale_root(&mut self) {
        self.release_all();
        self.scale_q.cycle_root();
        self.status_msg = if self.scale_q.scale == Scale::Off {
            format!("Root: {}", self.scale_q.root_name())
        } else {
            format!("Scale: {} {}", self.scale_q.root_name(), self.scale_q.scale.name())
        };
    }

    pub fn refresh_active_notes(&mut self) {
        self.active_notes = self.synth.lock().unwrap().active_notes();
    }

    // ── UI read helpers ───────────────────────────────────────────────────

    pub fn wave_name(&self) -> String {
        self.synth.lock().unwrap().wave_type.name().to_string()
    }

    pub fn volume(&self) -> f32 { self.synth.lock().unwrap().volume }

    pub fn active_note_names(&self) -> Vec<String> {
        let mut notes = self.active_notes.clone();
        notes.sort();
        notes.iter().map(|&n| note_name(n)).collect()
    }

    pub fn highlighted_notes(&self) -> HashSet<u8> {
        self.active_notes.iter().copied().collect()
    }

    pub fn seq_playing(&self) -> bool {
        self.synth.lock().unwrap().sequencer.playing
    }

    pub fn seq2_playing(&self) -> bool {
        self.synth.lock().unwrap().sequencer2.playing
    }

    pub fn drum_playing(&self) -> bool {
        self.synth.lock().unwrap().drum_machine.playing
    }

    // ── Mode cycling ──────────────────────────────────────────────────────

    /// Cycle focus: Keyboard → SynthSeq → SynthSeq2 → Drums → Effects → Keyboard.
    pub fn toggle_mode(&mut self) {
        self.release_all();
        self.mode = match self.mode {
            AppMode::Play      => AppMode::SynthSeq,
            AppMode::SynthSeq  => AppMode::SynthSeq2,
            AppMode::SynthSeq2 => AppMode::Drums,
            AppMode::Drums     => AppMode::Effects,
            AppMode::Effects   => AppMode::Play,
        };
        self.status_msg = match self.mode {
            AppMode::Play      => "Focus: Keyboard".to_string(),
            AppMode::SynthSeq  => "Focus: Synth Seq".to_string(),
            AppMode::SynthSeq2 => "Focus: Synth Seq 2".to_string(),
            AppMode::Drums     => "Focus: Drums".to_string(),
            AppMode::Effects   => "Focus: Effects".to_string(),
        };
    }

    // ── Melodic sequencer 1 controls ──────────────────────────────────────

    pub fn seq_cursor_left(&mut self) {
        let n = self.synth.lock().unwrap().sequencer.num_steps;
        self.seq_cursor = if self.seq_cursor == 0 { n - 1 } else { self.seq_cursor - 1 };
    }

    pub fn seq_cursor_right(&mut self) {
        let n = self.synth.lock().unwrap().sequencer.num_steps;
        self.seq_cursor = (self.seq_cursor + 1) % n;
    }

    pub fn seq_set_note(&mut self, key: char) {
        let Some(raw) = key_to_note(key, self.base_octave) else { return };
        let note = self.scale_q.quantize(raw);
        let cursor = self.seq_cursor;
        let n = {
            let mut s = self.synth.lock().unwrap();
            s.sequencer.set_step(cursor, note);
            s.sequencer.num_steps
        };
        self.status_msg = format!("Step {}: {}", cursor + 1, note_name(note));
        self.seq_cursor = (cursor + 1) % n;
    }

    pub fn seq_clear_step(&mut self) {
        let cursor = self.seq_cursor;
        self.synth.lock().unwrap().sequencer.clear_step(cursor);
        self.status_msg = format!("Step {} cleared", cursor + 1);
    }

    pub fn seq_toggle_play(&mut self) {
        let mut s = self.synth.lock().unwrap();
        if let Some(note) = s.sequencer.toggle_play() { s.note_off(note); }
        self.status_msg = if s.sequencer.playing { "Seq: Playing".to_string() }
                          else                   { "Seq: Paused".to_string() };
    }

    pub fn seq_cycle_steps(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.sequencer.cycle_num_steps();
        let n = s.sequencer.num_steps;
        drop(s);
        if self.seq_cursor >= n { self.seq_cursor = 0; }
        self.status_msg = format!("Seq steps: {}", n);
    }

    // ── Melodic sequencer 2 controls ──────────────────────────────────────

    pub fn seq2_cursor_left(&mut self) {
        let n = self.synth.lock().unwrap().sequencer2.num_steps;
        self.seq2_cursor = if self.seq2_cursor == 0 { n - 1 } else { self.seq2_cursor - 1 };
    }

    pub fn seq2_cursor_right(&mut self) {
        let n = self.synth.lock().unwrap().sequencer2.num_steps;
        self.seq2_cursor = (self.seq2_cursor + 1) % n;
    }

    pub fn seq2_set_note(&mut self, key: char) {
        let Some(raw) = key_to_note(key, self.base_octave) else { return };
        let note = self.scale_q.quantize(raw);
        let cursor = self.seq2_cursor;
        let n = {
            let mut s = self.synth.lock().unwrap();
            s.sequencer2.set_step(cursor, note);
            s.sequencer2.num_steps
        };
        self.status_msg = format!("Seq2 step {}: {}", cursor + 1, note_name(note));
        self.seq2_cursor = (cursor + 1) % n;
    }

    pub fn seq2_clear_step(&mut self) {
        let cursor = self.seq2_cursor;
        self.synth.lock().unwrap().sequencer2.clear_step(cursor);
        self.status_msg = format!("Seq2 step {} cleared", cursor + 1);
    }

    pub fn seq2_toggle_play(&mut self) {
        let mut s = self.synth.lock().unwrap();
        if let Some(note) = s.sequencer2.toggle_play() { s.note_off2(note); }
        self.status_msg = if s.sequencer2.playing { "Seq2: Playing".to_string() }
                          else                    { "Seq2: Paused".to_string() };
    }

    pub fn seq2_cycle_steps(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.sequencer2.cycle_num_steps();
        let n = s.sequencer2.num_steps;
        drop(s);
        if self.seq2_cursor >= n { self.seq2_cursor = 0; }
        self.status_msg = format!("Seq2 steps: {}", n);
    }

    // ── Drum machine controls ─────────────────────────────────────────────

    pub fn drum_track_up(&mut self) {
        let n = self.synth.lock().unwrap().drum_machine.tracks.len();
        self.drum_track = if self.drum_track == 0 { n - 1 } else { self.drum_track - 1 };
    }

    pub fn drum_track_down(&mut self) {
        let n = self.synth.lock().unwrap().drum_machine.tracks.len();
        self.drum_track = (self.drum_track + 1) % n;
    }

    pub fn drum_step_left(&mut self) {
        let n = self.synth.lock().unwrap().drum_machine.num_steps;
        self.drum_step = if self.drum_step == 0 { n - 1 } else { self.drum_step - 1 };
    }

    pub fn drum_step_right(&mut self) {
        let n = self.synth.lock().unwrap().drum_machine.num_steps;
        self.drum_step = (self.drum_step + 1) % n;
    }

    pub fn drum_toggle_step(&mut self) {
        let (track, step) = (self.drum_track, self.drum_step);
        self.synth.lock().unwrap().drum_machine.toggle_step(track, step);
    }

    pub fn drum_clear_step(&mut self) {
        let (track, step) = (self.drum_track, self.drum_step);
        self.synth.lock().unwrap().drum_machine.clear_step(track, step);
    }

    pub fn drum_toggle_mute(&mut self) {
        let track = self.drum_track;
        self.synth.lock().unwrap().drum_machine.toggle_mute(track);
        let muted = self.synth.lock().unwrap().drum_machine.tracks[track].muted;
        let kind  = self.synth.lock().unwrap().drum_machine.tracks[track].kind;
        self.status_msg = if muted {
            format!("{} muted", kind.name())
        } else {
            format!("{} unmuted", kind.name())
        };
    }

    pub fn drum_toggle_play(&mut self) {
        self.synth.lock().unwrap().drum_machine.toggle_play();
        let playing = self.synth.lock().unwrap().drum_machine.playing;
        self.status_msg = if playing { "Drums: Playing".to_string() }
                          else       { "Drums: Stopped".to_string() };
    }

    pub fn drum_cycle_steps(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.cycle_num_steps();
        let n = s.drum_machine.num_steps;
        drop(s);
        if self.drum_step >= n { self.drum_step = 0; }
        self.status_msg = format!("Drum steps: {}", n);
    }

    pub fn drum_vol_up(&mut self) {
        let track = self.drum_track;
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.track_volume_up(track);
        let vol  = s.drum_machine.tracks[track].volume;
        let kind = s.drum_machine.tracks[track].kind;
        self.status_msg = format!("{} vol: {}%", kind.name(), (vol * 100.0).round() as u32);
    }

    pub fn drum_vol_down(&mut self) {
        let track = self.drum_track;
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.track_volume_down(track);
        let vol  = s.drum_machine.tracks[track].volume;
        let kind = s.drum_machine.tracks[track].kind;
        self.status_msg = format!("{} vol: {}%", kind.name(), (vol * 100.0).round() as u32);
    }

    pub fn drum_prob_up(&mut self) {
        let (track, step) = (self.drum_track, self.drum_step);
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.step_prob_up(track, step);
        let prob = s.drum_machine.tracks[track].steps[step];
        let kind = s.drum_machine.tracks[track].kind;
        self.status_msg = format!("{} step {}: {}%", kind.name(), step + 1, prob);
    }

    pub fn drum_prob_down(&mut self) {
        let (track, step) = (self.drum_track, self.drum_step);
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.step_prob_down(track, step);
        let prob = s.drum_machine.tracks[track].steps[step];
        let kind = s.drum_machine.tracks[track].kind;
        self.status_msg = if prob == 0 {
            format!("{} step {}: OFF", kind.name(), step + 1)
        } else {
            format!("{} step {}: {}%", kind.name(), step + 1, prob)
        };
    }

    pub fn drum_swing_up(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.swing = (s.drum_machine.swing + 0.05).min(0.50);
        self.status_msg = format!("Swing: {:.0}%", s.drum_machine.swing * 100.0);
    }

    pub fn drum_swing_down(&mut self) {
        let mut s = self.synth.lock().unwrap();
        s.drum_machine.swing = (s.drum_machine.swing - 0.05).max(0.0);
        self.status_msg = format!("Swing: {:.0}%", s.drum_machine.swing * 100.0);
    }

    pub fn drum_euclidean(&mut self) {
        let track = self.drum_track;
        let (k, kind, n) = {
            let s = self.synth.lock().unwrap();
            let dm = &s.drum_machine;
            let k = dm.tracks[track].steps.iter().filter(|&&p| p > 0).count();
            let k = if k == 0 { 4 } else { k };
            (k, dm.tracks[track].kind, dm.num_steps)
        };
        self.synth.lock().unwrap().drum_machine.euclidean_fill(track, k);
        self.status_msg = format!("{}: E({},{})", kind.name(), k, n);
    }

    /// Preview a drum track by key: z=Kick x=Snare c=C-Hat v=O-Hat b=Clap
    /// n=L.Tom m=M.Tom ,=H.Tom  — all fully polyphonic.
    pub fn drum_preview(&mut self, key: char) {
        let idx: usize = match key {
            'z' => 0, 'x' => 1, 'c' => 2, 'v' => 3,
            'b' => 4, 'n' => 5, 'm' => 6, ',' => 7,
            _ => return,
        };
        self.synth.lock().unwrap().drum_machine.trigger_now(idx);
    }

    // ── Effects controls ──────────────────────────────────────────────────

    pub fn effects_sel_up(&mut self) {
        self.effects_sel = if self.effects_sel == 0 { 5 } else { self.effects_sel - 1 };
    }

    pub fn effects_sel_down(&mut self) {
        self.effects_sel = (self.effects_sel + 1) % 6;
    }

    /// Left/right cycles through params 0–5 (0-2=effect params, 3-5=send levels).
    pub fn effects_param_left(&mut self) {
        self.effects_param = if self.effects_param == 0 { 5 } else { self.effects_param - 1 };
    }

    pub fn effects_param_right(&mut self) {
        self.effects_param = (self.effects_param + 1) % 6;
    }

    /// Enter in Effects: always toggle on/off for the selected effect.
    pub fn effects_on_off(&mut self) {
        let sel = self.effects_sel;
        let msg = {
            let mut s = self.synth.lock().unwrap();
            match sel {
                0 => { s.reverb.enabled = !s.reverb.enabled;
                       format!("Reverb: {}", if s.reverb.enabled { "ON" } else { "OFF" }) }
                1 => { s.delay.enabled = !s.delay.enabled;
                       format!("Delay: {}", if s.delay.enabled { "ON" } else { "OFF" }) }
                2 => { s.distortion.enabled = !s.distortion.enabled;
                       format!("Distortion: {}", if s.distortion.enabled { "ON" } else { "OFF" }) }
                3 => { s.sidechain.enabled = !s.sidechain.enabled;
                       format!("Sidechain: {}", if s.sidechain.enabled { "ON" } else { "OFF" }) }
                4 => { s.filter1.enabled = !s.filter1.enabled;
                       if s.filter1.enabled { s.filter1.reset_state(); }
                       format!("S1 Filter: {}", if s.filter1.enabled { "ON" } else { "OFF" }) }
                5 => { s.filter2.enabled = !s.filter2.enabled;
                       if s.filter2.enabled { s.filter2.reset_state(); }
                       format!("S2 Filter: {}", if s.filter2.enabled { "ON" } else { "OFF" }) }
                _ => String::new()
            }
        };
        self.status_msg = msg;
    }

    /// Space in Effects: quick-toggle send level 0↔1 only for routing columns (params 3-5).
    pub fn effects_route_toggle(&mut self) {
        let sel = self.effects_sel;
        let par = self.effects_param;

        if par < 3 || sel >= 4 { return; }

        let ri = par - 3;
        let msg = {
            let mut s = self.synth.lock().unwrap();
            let (val, name) = match (sel, ri) {
                (0, 0) => { s.fx_routing.s1_reverb = if s.fx_routing.s1_reverb > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s1_reverb, "S1→Rev") }
                (0, 1) => { s.fx_routing.s2_reverb = if s.fx_routing.s2_reverb > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s2_reverb, "S2→Rev") }
                (0, 2) => { s.fx_routing.dr_reverb = if s.fx_routing.dr_reverb > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.dr_reverb, "DR→Rev") }
                (1, 0) => { s.fx_routing.s1_delay  = if s.fx_routing.s1_delay  > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s1_delay,  "S1→Dly") }
                (1, 1) => { s.fx_routing.s2_delay  = if s.fx_routing.s2_delay  > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s2_delay,  "S2→Dly") }
                (1, 2) => { s.fx_routing.dr_delay  = if s.fx_routing.dr_delay  > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.dr_delay,  "DR→Dly") }
                (2, 0) => { s.fx_routing.s1_dist   = if s.fx_routing.s1_dist   > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s1_dist,   "S1→Dst") }
                (2, 1) => { s.fx_routing.s2_dist   = if s.fx_routing.s2_dist   > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.s2_dist,   "S2→Dst") }
                (2, 2) => { s.fx_routing.dr_dist   = if s.fx_routing.dr_dist   > 0.5 { 0.0 } else { 1.0 }; (s.fx_routing.dr_dist,   "DR→Dst") }
                (3, 0) => { s.sidechain.duck_s1 = !s.sidechain.duck_s1; (s.sidechain.duck_s1 as u8 as f32, "SC→S1") }
                (3, 1) => { s.sidechain.duck_s2 = !s.sidechain.duck_s2; (s.sidechain.duck_s2 as u8 as f32, "SC→S2") }
                _ => (0.0, ""),
            };
            format!("{}: {:.0}%", name, val * 100.0)
        };
        self.status_msg = msg;
    }

    pub fn effects_param_inc(&mut self) {
        let (sel, param) = (self.effects_sel, self.effects_param);

        if param >= 3 {
            if sel >= 4 { return; } // Filter rows have no routing sends
            let ri = param - 3;
            let msg = {
                let mut s = self.synth.lock().unwrap();
                let (val, name) = match (sel, ri) {
                    (0, 0) => { s.fx_routing.s1_reverb = (s.fx_routing.s1_reverb + 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_reverb, "S1→Rev") }
                    (0, 1) => { s.fx_routing.s2_reverb = (s.fx_routing.s2_reverb + 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_reverb, "S2→Rev") }
                    (0, 2) => { s.fx_routing.dr_reverb = (s.fx_routing.dr_reverb + 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_reverb, "DR→Rev") }
                    (1, 0) => { s.fx_routing.s1_delay  = (s.fx_routing.s1_delay  + 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_delay,  "S1→Dly") }
                    (1, 1) => { s.fx_routing.s2_delay  = (s.fx_routing.s2_delay  + 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_delay,  "S2→Dly") }
                    (1, 2) => { s.fx_routing.dr_delay  = (s.fx_routing.dr_delay  + 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_delay,  "DR→Dly") }
                    (2, 0) => { s.fx_routing.s1_dist   = (s.fx_routing.s1_dist   + 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_dist,   "S1→Dst") }
                    (2, 1) => { s.fx_routing.s2_dist   = (s.fx_routing.s2_dist   + 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_dist,   "S2→Dst") }
                    (2, 2) => { s.fx_routing.dr_dist   = (s.fx_routing.dr_dist   + 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_dist,   "DR→Dst") }
                    _ => (0.0, ""),
                };
                format!("{}: {:.0}%", name, val * 100.0)
            };
            self.status_msg = msg;
        } else {
            let msg = {
                let mut s = self.synth.lock().unwrap();
                match sel {
                    0 => match param {
                        0 => { s.reverb.room_size = (s.reverb.room_size + 0.05).clamp(0.0, 1.0);
                               format!("Reverb Room: {:.0}%", s.reverb.room_size * 100.0) }
                        1 => { s.reverb.damping = (s.reverb.damping + 0.05).clamp(0.0, 1.0);
                               format!("Reverb Damp: {:.0}%", s.reverb.damping * 100.0) }
                        _ => { s.reverb.mix = (s.reverb.mix + 0.05).clamp(0.0, 1.0);
                               format!("Reverb Mix: {:.0}%", s.reverb.mix * 100.0) }
                    },
                    1 => match param {
                        0 => { s.delay.time_ms = (s.delay.time_ms + 25.0).clamp(10.0, 1000.0);
                               format!("Delay Time: {:.0}ms", s.delay.time_ms) }
                        1 => { s.delay.feedback = (s.delay.feedback + 0.05).clamp(0.0, 0.95);
                               format!("Delay Feed: {:.0}%", s.delay.feedback * 100.0) }
                        _ => { s.delay.mix = (s.delay.mix + 0.05).clamp(0.0, 1.0);
                               format!("Delay Mix: {:.0}%", s.delay.mix * 100.0) }
                    },
                    2 => match param {
                        0 => { s.distortion.drive = (s.distortion.drive + 0.5).clamp(1.0, 10.0);
                               format!("Dist Drive: {:.1}x", s.distortion.drive) }
                        1 => { s.distortion.tone = (s.distortion.tone + 0.05).clamp(0.0, 1.0);
                               format!("Dist Tone: {:.0}%", s.distortion.tone * 100.0) }
                        _ => { s.distortion.level = (s.distortion.level + 0.05).clamp(0.0, 1.0);
                               format!("Dist Level: {:.0}%", s.distortion.level * 100.0) }
                    },
                    3 => match param {
                        0 => { s.sidechain.depth = (s.sidechain.depth + 0.05).clamp(0.0, 1.0);
                               format!("SC Depth: {:.0}%", s.sidechain.depth * 100.0) }
                        1 => { s.sidechain.release_ms = (s.sidechain.release_ms + 25.0).clamp(10.0, 500.0);
                               format!("SC Release: {:.0}ms", s.sidechain.release_ms) }
                        _ => String::new()
                    },
                    4 => match param {
                        0 => { s.filter1.mode = s.filter1.mode.next();
                               format!("S1 Filter: {}", s.filter1.mode.name()) }
                        1 => { s.filter1.cutoff = (s.filter1.cutoff * 1.0595).clamp(80.0, 18000.0);
                               format!("S1 Cutoff: {:.0}Hz", s.filter1.cutoff) }
                        _ => { s.filter1.q = (s.filter1.q + 0.1).clamp(0.5, 10.0);
                               format!("S1 Q: {:.1}", s.filter1.q) }
                    },
                    5 => match param {
                        0 => { s.filter2.mode = s.filter2.mode.next();
                               format!("S2 Filter: {}", s.filter2.mode.name()) }
                        1 => { s.filter2.cutoff = (s.filter2.cutoff * 1.0595).clamp(80.0, 18000.0);
                               format!("S2 Cutoff: {:.0}Hz", s.filter2.cutoff) }
                        _ => { s.filter2.q = (s.filter2.q + 0.1).clamp(0.5, 10.0);
                               format!("S2 Q: {:.1}", s.filter2.q) }
                    },
                    _ => String::new(),
                }
            };
            self.status_msg = msg;
        }
    }

    pub fn effects_param_dec(&mut self) {
        let (sel, param) = (self.effects_sel, self.effects_param);

        if param >= 3 {
            if sel >= 4 { return; } // Filter rows have no routing sends
            let ri = param - 3;
            let msg = {
                let mut s = self.synth.lock().unwrap();
                let (val, name) = match (sel, ri) {
                    (0, 0) => { s.fx_routing.s1_reverb = (s.fx_routing.s1_reverb - 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_reverb, "S1→Rev") }
                    (0, 1) => { s.fx_routing.s2_reverb = (s.fx_routing.s2_reverb - 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_reverb, "S2→Rev") }
                    (0, 2) => { s.fx_routing.dr_reverb = (s.fx_routing.dr_reverb - 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_reverb, "DR→Rev") }
                    (1, 0) => { s.fx_routing.s1_delay  = (s.fx_routing.s1_delay  - 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_delay,  "S1→Dly") }
                    (1, 1) => { s.fx_routing.s2_delay  = (s.fx_routing.s2_delay  - 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_delay,  "S2→Dly") }
                    (1, 2) => { s.fx_routing.dr_delay  = (s.fx_routing.dr_delay  - 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_delay,  "DR→Dly") }
                    (2, 0) => { s.fx_routing.s1_dist   = (s.fx_routing.s1_dist   - 0.05).clamp(0.0, 1.0); (s.fx_routing.s1_dist,   "S1→Dst") }
                    (2, 1) => { s.fx_routing.s2_dist   = (s.fx_routing.s2_dist   - 0.05).clamp(0.0, 1.0); (s.fx_routing.s2_dist,   "S2→Dst") }
                    (2, 2) => { s.fx_routing.dr_dist   = (s.fx_routing.dr_dist   - 0.05).clamp(0.0, 1.0); (s.fx_routing.dr_dist,   "DR→Dst") }
                    _ => (0.0, ""),
                };
                format!("{}: {:.0}%", name, val * 100.0)
            };
            self.status_msg = msg;
        } else {
            let msg = {
                let mut s = self.synth.lock().unwrap();
                match sel {
                    0 => match param {
                        0 => { s.reverb.room_size = (s.reverb.room_size - 0.05).clamp(0.0, 1.0);
                               format!("Reverb Room: {:.0}%", s.reverb.room_size * 100.0) }
                        1 => { s.reverb.damping = (s.reverb.damping - 0.05).clamp(0.0, 1.0);
                               format!("Reverb Damp: {:.0}%", s.reverb.damping * 100.0) }
                        _ => { s.reverb.mix = (s.reverb.mix - 0.05).clamp(0.0, 1.0);
                               format!("Reverb Mix: {:.0}%", s.reverb.mix * 100.0) }
                    },
                    1 => match param {
                        0 => { s.delay.time_ms = (s.delay.time_ms - 25.0).clamp(10.0, 1000.0);
                               format!("Delay Time: {:.0}ms", s.delay.time_ms) }
                        1 => { s.delay.feedback = (s.delay.feedback - 0.05).clamp(0.0, 0.95);
                               format!("Delay Feed: {:.0}%", s.delay.feedback * 100.0) }
                        _ => { s.delay.mix = (s.delay.mix - 0.05).clamp(0.0, 1.0);
                               format!("Delay Mix: {:.0}%", s.delay.mix * 100.0) }
                    },
                    2 => match param {
                        0 => { s.distortion.drive = (s.distortion.drive - 0.5).clamp(1.0, 10.0);
                               format!("Dist Drive: {:.1}x", s.distortion.drive) }
                        1 => { s.distortion.tone = (s.distortion.tone - 0.05).clamp(0.0, 1.0);
                               format!("Dist Tone: {:.0}%", s.distortion.tone * 100.0) }
                        _ => { s.distortion.level = (s.distortion.level - 0.05).clamp(0.0, 1.0);
                               format!("Dist Level: {:.0}%", s.distortion.level * 100.0) }
                    },
                    3 => match param {
                        0 => { s.sidechain.depth = (s.sidechain.depth - 0.05).clamp(0.0, 1.0);
                               format!("SC Depth: {:.0}%", s.sidechain.depth * 100.0) }
                        1 => { s.sidechain.release_ms = (s.sidechain.release_ms - 25.0).clamp(10.0, 500.0);
                               format!("SC Release: {:.0}ms", s.sidechain.release_ms) }
                        _ => String::new()
                    },
                    4 => match param {
                        0 => { s.filter1.mode = s.filter1.mode.prev();
                               format!("S1 Filter: {}", s.filter1.mode.name()) }
                        1 => { s.filter1.cutoff = (s.filter1.cutoff / 1.0595).clamp(80.0, 18000.0);
                               format!("S1 Cutoff: {:.0}Hz", s.filter1.cutoff) }
                        _ => { s.filter1.q = (s.filter1.q - 0.1).clamp(0.5, 10.0);
                               format!("S1 Q: {:.1}", s.filter1.q) }
                    },
                    5 => match param {
                        0 => { s.filter2.mode = s.filter2.mode.prev();
                               format!("S2 Filter: {}", s.filter2.mode.name()) }
                        1 => { s.filter2.cutoff = (s.filter2.cutoff / 1.0595).clamp(80.0, 18000.0);
                               format!("S2 Cutoff: {:.0}Hz", s.filter2.cutoff) }
                        _ => { s.filter2.q = (s.filter2.q - 0.1).clamp(0.5, 10.0);
                               format!("S2 Q: {:.1}", s.filter2.q) }
                    },
                    _ => String::new(),
                }
            };
            self.status_msg = msg;
        }
    }

    /// Returns FX active indicators for the title bar (one lock acquisition).
    pub fn fx_indicators(&self) -> String {
        let s = self.synth.lock().unwrap();
        let mut ind = String::new();
        if s.reverb.enabled     { ind.push_str("  ▶RVB"); }
        if s.delay.enabled      { ind.push_str("  ▶DLY"); }
        if s.distortion.enabled { ind.push_str("  ▶DST"); }
        if s.sidechain.enabled  { ind.push_str("  ▶SC"); }
        if s.filter1.enabled    { ind.push_str("  ▶F1"); }
        if s.filter2.enabled    { ind.push_str("  ▶F2"); }
        ind
    }

    // ── Persistence ───────────────────────────────────────────────────────

    pub fn save(&mut self, path: &str) {
        fn wave_idx(w: WaveType) -> u8 {
            match w { WaveType::Sine=>0, WaveType::Square=>1,
                      WaveType::Sawtooth=>2, WaveType::Triangle=>3 }
        }
        fn filter_mode_idx(m: FilterMode) -> u8 {
            match m { FilterMode::LowPass=>0, FilterMode::HighPass=>1, FilterMode::BandPass=>2 }
        }

        // Copy App-level fields before taking the synth lock.
        let base_octave = self.base_octave;
        let scale_idx = Scale::ALL.iter()
            .position(|&sc| sc == self.scale_q.scale)
            .unwrap_or(0) as u8;
        let scale_root = self.scale_q.root;

        let sf = {
            let s = self.synth.lock().unwrap();

            let seq1 = SeqSave {
                num_steps: s.sequencer.num_steps,
                steps: s.sequencer.steps.clone(),
            };
            let seq2 = SeqSave {
                num_steps: s.sequencer2.num_steps,
                steps: s.sequencer2.steps.clone(),
            };

            let drums = DrumsSave {
                num_steps: s.drum_machine.num_steps,
                swing:     s.drum_machine.swing,
                tracks: s.drum_machine.tracks.iter().map(|t| TrackSave {
                    kind:   DrumKind::ALL.iter().position(|&k| k == t.kind).unwrap_or(0) as u8,
                    steps:  t.steps.clone(),
                    muted:  t.muted,
                    volume: t.volume,
                }).collect(),
            };

            let reverb = ReverbSave {
                enabled:   s.reverb.enabled,
                room_size: s.reverb.room_size,
                damping:   s.reverb.damping,
                mix:       s.reverb.mix,
            };
            let delay = DelaySave {
                enabled:  s.delay.enabled,
                time_ms:  s.delay.time_ms,
                feedback: s.delay.feedback,
                mix:      s.delay.mix,
            };
            let distortion = DistSave {
                enabled: s.distortion.enabled,
                drive:   s.distortion.drive,
                tone:    s.distortion.tone,
                level:   s.distortion.level,
            };
            let sidechain = SidechainSave {
                enabled:    s.sidechain.enabled,
                depth:      s.sidechain.depth,
                release_ms: s.sidechain.release_ms,
                duck_s1:    s.sidechain.duck_s1,
                duck_s2:    s.sidechain.duck_s2,
            };
            let filter1 = FilterSave {
                enabled: s.filter1.enabled,
                mode:    filter_mode_idx(s.filter1.mode),
                cutoff:  s.filter1.cutoff,
                q:       s.filter1.q,
            };
            let filter2 = FilterSave {
                enabled: s.filter2.enabled,
                mode:    filter_mode_idx(s.filter2.mode),
                cutoff:  s.filter2.cutoff,
                q:       s.filter2.q,
            };
            let routing = RoutingSave {
                s1_reverb: s.fx_routing.s1_reverb, s1_delay: s.fx_routing.s1_delay, s1_dist: s.fx_routing.s1_dist,
                s2_reverb: s.fx_routing.s2_reverb, s2_delay: s.fx_routing.s2_delay, s2_dist: s.fx_routing.s2_dist,
                dr_reverb: s.fx_routing.dr_reverb, dr_delay: s.fx_routing.dr_delay, dr_dist: s.fx_routing.dr_dist,
            };

            SaveFile {
                bpm:        s.bpm,
                base_octave,
                scale:      scale_idx,
                scale_root,
                wave1:      wave_idx(s.wave_type),
                wave2:      wave_idx(s.wave_type2),
                volume:     s.volume,
                volume2:    s.volume2,
                seq1, seq2, drums,
                reverb, delay, distortion, sidechain,
                filter1, filter2, routing,
            }
        };

        match serde_json::to_string_pretty(&sf) {
            Ok(json) => match std::fs::write(path, &json) {
                Ok(_)  => self.status_msg = format!("Saved → {}", path),
                Err(e) => self.status_msg = format!("Save error: {}", e),
            },
            Err(e) => self.status_msg = format!("Serialize error: {}", e),
        }
    }

    pub fn load(&mut self, path: &str) {
        let json = match std::fs::read_to_string(path) {
            Ok(j)  => j,
            Err(e) => { self.status_msg = format!("Load error: {}", e); return; }
        };
        let sf: SaveFile = match serde_json::from_str(&json) {
            Ok(s)  => s,
            Err(e) => { self.status_msg = format!("Load error: {}", e); return; }
        };

        self.release_all();

        {
            let mut s = self.synth.lock().unwrap();

            s.bpm = sf.bpm.clamp(30.0, 300.0);

            s.wave_type = match sf.wave1 {
                1 => WaveType::Square, 2 => WaveType::Sawtooth,
                3 => WaveType::Triangle, _ => WaveType::Sine,
            };
            s.wave_type2 = match sf.wave2 {
                1 => WaveType::Square, 2 => WaveType::Sawtooth,
                3 => WaveType::Triangle, _ => WaveType::Sine,
            };

            s.volume  = sf.volume.clamp(0.0, 1.0);
            s.volume2 = sf.volume2.clamp(0.0, 1.0);

            // Sequencer 1
            let n1 = sf.seq1.num_steps.clamp(1, 32);
            s.sequencer.num_steps = n1;
            s.sequencer.steps = sf.seq1.steps;
            s.sequencer.steps.resize(n1, None);

            // Sequencer 2
            let n2 = sf.seq2.num_steps.clamp(1, 32);
            s.sequencer2.num_steps = n2;
            s.sequencer2.steps = sf.seq2.steps;
            s.sequencer2.steps.resize(n2, None);

            // Drums
            let nd = sf.drums.num_steps.clamp(1, 32);
            s.drum_machine.num_steps = nd;
            s.drum_machine.swing = sf.drums.swing.clamp(0.0, 0.5);
            let n_tracks = s.drum_machine.tracks.len().min(sf.drums.tracks.len());
            for i in 0..n_tracks {
                let t = &sf.drums.tracks[i];
                s.drum_machine.tracks[i].steps = t.steps.clone();
                s.drum_machine.tracks[i].steps.resize(nd, 0);
                s.drum_machine.tracks[i].muted  = t.muted;
                s.drum_machine.tracks[i].volume = t.volume.clamp(0.0, 1.0);
            }

            // Reverb
            s.reverb.enabled   = sf.reverb.enabled;
            s.reverb.room_size = sf.reverb.room_size.clamp(0.0, 1.0);
            s.reverb.damping   = sf.reverb.damping.clamp(0.0, 1.0);
            s.reverb.mix       = sf.reverb.mix.clamp(0.0, 1.0);

            // Delay
            s.delay.enabled  = sf.delay.enabled;
            s.delay.time_ms  = sf.delay.time_ms.clamp(10.0, 1000.0);
            s.delay.feedback = sf.delay.feedback.clamp(0.0, 0.95);
            s.delay.mix      = sf.delay.mix.clamp(0.0, 1.0);

            // Distortion
            s.distortion.enabled = sf.distortion.enabled;
            s.distortion.drive   = sf.distortion.drive.clamp(1.0, 10.0);
            s.distortion.tone    = sf.distortion.tone.clamp(0.0, 1.0);
            s.distortion.level   = sf.distortion.level.clamp(0.0, 1.0);

            // Sidechain
            s.sidechain.enabled    = sf.sidechain.enabled;
            s.sidechain.depth      = sf.sidechain.depth.clamp(0.0, 1.0);
            s.sidechain.release_ms = sf.sidechain.release_ms.clamp(10.0, 500.0);
            s.sidechain.duck_s1    = sf.sidechain.duck_s1;
            s.sidechain.duck_s2    = sf.sidechain.duck_s2;

            // Filter 1
            s.filter1.enabled = sf.filter1.enabled;
            s.filter1.mode    = match sf.filter1.mode {
                1 => FilterMode::HighPass, 2 => FilterMode::BandPass, _ => FilterMode::LowPass
            };
            s.filter1.cutoff = sf.filter1.cutoff.clamp(80.0, 18000.0);
            s.filter1.q      = sf.filter1.q.clamp(0.5, 10.0);
            if s.filter1.enabled { s.filter1.reset_state(); }

            // Filter 2
            s.filter2.enabled = sf.filter2.enabled;
            s.filter2.mode    = match sf.filter2.mode {
                1 => FilterMode::HighPass, 2 => FilterMode::BandPass, _ => FilterMode::LowPass
            };
            s.filter2.cutoff = sf.filter2.cutoff.clamp(80.0, 18000.0);
            s.filter2.q      = sf.filter2.q.clamp(0.5, 10.0);
            if s.filter2.enabled { s.filter2.reset_state(); }

            // Routing
            s.fx_routing.s1_reverb = sf.routing.s1_reverb.clamp(0.0, 1.0);
            s.fx_routing.s1_delay  = sf.routing.s1_delay.clamp(0.0, 1.0);
            s.fx_routing.s1_dist   = sf.routing.s1_dist.clamp(0.0, 1.0);
            s.fx_routing.s2_reverb = sf.routing.s2_reverb.clamp(0.0, 1.0);
            s.fx_routing.s2_delay  = sf.routing.s2_delay.clamp(0.0, 1.0);
            s.fx_routing.s2_dist   = sf.routing.s2_dist.clamp(0.0, 1.0);
            s.fx_routing.dr_reverb = sf.routing.dr_reverb.clamp(0.0, 1.0);
            s.fx_routing.dr_delay  = sf.routing.dr_delay.clamp(0.0, 1.0);
            s.fx_routing.dr_dist   = sf.routing.dr_dist.clamp(0.0, 1.0);
        }

        // App-level fields
        self.base_octave   = sf.base_octave.clamp(0, 8);
        self.scale_q.scale = Scale::ALL.get(sf.scale as usize).copied().unwrap_or(Scale::Off);
        self.scale_q.root  = sf.scale_root % 12;

        // Reset cursors
        self.seq_cursor  = 0;
        self.seq2_cursor = 0;
        self.drum_step   = 0;

        self.status_msg = format!("Loaded ← {}", path);
    }

    /// Commit the current file-path input: call save or load, then reset input state.
    pub fn commit_input(&mut self) {
        let path = self.input_buf.trim().to_string();
        let mode = self.input_mode.clone();
        self.input_mode = InputMode::None;
        self.input_buf.clear();
        if path.is_empty() { return; }
        match mode {
            InputMode::Save => self.save(&path),
            InputMode::Load => self.load(&path),
            InputMode::None => {}
        }
    }
}
