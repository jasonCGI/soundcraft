//! Immutable lyric revisions with generation-ready section and vocal-role tags.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LyricVersion {
    pub name: String,
    pub text: String,
    pub lead: String,
    pub backing: String,
}

/// A project-saved listening decision for a generated take.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TakeReview {
    pub take_name: String,
    pub rating: u8,
    pub notes: String,
    pub promoted: bool,
    pub dimension: String,
    pub seed: u64,
}

/// Reusable direction for lead and backing vocal generation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VocalProfile {
    pub name: String,
    pub lead: String,
    pub backing: String,
    pub aggression: u8,
    pub clarity: u8,
    pub layers: u8,
}

/// A bounded request for generating an alternate arrangement section.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SectionCandidate {
    pub name: String,
    pub start: i64,
    pub end: i64,
    pub prompt: String,
    pub lyrics: String,
    pub intensity: u8,
    pub transition_ms: u32,
    pub vocal_profile: String,
}
