//! Immutable lyric revisions with generation-ready section and vocal-role tags.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LyricVersion {
    pub name: String,
    pub text: String,
    pub lead: String,
    pub backing: String,
}
