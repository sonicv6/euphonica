extern crate bson;
use async_channel::Sender;
use image::DynamicImage;
use super::models;
pub enum MetadataResponse {
    AlbumMeta(String, models::AlbumMeta),  // folder URI and info
    AlbumArt(String, DynamicImage, DynamicImage),  // folder URI, hires, thumbnail
    ArtistMeta(String, models::ArtistMeta),  // artist name and info
    ArtistAvatar(String, DynamicImage, DynamicImage),  // artist name, hires, thumbnail
}

/// Common provider-agnostic utilities.
pub mod utils {
    use std::sync::{Arc, Mutex};
    use reqwest::blocking::Client;
    use image::DynamicImage;
    use crate::utils;
    use super::*;

    /// Get a file from the given URL as bytes. Useful for downloading images.
    fn get_file(
        client: Arc<Mutex<Client>>,
        url: &str
    ) -> Option<Vec<u8>> {
        let response = client
            .lock()
            .ok()?
            .get(url)
            .send();
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
        client: Arc<Mutex<Client>>,
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
            if let Some(bytes) = get_file(client.clone(), image.url.as_ref()) {
                println!("Downloaded image from: {:?}", &image.url);
                if let Some(image) = utils::read_image_from_bytes(bytes) {
                    return Ok(image);
                }
            }
        }
        Err(String::from("This album's metadata provided image URLs but none of them could be downloaded."))
    }
}

pub trait MetadataProvider {
    fn new(result_sender: Sender<MetadataResponse>) -> Self;
    /// Get wiki & other metadata if possible.
    fn get_album_meta(&self, folder_uri: &str, key: bson::Document);
    /// Get bio & artist avatar if possible.
    fn get_artist_meta(&self, key: bson::Document);
}
