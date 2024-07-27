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
    fmt
};
use gtk::gdk::Texture;
use stretto::Cache;
use fasthash::murmur2;
use crate::utils::strip_filename_linux;

pub struct AlbumArtCache {
    // Path to where the downloaded copies are stored
    cache_path: PathBuf,
    // In-memory cache.
    // gdk::Textures are GObjects, which by themselves are boxed reference-counted.
    // This means that even if a texture is evicted from this cache, as long as there
    // is a widget on screen still using it, it will not actually leave RAM.
    // This cache merely holds an additional reference to each texture to keep them
    // around when no widget using them are being displayed, so as to reduce disk
    // thrashing while quickly scrolling through like a million albums.
    //
    // This cache's keys are the folder-level URIs directly, since there's no risk
    // of reserved characters breaking stuff.
    cache: Cache<(String, bool), Texture>
}

impl AlbumArtCache {
    pub fn new(app_cache_path: &PathBuf) -> Self {
        let mut cache_path = app_cache_path.clone();
        cache_path.push("albumart");
        create_dir_all(&cache_path).expect("ERROR: cannot create cache folder");

        // TODO: figure out max cost based on user-selectable RAM limit
        // TODO: figure out cost of textures based on user-selectable resolution
        let cache = Cache::new(10240, 1024).unwrap();

        Self {
            cache_path,
            cache
        }
    }

    pub fn get_path_for(&self, folder_uri: &str) -> PathBuf {
        // Returns the full-resolution path.
        // Do not include filename in URI.
        let hashed = murmur2::hash64(folder_uri).to_string();
        let mut path = self.cache_path.clone();
        path.push(hashed.clone() + ".png");
        path
    }

    pub fn get_thumbnail_path_for(&self, folder_uri: &str) -> PathBuf {
        // Returns the thumbnail path.
        // Do not include filename in URI.
        // We hash the URI to avoid problems with reserved characters in paths.
        let hashed = murmur2::hash64(folder_uri).to_string();
        let mut path = self.cache_path.clone();
        path.push(hashed + "_thumb.png");
        path
    }

    pub fn get_for(&self, folder_uri: &str, thumbnail: bool) -> Option<Texture> {
        // First try to get from cache.
        if let Some(tex) = self.cache.get(&(folder_uri.to_owned(), thumbnail)) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Clone GObjects are cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, load into cache from disk
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = if thumbnail {self.get_thumbnail_path_for(folder_uri)} else {self.get_path_for(folder_uri)};
        if path.exists() {
            if let Ok(tex) = Texture::from_filename(path) {
                self.cache.insert((String::from(folder_uri), thumbnail), tex.clone(), if thumbnail {1} else {16});
                self.cache.wait().unwrap();
                return Some(tex);
            }
            return None;
        }
        // Else return nothing (download request should be sent by user code, not this thing)
        None
    }
}

// Stretto cache does not implement Debug so we'll create a simple one ourselves.
impl fmt::Debug for AlbumArtCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AlbumArtCache(max cost = {})", self.cache.max_cost())
    }
}
