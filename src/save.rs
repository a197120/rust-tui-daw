use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    // Global
    pub bpm: f32,
    pub base_octave: i32,
    pub scale: u8,        // index into Scale::ALL
    pub scale_root: u8,
    // Synths
    pub wave1: u8,        // 0=Sine 1=Square 2=Saw 3=Tri
    pub wave2: u8,
    pub volume: f32,
    pub volume2: f32,
    // Sequencers
    pub seq1: SeqSave,
    pub seq2: SeqSave,
    // Drums
    pub drums: DrumsSave,
    // Effects
    pub reverb: ReverbSave,
    pub delay: DelaySave,
    pub distortion: DistSave,
    pub sidechain: SidechainSave,
    pub filter1: FilterSave,
    pub filter2: FilterSave,
    pub routing: RoutingSave,
}

#[derive(Serialize, Deserialize)]
pub struct SeqSave { pub num_steps: usize, pub steps: Vec<Option<u8>> }

#[derive(Serialize, Deserialize)]
pub struct DrumsSave { pub num_steps: usize, pub swing: f32, pub tracks: Vec<TrackSave> }

#[derive(Serialize, Deserialize)]
pub struct TrackSave { pub kind: u8, pub steps: Vec<u8>, pub muted: bool, pub volume: f32 }

#[derive(Serialize, Deserialize)]
pub struct ReverbSave { pub enabled: bool, pub room_size: f32, pub damping: f32, pub mix: f32 }

#[derive(Serialize, Deserialize)]
pub struct DelaySave { pub enabled: bool, pub time_ms: f32, pub feedback: f32, pub mix: f32 }

#[derive(Serialize, Deserialize)]
pub struct DistSave { pub enabled: bool, pub drive: f32, pub tone: f32, pub level: f32 }

#[derive(Serialize, Deserialize)]
pub struct SidechainSave {
    pub enabled: bool, pub depth: f32, pub release_ms: f32,
    pub duck_s1: bool, pub duck_s2: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FilterSave {
    pub enabled: bool,
    pub mode: u8,     // 0=LP 1=HP 2=BP
    pub cutoff: f32,
    pub q: f32,
}

#[derive(Serialize, Deserialize)]
pub struct RoutingSave {
    pub s1_reverb: f32, pub s1_delay: f32, pub s1_dist: f32,
    pub s2_reverb: f32, pub s2_delay: f32, pub s2_dist: f32,
    pub dr_reverb: f32, pub dr_delay: f32, pub dr_dist: f32,
}
