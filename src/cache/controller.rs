// Cache system to store album arts, artist avatars, wikis, bios,
// you name it.
// This helps avoid having to query the same thing multiple times,
// whether from MPD or from Last.fm.
// Images are stored as resized PNG files on disk.
// - Album arts are named with hashes of their URIs (down to the album's
//   folder). This is because all albums have URIs, but not all have
//   MusicBrainz IDs.
// - Artist avatars are named with hashes of their names. Artist names can be substrings
//   of artist tags instead of the full tags.
// - Text data is stored as BSON in PoloDB as most of the time we'll be querying
//   from Last.fm.
extern crate stretto;
extern crate fasthash;
extern crate bson;
extern crate polodb_core;
use image::DynamicImage;
use once_cell::sync::Lazy;
use async_channel::{Sender, Receiver};
use std::{
    fmt,
    rc::Rc,
    path::PathBuf,
    cell::OnceCell,
    fs::create_dir_all
};
use gtk::{
    glib,
    gio,
    gdk::Texture
};
use gio::{
    prelude::*,
    Settings
};
use glib::{
    BoxedAnyObject,
    clone,
    closure_local
};
use fasthash::murmur2;

use crate::{
    utils::{settings_manager, deduplicate},
    client::MpdMessage,
    meta_providers::{
        prelude::*,
        MetadataResponse,
        models::{AlbumMeta, ArtistMeta},
        lastfm::LastfmWrapper
    },
};

use super::CacheState;

pub enum CacheContentType {
    AlbumArt(bool),  // use true to get thumbnail
    ArtistAvatar(bool),  // use true to get thumbnail
}

pub static ALBUMART_PLACEHOLDER: Lazy<Texture> = Lazy::new(|| {
    println!("Loading placeholder texture...");
    Texture::from_resource("/org/euphonia/Euphonia/albumart-placeholder.png")
});

pub struct Cache {
    albumart_path: PathBuf,
    avatar_path: PathBuf,
    // In-memory image cache.
    // gdk::Textures are GObjects, which by themselves are boxed reference-counted.
    // This means that even if a texture is evicted from this cache, as long as there
    // is a widget on screen still using it, it will not actually leave RAM.
    // This cache merely holds an additional reference to each texture to keep them
    // around when no widget using them are being displayed, so as to reduce disk
    // thrashing while quickly scrolling through like a million albums.
    //
    // This cache's keys are uri:<folder-level URI>s (for album arts) or artist:<name>
    // (for avatars).
    image_cache: stretto::Cache<(String, bool), Texture>,
    // Embedded document database for caching responses from metadata providers.
    // Think MongoDB x SQLite x Rust.
    doc_cache: polodb_core::Database,
    mpd_sender: OnceCell<Sender<MpdMessage>>,
    meta_sender: Sender<MetadataResponse>,
    // TODO: Refactor into generic metadata providers for modularity
    meta_provider: Rc<LastfmWrapper>,
    state: CacheState,
    settings: Settings
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Cache")
            .field("albumart_path", &self.albumart_path)
            .field("avatar_path", &self.avatar_path)
            .finish()
    }
}

impl Cache {
    pub fn new(
        app_cache_path: &PathBuf
    ) -> Rc<Self> {
        let (
            meta_sender,
            meta_receiver
        ): (Sender<MetadataResponse>, Receiver<MetadataResponse>) = async_channel::bounded(1);
        let mut albumart_path = app_cache_path.clone();
        albumart_path.push("albumart");
        create_dir_all(&albumart_path)
            .expect("ERROR: cannot create albumart cache folder");

        let mut avatar_path = app_cache_path.clone();
        avatar_path.push("avatar");
        create_dir_all(&avatar_path)
            .expect("ERROR: cannot create albumart cache folder");

        // TODO: figure out max cost based on user-selectable RAM limit
        // TODO: figure out cost of textures based on user-selectable resolution
        let image_cache = stretto::Cache::new(10240, 1024).unwrap();
        let meta_provider = Rc::new(LastfmWrapper::new(meta_sender.clone()));

        let mut doc_path = app_cache_path.clone();
        doc_path.push("metadata.polodb");
        let doc_cache = polodb_core::Database::open_file(doc_path)
            .expect("ERROR: cannot create a metadata database");

        let res = Rc::new(Self {
            albumart_path,
            avatar_path,
            image_cache,
            doc_cache,
            meta_provider,
            mpd_sender: OnceCell::new(),
            meta_sender,
            state: CacheState::default(),
            settings: settings_manager().child("client")
        });

        res.clone().setup_channel(meta_receiver);

        res
    }

