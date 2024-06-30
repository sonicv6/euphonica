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
extern crate fasthash;
use std::{
    path::PathBuf,
    fs::create_dir_all,
};
use fasthash::murmur2;

#[derive(Debug)]
pub struct AlbumArtCache {
    cache_path: PathBuf
}

pub fn strip_filename_linux(path: &String) -> &str {
    // MPD insists on having a trailing slash so here we go
    if let Some(last_slash) = path.rfind("/") {
        return &path[..last_slash + 1];
    }
    &path[..]
}

impl AlbumArtCache {
    pub fn new(app_cache_path: &PathBuf) -> Self {
        let mut cache_path = app_cache_path.clone();
        cache_path.push("albumart");
        create_dir_all(&cache_path).expect("ERROR: cannot create cache folder");

        Self {
            cache_path
        }
    }

    pub fn get_path_for(&self, folder_uri: &str) -> (PathBuf, PathBuf) {
        // Returns both a full-resolution path and a thumbnail path.
        // Do not include filename in URI.
        let hashed = murmur2::hash64(folder_uri).to_string();
        let mut path = self.cache_path.clone();
        let mut thumb_path = path.clone();
        path.push(hashed.clone() + ".png");
        thumb_path.push(hashed + "_thumb.png");
        (path, thumb_path)
    }

    // pub fn download_for(&self, folder_uri: &PathBuf) {
        // Do not include filename in URI.
        // Send download request to wrapper. Wrapper will pass to child client
        // in background thread.
    //     println!("Fetching album art for path: {:?}", folder_uri);
    //     if let Some(s) = folder_uri.to_str() {
    //         let _ = self.sender.send_blocking(MpdMessage::AlbumArt(String::from(s)));
    //     }
    // }
}