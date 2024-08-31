extern crate bson;

use super::models;

pub enum Metadata {
    // folder-level URI, true for thumbnail
    AlbumArt(String, bool),
    // folder-level URI
    AlbumMeta(String),
    // Tag, true for thumbnail
    ArtistAvatar(String, bool),
    // Tag
    ArtistMeta(String)
}

/// Common provider-agnostic utilities.
pub mod utils {
    use image::DynamicImage;
    use crate::utils;
    use super::*;

    /// Get a file from the given URL as bytes. Useful for downloading images.
    fn get_file(
        url: &str
    ) -> Option<Vec<u8>> {
        let response = reqwest::blocking::get(url);
        if let Ok(res) = response {
            if let Ok(bytes) = res.bytes() {
                return Some(bytes.to_vec());
            }
            else {
                println!("get_file: Failed to read response as bytes!");
                return None;
            }
        }
        println!("get_file: {:?}", response.err());
        None
    }

    pub fn get_best_image(
        metas: &[models::ImageMeta]
    ) -> Result<DynamicImage, String> {
        // Get all image URLs, sorted by size in reverse.
        // Avoid cloning by sorting a mutable vector of references.
        let mut images: Vec<&models::ImageMeta> = metas.iter().collect();
        if images.is_empty() {
            return Err(String::from("This album's metadata does not provide any image."));
        }
        images.sort_by_key(|img| img.size);
        for image in images.iter().rev() {
            if let Some(bytes) = get_file(image.url.as_ref()) {
                println!("Downloaded image from: {:?}", &image.url);
                if let Some(image) = utils::read_image_from_bytes(bytes) {
                    return Ok(image);
                }
            }
        }
        Err(String::from("This album's metadata provided image URLs but none of them could be downloaded."))
    }
}

pub trait MetadataProvider: Send + Sync {
    fn new() -> Self where Self: Sized;
    /// Get textual metadata that wouldn't be available as song tags, such as wiki, producer name,
    /// etc. A new AlbumMeta object containing data from both the existing AlbumMeta and newly fetched data. New
    /// data will always overwrite existing fields.
    fn get_album_meta(
        self: &Self, key: bson::Document, existing: Option<models::AlbumMeta>
    ) -> Option<models::AlbumMeta>;
    /// Get textual metadata about an artist, such as biography, DoB, etc.
    /// A new ArtistMeta object containing data from both the existing ArtistMeta and newly fetched data. New
    /// data will always overwrite existing fields.
    fn get_artist_meta(
        self: &Self, key: bson::Document, existing: Option<models::ArtistMeta>
    ) -> Option<models::ArtistMeta>;
}
