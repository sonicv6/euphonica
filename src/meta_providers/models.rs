use serde::{Deserialize, Serialize};

// Common building blocks that can be shared between different providers
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag {
    pub url: String,
    pub name: String
}

pub trait Tagged {
    fn get_tags(&self) -> &[Tag];
}

pub trait Merge {
    /// Merge data from another AlbumMeta instance.
    /// - Vec fields are concatenated.
    /// - Option fields are filled in if our own is None and theirs is Some.
    /// - Normal fields are unchanged.
    fn merge(self, other: Self) -> Self;
}

/// Image size enumeration. Note to self: automatic derivation of Ord traits assumes
/// that the variants are declared in increasing order.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub enum ImageSize {
    Small, // Around 32x32
    Medium, // Around 64x64
    Large, // Around 128x128
    ExtraLarge, // Around 256x256
    Mega // 512x512 or more
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageMeta {
    pub size: ImageSize,
    #[serde(rename = "#text")]
    pub url: String
}

pub trait HasImage {
    fn get_images(&self) -> &[ImageMeta];
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

impl Merge for AlbumMeta {
    fn merge(mut self, AlbumMeta { mbid, mut tags, mut image, url, wiki, .. }: Self) -> Self {
        if mbid.is_some() && self.mbid.is_none() {
            self.mbid = mbid;
        }
        self.tags.append(&mut tags);
        self.image.append(&mut image);
        if url.is_some() && self.url.is_none() {
            self.url = url;
        }
        if wiki.is_some() && self.wiki.is_none() {
            self.wiki = wiki;
        }
        self
    }
}

impl Tagged for AlbumMeta {
    fn get_tags(&self) -> &[Tag] {
        &self.tags
    }
}

impl HasImage for AlbumMeta {
    fn get_images(&self) -> &[ImageMeta] {
        &self.image
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub struct ArtistMeta {
    pub name: String,
    pub tags: Vec<Tag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    pub similar: Vec<ArtistMeta>,
    pub image: Vec<ImageMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<Wiki>
}

impl Merge for ArtistMeta {
    fn merge(mut self, ArtistMeta { mut tags, mut similar, mut image, url, bio, mbid, .. }: Self) -> Self {
        self.tags.append(&mut tags);
        self.image.append(&mut image);
        self.similar.append(&mut similar);
        if mbid.is_some() && self.mbid.is_none() {
            self.mbid = mbid;
        }
        if url.is_some() && self.url.is_none() {
            self.url = url;
        }
        if bio.is_some() && self.bio.is_none() {
            self.bio = bio;
        }
        self
    }
}

impl Tagged for ArtistMeta {
    fn get_tags(&self) -> &[Tag] {
        &self.tags
    }
}

impl HasImage for ArtistMeta {
    fn get_images(&self) -> &[ImageMeta] {
        &self.image
    }
}
