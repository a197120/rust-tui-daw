use std::f32::consts::PI;

/// Mono audio effect: one sample in, one sample out.
#[allow(dead_code)]
///
/// All implementations must be `Send` so they can live inside the audio thread
/// (behind `Arc<Mutex<Synth>>`).  Stereo can be modelled as two independent
/// mono effects or as a future specialisation — that's a later concern.
pub trait AudioEffect: Send {
    fn process(&mut self, sample: f32) -> f32;
    fn name(&self) -> &'static str;
    /// Reset all internal state (clear delay lines, reset envelopes, etc.).
    fn reset(&mut self);
}

/// A serial chain of effects applied to a mono signal.
///
/// When the chain is empty the audio passes through completely unchanged,
/// so there is zero CPU overhead until effects are actually inserted.
pub struct EffectChain {
    pub effects: Vec<Box<dyn AudioEffect>>,
}

#[allow(dead_code)]
impl EffectChain {
    pub fn new() -> Self {
        Self { effects: Vec::new() }
    }

    #[inline]
    pub fn process(&mut self, sample: f32) -> f32 {
        if self.effects.is_empty() {
            return sample;
        }
        self.effects.iter_mut().fold(sample, |s, fx| fx.process(s))
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn reset_all(&mut self) {
        for fx in &mut self.effects {
            fx.reset();
        }
    }
}

impl Default for EffectChain {
    fn default() -> Self {
        Self::new()
    }
}

// ── Freeverb helpers (private) ────────────────────────────────────────────────

struct CombFilter {
    buf: Vec<f32>,
    pos: usize,
    feedback: f32,
    damp_store: f32,
    damp1: f32,
    damp2: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self { buf: vec![0.0; size], pos: 0, feedback: 0.84,
               damp_store: 0.0, damp1: 0.2, damp2: 0.8 }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buf[self.pos];
        self.damp_store = output * self.damp2 + self.damp_store * self.damp1;
        self.buf[self.pos] = input + self.damp_store * self.feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        output
    }

    fn set_feedback(&mut self, v: f32) { self.feedback = v; }
    fn set_damp(&mut self, v: f32) { self.damp1 = v; self.damp2 = 1.0 - v; }
}

struct AllpassFilter {
    buf: Vec<f32>,
    pos: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self { buf: vec![0.0; size], pos: 0 }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.pos];
        let output = -input + bufout;
        self.buf[self.pos] = input + bufout * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        output
    }
}

// ── Reverb (Freeverb: 8 comb + 4 allpass, tuned for 44100 Hz) ────────────────

pub struct Reverb {
    pub enabled:   bool,
    pub room_size: f32,  // 0.0–1.0  (comb feedback = room_size*0.28+0.7)
    pub damping:   f32,  // 0.0–1.0  (comb damp = damping*0.4)
    pub mix:       f32,  // 0.0–1.0  wet/dry
    combs:    [CombFilter; 8],
    allpasses: [AllpassFilter; 4],
}

impl Reverb {
    pub fn new() -> Self {
        let mut r = Self {
            enabled: false, room_size: 0.5, damping: 0.5, mix: 0.3,
            combs: [
                CombFilter::new(1116), CombFilter::new(1188),
                CombFilter::new(1277), CombFilter::new(1356),
                CombFilter::new(1422), CombFilter::new(1491),
                CombFilter::new(1557), CombFilter::new(1617),
            ],
            allpasses: [
                AllpassFilter::new(556), AllpassFilter::new(441),
                AllpassFilter::new(341), AllpassFilter::new(225),
            ],
        };
        let fb = r.room_size * 0.28 + 0.7;
        let dp = r.damping * 0.4;
        for c in &mut r.combs { c.set_feedback(fb); c.set_damp(dp); }
        r
    }
}

impl AudioEffect for Reverb {
    fn process(&mut self, sample: f32) -> f32 {
        if !self.enabled { return 0.0; }
        let fb = self.room_size * 0.28 + 0.7;
        let dp = self.damping * 0.4;
        for c in &mut self.combs { c.set_feedback(fb); c.set_damp(dp); }
        let input = sample * 0.015;
        let mut wet = 0.0f32;
        for c in &mut self.combs { wet += c.process(input); }
        for ap in &mut self.allpasses { wet = ap.process(wet); }
        wet * self.mix * 3.0
    }

    fn name(&self) -> &'static str { "Reverb" }

    fn reset(&mut self) {
        for c in &mut self.combs { c.buf.fill(0.0); c.pos = 0; c.damp_store = 0.0; }
        for ap in &mut self.allpasses { ap.buf.fill(0.0); ap.pos = 0; }
    }
}

// ── Delay (ring-buffer echo) ──────────────────────────────────────────────────

pub struct Delay {
    pub enabled:  bool,
    pub time_ms:  f32,   // 10–1000 ms
    pub feedback: f32,   // 0.0–0.95
    pub mix:      f32,   // 0.0–1.0
    buf:         Vec<f32>,
    write:       usize,
    sample_rate: f32,
}