    pub fn set_mpd_sender(&self, sender: Sender<MpdMessage>) {
        let _ = self.mpd_sender.set(sender);
    }

    pub fn get_sender(&self) -> Sender<MetadataResponse> {
        self.meta_sender.clone()
    }

    fn setup_channel(self: Rc<Self>, receiver: Receiver<MetadataResponse>) {
        // Set up a listener for updates from metadata providers.
        glib::MainContext::default().spawn_local(clone!(
            #[weak(rename_to = this)]
            self,
            async move {
            use futures::prelude::*;
            // Allow receiver to be mutated, but keep it at the same memory address.
            // See Receiver::next doc for why this is needed.
            let mut receiver = std::pin::pin!(receiver);
            while let Some(request) = receiver.next().await {
                match request {
                    MetadataResponse::AlbumMeta(folder_uri, model) => {
                        this.on_album_meta_downloaded(folder_uri.as_ref(), model)
                    }
                    MetadataResponse::ArtistMeta(name, model) => {
                        this.on_artist_meta_downloaded(name.as_ref(), model)
                    }
                    MetadataResponse::AlbumArt(folder_uri, hires, thumb) => {
                        this.on_album_art_downloaded(folder_uri.as_ref(), &hires, &thumb)
                    }
                    MetadataResponse::ArtistAvatar(name, hires, thumb) => {
                        this.on_artist_avatar_downloaded(name.as_ref(), &hires, &thumb)
                    }
                }
            }
        }));
    }

    /// Take the downloaded DynamicImages and save locally.
    /// This should be called whenever an album art is downloaded, regardless of provider,
    /// including from MPD itself.
    fn on_album_art_downloaded(&self, folder_uri: &str, hires: &DynamicImage, thumbnail: &DynamicImage) {
        let thumbnail_path = self.get_path_for(folder_uri, CacheContentType::AlbumArt(true));
        let path = self.get_path_for(folder_uri, CacheContentType::AlbumArt(false));
        if !path.exists() || !thumbnail_path.exists() {
            if let (Ok(_), Ok(_)) = (
                hires.save(path),
                thumbnail.save(thumbnail_path)
            ) {
                self.state.emit_with_param("album-art-downloaded", &folder_uri);
            }
        }
    }

    fn on_album_meta_downloaded(&self, folder_uri: &str, model: AlbumMeta) {
        // Insert into cache
        println!("Downloaded album meta for {}. Caching...", folder_uri);
        let insert_res = self.doc_cache.collection::<AlbumMeta>("album").insert_one(model);
        if let Err(msg) = insert_res {
            println!("{:?}", msg);
        }
        // Notify widgets
        self.state.emit_with_param("album-meta-downloaded", folder_uri);
    }

    fn on_artist_meta_downloaded(&self, name: &str, model: ArtistMeta) {
        // Insert into cache
        println!("Downloaded album meta for {}. Caching...", name);
        let insert_res = self.doc_cache.collection::<ArtistMeta>("artist").insert_one(model);
        if let Err(msg) = insert_res {
            println!("{:?}", msg);
        }
        // Notify widgets
        self.state.emit_with_param("artist-meta-downloaded", name);
    }

    pub fn get_cache_state(&self) -> CacheState {
        self.state.clone()
    }

