extern crate bson;
use async_channel::Sender;
use super::models;

pub enum MetadataResponse {
    Album(String, models::AlbumMeta)  // folder URI and info
}

pub trait MetadataProvider {
    fn new(result_sender: Sender<MetadataResponse>) -> Self;
    fn get_album_meta(&self, folder_uri: &str, key: bson::Document);
    // fn get_artist_meta(&self, key: bson::Document);
}
