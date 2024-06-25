// Album arts are fetched from mpd whether they're remote or local.
// To make this faster we'll cache all downloaded images locally.
// Cached arts are named after a hash of their URI.
// That way, given a song's URI we can strip the filename out, hash it and
// see if there's a cached album art named with that hash.
// This also means that album arts will be folder-based.
// This is to avoid having to keep a DB of which song uses which image locally
// or having to name the cached files with their paths (fwd and backslash escape
// issue, also looks horrible) or having to recreate the whole folder tree in
// the cache (slow & cumbersome).
// For simplicity cached album arts will be in PNG format regardless or their original ones.

use std::{
    path::PathBuf,
    cell::{Rc},
    fs::create_dir_all,
};
use crate::client::wrapper::MpdMessage;
use async_channel::Sender;
use fasthash::murmur2;

struct AlbumArtCache {
    cache_path: Rc<PathBuf>,
    sender: Sender<MpdMessage>
}

impl AlbumArtCache {
    pub fn new(app_cache_path: &PathBuf, sender: Sender<MpdMessage>) -> Self {
        let mut cache_path = app_cache_path.clone();
        cache_path.push("albumart");
        create_dir_all(&cache_path);

        Self {
            cache_path,
            sender
        }
    }

    pub fn try_get_path_for(&self, folder_uri: &PathBuf) -> Option<String> {
        // Do not include filename in URI.
        if let Some(s) = folder_uri.to_str() {
            let mut path = self.cache_path.clone();
            path.push(murmur2::hash64(s).to_string() + ".png");
            return Some(path.to_str());
        }
        None
    }

    pub fn download_for(&self, folder_uri: &PathBuf) {
        // Do not include filename in URI.
        // Send download request to wrapper. Wrapper will pass to child client
        // in background thread.
        if let Some(s) = folder_uri.to_str() {
            let _ = self.sender.send_blocking(MpdMessage::AlbumArt(String::from(s)));
        }
    }
}