    pub fn get_path_for(&self, key: &str, content_type: CacheContentType) -> PathBuf {
        match content_type {
            // Returns the full-resolution path.
            // Do not include filename in URI.
            CacheContentType::AlbumArt(thumbnail) => {
                let hashed = murmur2::hash64(key).to_string();

                let mut path = self.albumart_path.clone();
                if thumbnail {
                    path.push(hashed + "_thumb.png");
                }
                else {
                    path.push(hashed + ".png");
                }
                path
            }
            CacheContentType::ArtistAvatar(thumbnail) => {
                let hashed = murmur2::hash64(key).to_string();

                let mut path = self.avatar_path.clone();
                if thumbnail {
                    path.push(hashed + "_thumb.png");
                }
                else {
                    path.push(hashed + ".png");
                }
                path
            }
        }
    }

    pub fn load_local_album_art(&self, folder_uri: &str, thumbnail: bool) -> Option<Texture> {
        // This is a public method to allow other controllers to get album arts for
        // specific songs/albums directly if possible.
        // Without this, they can only get the textures via signals, which have overheads.
        // First try to get from cache
        let key = (format!("uri:{}", folder_uri), thumbnail);
        if let Some(tex) = self.image_cache.get(&key) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Cloning GObjects is cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, try loading from disk into cache
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = self.get_path_for(folder_uri, CacheContentType::AlbumArt(thumbnail));
        if path.exists() {
            if let Ok(tex) = Texture::from_filename(path) {
                self.image_cache.insert(key, tex.clone(), if thumbnail {1} else {16});
                self.image_cache.wait().unwrap();
                return Some(tex);
            }
        }
        None
    }

    /// Convenience method to check whether album art for a given album is locally available,
    /// and if not, queue its downloading from MPD.
    pub fn ensure_local_album_art(&self, folder_uri: &str) {
        let thumbnail_path = self.get_path_for(folder_uri, CacheContentType::AlbumArt(true));
        let path = self.get_path_for(folder_uri, CacheContentType::AlbumArt(false));
        if !path.exists() || !thumbnail_path.exists() {
            if self.settings.boolean("mpd-download-album-art") {
                if let Some(sender) = self.mpd_sender.get() {
                    // Queue download from MPD if enabled
                    // Place request with MpdWrapper
                    let _ = sender.send_blocking(MpdMessage::AlbumArt(
                        folder_uri.to_string()
                    ));
                }
            }
        }
    }

    // TODO: GUI for downloading album arts from external providers.

    /// Batched version of ensure_local_album_art.
    /// The list of folder-level URIs will be deduplicated internally to avoid fetching the same
    /// album art multiple times. This is useful for fetching album arts of songs in the queue,
    /// for example.
    /// For now album arts will not be automatically downloaded from remote providers.
    pub fn ensure_local_album_arts(&self, folder_uris: Vec<String>) {
        let deduped: Vec<String> = deduplicate(&folder_uris);
        for folder_uri in deduped.iter() {
            self.ensure_local_album_art(folder_uri);
        }
    }

    fn get_album_key(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or BOTH of these
        album: Option<&str>, artist: Option<&str>
    ) -> Result<bson::Document, &str> {
        if let Some(id) = mbid {
            Ok(bson::doc! {
                "mbid": id.to_string()
            })
        }
        else if album.is_some() && artist.is_some() {
            Ok(bson::doc! {
                "name": album.unwrap().to_string(),
                "artist": artist.unwrap().to_string()
            })
        }
        else {
            Err("If no mbid is available, both album name and artist must be specified")
        }
    }

    pub fn load_local_album_meta(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or BOTH of these
        album: Option<&str>, artist: Option<&str>,
    ) -> Option<AlbumMeta> {
        if let Ok(key) = self.get_album_key(mbid, album, artist) {
            let result = self.doc_cache.collection::<AlbumMeta>("album").find_one(key);
            if let Ok(res) = result {
                if let Some(info) = res {
                    println!("Album info cache hit!");
                    return Some(info);
                }
                println!("Album info cache miss");
                return None;
            }
            println!("{:?}", result.err());
            return None;
        }
        println!("No key!");
        None
    }

