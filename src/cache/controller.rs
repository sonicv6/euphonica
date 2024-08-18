// Cache system to store album arts, artist avatars, wikis, bios,
// you name it.
// This helps avoid having to query the same thing multiple times,
// whether from MPD or from Last.fm.
// Images are stored as resized PNG files on disk.
// - Album arts are named with hashes of their URIs (down to the album's
//   folder). This is because all albums have URIs, but not all have
//   MusicBrainz IDs.
// - Artist avatars are named after the artists themselves, with special
//   characters removed.
// - Text data is stored as JSON in PoloDB as most of the time we'll be querying
//   from Last.fm.
extern crate stretto;
extern crate fasthash;
extern crate bson;
extern crate polodb_core;
use once_cell::sync::Lazy;
use async_channel::{Sender, Receiver};
use std::{
    fmt,
    rc::Rc,
    path::PathBuf,
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
    clone,
    closure_local
};
use fasthash::murmur2;

use crate::{
    utils::settings_manager,
    client::{MpdMessage, ClientState},
    meta_providers::{
        prelude::*,
        MetadataResponse,
        models::AlbumMeta,
        lastfm::LastfmWrapper
    },
};

use super::CacheState;

pub enum CacheContentType {
    AlbumArt,
    AlbumArtThumbnail,
    Avatar
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
    // This cache's keys are the folder-level URIs (for album arts) or raw artist
    // name (for avatars).
    image_cache: stretto::Cache<(String, bool), Texture>,
    // Embedded document database for caching responses from metadata providers.
    // Think MongoDB x SQLite x Rust.
    doc_cache: polodb_core::Database,
    album_meta_cache: polodb_core::Collection<AlbumMeta>,
    mpd_sender: Sender<MpdMessage>,
    // receiver: RefCell<Receiver<CacheMessage>>,
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
        app_cache_path: &PathBuf,
        mpd_sender: Sender<MpdMessage>,
        mpd_state: ClientState,
    ) -> Rc<Self> {
        let (
            sender,
            receiver
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
        let meta_provider = Rc::new(LastfmWrapper::new(sender.clone()));

        let mut doc_path = app_cache_path.clone();
        doc_path.push("metadata.polodb");
        let doc_cache = polodb_core::Database::open_file(doc_path)
            .expect("ERROR: cannot create a metadata database");
        // Init collection schema
        let album_meta_cache = doc_cache.collection("album");
        // doc_cache.collection("artist");
        // doc_cache.collection("track");

        let res = Rc::new(Self {
            albumart_path,
            avatar_path,
            image_cache,
            doc_cache,
            album_meta_cache,
            meta_provider,
            mpd_sender,
            state: CacheState::default(),
            settings: settings_manager().child("client")
        });

        res.clone().bind_state(mpd_state);
        res.clone().setup_channel(receiver);

        res
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
                    MetadataResponse::Album(folder_uri, model) => this.on_album_meta_downloaded(folder_uri.as_ref(), model)
                }
            }
        }));
    }

    fn on_album_meta_downloaded(&self, folder_uri: &str, model: AlbumMeta) {
        // Insert into cache
        println!("Downloaded album meta for {}. Caching...", folder_uri);
        let insert_res = self.album_meta_cache.insert_one(model);
        if let Err(msg) = insert_res {
            println!("{:?}", msg);
        }
        // Notify widgets
        self.state.emit_with_param("album-meta-downloaded", folder_uri);
    }

    pub fn get_cache_state(&self) -> CacheState {
        self.state.clone()
    }

    fn bind_state(self: Rc<Self>, client_state: ClientState) {
        client_state.connect_closure(
            "album-art-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, folder_uri: String| {
                    this.state.emit_with_param("album-art-downloaded", &folder_uri);
                }
            )
        );

        // Daisy-chain
        client_state.connect_closure(
            "album-art-not-available",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, folder_uri: String| {

                    this.state.emit_with_param("album-art-downloaded", &folder_uri);
                }
            )
        );
    }

    pub fn get_path_for(&self, key: &str, content_type: CacheContentType) -> PathBuf {
        match content_type {
            // Returns the full-resolution path.
            // Do not include filename in URI.
            CacheContentType::AlbumArt => {
                let hashed = murmur2::hash64(key).to_string();

                let mut path = self.albumart_path.clone();
                path.push(hashed.clone() + ".png");
                path
            }
            CacheContentType::AlbumArtThumbnail => {
                let hashed = murmur2::hash64(key).to_string();

                let mut path = self.albumart_path.clone();
                path.push(hashed.clone() + "_thumb.png");
                path
            }
            _ => unimplemented!()
        }
    }

    pub fn load_local_album_art(&self, folder_uri: &str, thumbnail: bool) -> Option<Texture> {
        // This is a public method to allow other controllers to get album arts for
        // specific songs/albums directly if possible.
        // Without this, they can only get the textures via signals, which requires them
        // to possibly iterate through all of their songs/albums every time.
        // First try to get from cache
        if let Some(tex) = self.image_cache.get(&(folder_uri.to_owned(), thumbnail)) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Clone GObjects are cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, try loading from disk into cache
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = if thumbnail {
            self.get_path_for(folder_uri, CacheContentType::AlbumArtThumbnail)
        }
        else {
            self.get_path_for(folder_uri, CacheContentType::AlbumArt)
        };
        if path.exists() {
            if let Ok(tex) = Texture::from_filename(path) {
                self.image_cache.insert((String::from(folder_uri), thumbnail), tex.clone(), if thumbnail {1} else {16});
                self.image_cache.wait().unwrap();
                return Some(tex);
            }
        }
        None
    }

    pub fn ensure_local_album_art(&self, folder_uri: &str) {
        // Convenience method to check whether album art for a given album is locally available,
        // and if not, queue its downloading from enabled remote providers.
        let thumbnail_path = self.get_path_for(folder_uri, CacheContentType::AlbumArtThumbnail);
        let path = self.get_path_for(folder_uri, CacheContentType::AlbumArt);
        if !path.exists() || !thumbnail_path.exists() {
            if self.settings.boolean("mpd-download-album-art") {
                // Queue download from MPD if enabled
                // Place request with MpdWrapper
                let path = self.get_path_for(&folder_uri, CacheContentType::AlbumArt);
                let thumbnail_path = self.get_path_for(&folder_uri, CacheContentType::AlbumArtThumbnail);
                let _ = self.mpd_sender.send_blocking(MpdMessage::AlbumArt(
                    folder_uri.to_owned(), path, thumbnail_path
                ));
            }
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
            let result = self.album_meta_cache.find_one(key);
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
        mbid: Option<String>,
        // Or BOTH of these
        album: Option<String>, artist: Option<String>,
        folder_uri: &str
    ) {
        // Check whether we have this album cached
        if let Ok(key) = self.get_album_key(
            mbid.as_deref(),
            album.as_deref(),
            artist.as_deref()
        ) {
            let result = self.album_meta_cache.find_one(key.clone());
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
}
