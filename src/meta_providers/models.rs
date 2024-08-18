use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ImageSize {
    Small, // Around 32x32
    Medium, // Around 64x64
    Large, // Around 128x128
    ExtraLarge, // Around 256x256
    Mega // 512x512 or more
}

// Common building blocks that can be shared between different providers
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag {
    pub url: String,
    pub name: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageMeta {
    pub size: ImageSize,
    #[serde(rename = "#text")]
    pub url: String
}

// Album
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub struct Wiki {
    pub content: String,
    /// "Read more" URLs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub attribution: String // Mandatory. If public domain or local-only, specify so explicitly.
}

// Standard (provider-agnostic) metadata structures for use across the app.
// All providers must return these structs instead of their own formats.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub struct AlbumMeta {
    pub name: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    pub tags: Vec<Tag>,
    pub image: Vec<ImageMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki: Option<Wiki>,
}