    pub fn ensure_local_album_meta(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or BOTH of these
        album: Option<&str>, artist: Option<&str>,
        folder_uri: &str
    ) {
        // Check whether we have this album cached
        if let Ok(key) = self.get_album_key(
            mbid,
            album,
            artist
        ) {
            let result = self.doc_cache.collection::<AlbumMeta>("album").find_one(key.clone());
            if let Ok(response) = result {
                if response.is_none() {
                    // TODO: Refactor to accommodate multiple metadata providers
                    self.meta_provider.get_album_meta(folder_uri, key);
                }
                else {
                    println!("Album info already cached, won't download again");
                }
            }
            else {
                println!("{:?}", result.err());
            }
        }
    }

    fn get_artist_key(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or this
        artist: Option<&str>
    ) -> Result<bson::Document, &str> {
        if let Some(id) = mbid {
            Ok(bson::doc! {
                "mbid": id.to_string()
            })
        }
        else if let Some(artist) = artist {
            Ok(bson::doc! {
                "artist": artist.to_string()
            })
        }
        else {
            Err("Either mbid (preferred) or artist name must be specified")
        }
    }

    pub fn load_local_artist_meta(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or this
        artist: Option<&str>
    ) -> Option<ArtistMeta> {
        if let Ok(key) = self.get_artist_key(mbid, artist) {
            let result = self.doc_cache.collection::<ArtistMeta>("artist").find_one(key);
            if let Ok(res) = result {
                if let Some(info) = res {
                    println!("Artist info cache hit!");
                    return Some(info);
                }
                println!("Artist info cache miss");
                return None;
            }
            println!("{:?}", result.err());
            return None;
        }
        println!("No key!");
        None
    }

    pub fn ensure_local_artist_meta(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or this
        artist: Option<&str>
    ) {
        // Check whether we have this album cached
        if let Ok(key) = self.get_artist_key(
            mbid,
            artist
        ) {
            let result = self.doc_cache.collection::<ArtistMeta>("artist").find_one(key.clone());
            if let Ok(response) = result {
                if response.is_none() {
                    // TODO: Refactor to accommodate multiple metadata providers
                    self.meta_provider.get_artist_meta(key);
                }
                else {
                    println!("Artist info already cached, won't download again");
                }
            }
            else {
                println!("{:?}", result.err());
            }
        }
    }

    /// Public method to allow other controllers to get artist avatars for
    /// directly if possible.
    /// Without this, they can only get the textures via signals, which have overhead.
    /// To queue downloading artist avatars, simply use ensure_local_artist_meta, which
    /// will also download artist avatars if the provider is configured to do so.
    pub fn load_local_artist_avatar(
        &self,
        name: &str,
        thumbnail: bool
    ) -> Option<Texture> {
        // First try to get from cache
        let cache_key = (format!("artist:{}", name), thumbnail);
        if let Some(tex) = self.image_cache.get(&cache_key) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Cloning GObjects is cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, try loading from disk into cache
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = self.get_path_for(name, CacheContentType::ArtistAvatar(thumbnail));
        if path.exists() {
            if let Ok(tex) = Texture::from_filename(path) {
                self.image_cache.insert(cache_key, tex.clone(), if thumbnail {1} else {16});
                self.image_cache.wait().unwrap();
                return Some(tex);
            }
        }
        None
    }

    /// Take the downloaded DynamicImages and save locally.
    /// This should be called whenever an artist avatar is downloaded, regardless of provider,
    /// including from MPD itself.
    fn on_artist_avatar_downloaded(&self, name: &str, hires: &DynamicImage, thumbnail: &DynamicImage) {
        let thumbnail_path = self.get_path_for(name, CacheContentType::ArtistAvatar(true));
        let path = self.get_path_for(name, CacheContentType::ArtistAvatar(false));
        if !path.exists() || !thumbnail_path.exists() {
            if let (Ok(_), Ok(_)) = (
                hires.save(path),
                thumbnail.save(thumbnail_path)
            ) {
                self.state.emit_with_param("artist-avatar-downloaded", &name);
            }
        }
    }
}