impl Delay {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false, time_ms: 250.0, feedback: 0.4, mix: 0.3,
            buf: vec![0.0; sample_rate as usize],
            write: 0, sample_rate,
        }
    }
}

impl AudioEffect for Delay {
    fn process(&mut self, sample: f32) -> f32 {
        if !self.enabled { return 0.0; }
        let delay_samp = ((self.time_ms / 1000.0 * self.sample_rate) as usize)
            .clamp(1, self.buf.len() - 1);
        let read = (self.write + self.buf.len() - delay_samp) % self.buf.len();
        let delayed = self.buf[read];
        self.buf[self.write] = sample + delayed * self.feedback;
        self.write = (self.write + 1) % self.buf.len();
        delayed * self.mix
    }

    fn name(&self) -> &'static str { "Delay" }

    fn reset(&mut self) { self.buf.fill(0.0); self.write = 0; }
}

// ── Distortion (waveshaper) ───────────────────────────────────────────────────

pub struct Distortion {
    pub enabled: bool,
    pub drive:   f32,   // 1.0–10.0  gain before clipping
    pub tone:    f32,   // 0.0–1.0   blend: 0=soft tanh, 1=hard clip
    pub level:   f32,   // 0.0–1.0   output level
}

impl Distortion {
    pub fn new() -> Self {
        Self { enabled: false, drive: 3.0, tone: 0.3, level: 0.7 }
    }
}

impl AudioEffect for Distortion {
    fn process(&mut self, sample: f32) -> f32 {
        if !self.enabled { return 0.0; }
        let driven = sample * self.drive;
        let soft   = driven.tanh();
        let hard   = driven.clamp(-1.0, 1.0);
        (soft * (1.0 - self.tone) + hard * self.tone) * self.level
    }

    fn name(&self) -> &'static str { "Distortion" }

    fn reset(&mut self) {}
}

// ── Biquad filter (RBJ Audio EQ Cookbook) ────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterMode { LowPass, HighPass, BandPass }

impl FilterMode {
    pub fn name(self) -> &'static str {
        match self { Self::LowPass => "LP", Self::HighPass => "HP", Self::BandPass => "BP" }
    }
    pub fn next(self) -> Self {
        match self { Self::LowPass => Self::HighPass, Self::HighPass => Self::BandPass, Self::BandPass => Self::LowPass }
    }
    pub fn prev(self) -> Self {
        match self { Self::LowPass => Self::BandPass, Self::HighPass => Self::LowPass, Self::BandPass => Self::HighPass }
    }
}

/// Two-pole biquad filter applied directly to a synth bus (not via EffectChain).
/// When disabled, passes signal through unchanged at zero cost.
pub struct BiquadFilter {
    pub enabled: bool,
    pub mode:    FilterMode,
    pub cutoff:  f32,   // Hz, 80.0–18 000.0
    pub q:       f32,   // 0.5–10.0
    sample_rate: f32,
    // Cached normalised coefficients
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    // Direct Form I delay state
    x1: f32, x2: f32, y1: f32, y2: f32,
    // Track last computed params to detect when a recompute is needed
    last_cutoff: f32, last_q: f32, last_mode: FilterMode,
}

impl BiquadFilter {
    pub fn new(sample_rate: f32) -> Self {
        let mut f = Self {
            enabled: false,
            mode: FilterMode::LowPass,
            cutoff: 5000.0,
            q: 0.707,
            sample_rate,
            b0: 0.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
            last_cutoff: -1.0, last_q: -1.0, last_mode: FilterMode::LowPass,
        };
        f.recompute();
        f
    }

    /// Reset delay state (call when toggling on to avoid a transient pop).
    pub fn reset_state(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0; self.y1 = 0.0; self.y2 = 0.0;
    }

    fn recompute(&mut self) {
        let w0    = 2.0 * PI * self.cutoff.min(self.sample_rate * 0.499) / self.sample_rate;
        let cos_w = w0.cos();
        let sin_w = w0.sin();
        let alpha = sin_w / (2.0 * self.q);

        let (b0, b1, b2) = match self.mode {
            FilterMode::LowPass  => { let h = (1.0 - cos_w) / 2.0; (h, 1.0 - cos_w, h) }
            FilterMode::HighPass => { let h = (1.0 + cos_w) / 2.0; (h, -(1.0 + cos_w), h) }
            FilterMode::BandPass => { let h = sin_w / 2.0; (h, 0.0, -h) }
        };
        let a0 = 1.0 + alpha;
        self.b0 = b0 / a0;  self.b1 = b1 / a0;  self.b2 = b2 / a0;
        self.a1 = -2.0 * cos_w / a0;
        self.a2 = (1.0 - alpha) / a0;

        self.last_cutoff = self.cutoff;
        self.last_q      = self.q;
        self.last_mode   = self.mode;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.enabled { return x; }
        if self.cutoff != self.last_cutoff || self.q != self.last_q || self.mode != self.last_mode {
            self.recompute();
        }
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
                             - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;  self.x1 = x;
        self.y2 = self.y1;  self.y1 = y;
        y
    }
}
