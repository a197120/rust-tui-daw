/// An event fired when the sequencer crosses a step boundary.
pub struct StepEvent {
    pub note_off: Option<u8>,
    pub note_on:  Option<u8>,
}

/// Sample-accurate melodic step sequencer.
///
/// BPM is **not** stored here — it is passed to `tick()` every sample from
/// `Synth::bpm` so the melodic and drum sequencers always share one master clock.
pub struct Sequencer {
    pub steps:        Vec<Option<u8>>,
    pub num_steps:    usize,
    pub current_step: usize,
    pub playing:      bool,

    sample_rate: f32,
}

impl Sequencer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            steps:        vec![None; 16],
            num_steps:    16,
            current_step: 0,
            playing:      false,
            sample_rate,
        }
    }

    fn samples_per_step(&self, bpm: f32) -> u64 {
        ((self.sample_rate * 60.0) / (bpm * 4.0)).round() as u64
    }

    /// Called once per audio sample with the shared master clock.
    /// Returns `Some(StepEvent)` on step boundaries.
    pub fn tick(&mut self, bpm: f32, clock: u64) -> Option<StepEvent> {
        if !self.playing { return None; }

        let sps = self.samples_per_step(bpm).max(1);
        let step_idx = (clock / sps) as usize % self.num_steps;
        let phase_in = clock % sps;

        self.current_step = step_idx;

        if phase_in == 0 {
            let prev = if step_idx == 0 { self.num_steps - 1 } else { step_idx - 1 };
            Some(StepEvent {
                note_off: self.steps[prev],
                note_on:  self.steps[step_idx],
            })
        } else {
            None
        }
    }

    /// Toggle play/pause.  Returns the note currently held (for note-off).
    pub fn toggle_play(&mut self) -> Option<u8> {
        self.playing = !self.playing;
        if self.playing {
            None
        } else {
            self.steps.get(self.current_step).copied().flatten()
        }
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) -> Option<u8> {
        let note = if self.playing { self.steps.get(self.current_step).copied().flatten() } else { None };
        self.playing      = false;
        self.current_step = 0;
        note
    }

    pub fn cycle_num_steps(&mut self) {
        let next = match self.num_steps { 8 => 16, 16 => 24, 24 => 32, _ => 8 };
        self.num_steps = next;
        self.steps.resize(next, None);
        if self.current_step >= next { self.current_step = 0; }
    }

    pub fn set_step(&mut self, step: usize, note: u8) {
        if step < self.steps.len() { self.steps[step] = Some(note); }
    }

    pub fn clear_step(&mut self, step: usize) {
        if step < self.steps.len() { self.steps[step] = None; }
    }
}
