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
use async_channel::{Sender, Receiver};
use rustc_hash::FxHashSet;
use std::{
    fmt,
    time::Duration,
    sync::{Arc, RwLock},
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
use glib::clone;
use fasthash::murmur2;

use crate::{
    client::MpdMessage,
    common::{AlbumInfo, ArtistInfo},
    meta_providers::{
        prelude::*,
        models,
        utils::get_best_image,
        Metadata,
        MetadataChain,
        lastfm::LastfmWrapper,
        musicbrainz::MusicBrainzWrapper
    },
    utils::{resize_image, settings_manager}
};
use crate::meta_providers::models::ArtistMeta;

use super::CacheState;

enum CacheTask {
    // Separate task since we might just need the textual metadata
    // (album art can be provided locally)
    AlbumArt(String, bson::Document, PathBuf, PathBuf),
    AlbumMeta(String, bson::Document),
    // Both meta and album art together, since for now we cannot provide artist avatars
    // locally.
    ArtistMeta(bson::Document, PathBuf, PathBuf)
}

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
    doc_cache: Arc<RwLock<polodb_core::Database>>,
    mpd_sender: OnceCell<Sender<MpdMessage>>,
    fg_sender: Sender<Metadata>,
    bg_sender: Sender<CacheTask>,
    meta_providers: Arc<RwLock<MetadataChain>>,
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
            fg_sender,
            fg_receiver
        ): (Sender<Metadata>, Receiver<Metadata>) = async_channel::bounded(1);
        let (
            bg_sender,
            bg_receiver
        ): (Sender<CacheTask>, Receiver<CacheTask>) = async_channel::unbounded();
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
        let mut doc_path = app_cache_path.clone();

        let mut providers = MetadataChain::new();
        // TODO: Allow user reordering
        providers.providers = vec![
            Box::new(MusicBrainzWrapper::new()),
            Box::new(LastfmWrapper::new())
        ];
        doc_path.push("metadata.polodb");
        let cache = Self {
            albumart_path,
            avatar_path,
            image_cache: stretto::Cache::new(10240, 1024).unwrap(),
            doc_cache: Arc::new(RwLock::new(
                polodb_core::Database::open_file(doc_path).expect("ERROR: cannot create a metadata database")
            )),
            meta_providers: Arc::new(RwLock::new(providers)),
            mpd_sender: OnceCell::new(),
            fg_sender: fg_sender.clone(),
            bg_sender,
            state: CacheState::default(),
            settings: settings_manager().child("client")
        };
        let res = Rc::new(cache);

        res.clone().setup_channel(bg_receiver, fg_sender, fg_receiver);
        res
    }

    pub fn set_mpd_sender(&self, sender: Sender<MpdMessage>) {
        let _ = self.mpd_sender.set(sender);
    }

    pub fn get_sender(&self) -> Sender<Metadata> {
        self.fg_sender.clone()
    }

    fn setup_channel(
        self: Rc<Self>,
        bg_receiver: Receiver<CacheTask>,
        fg_sender: Sender<Metadata>,
        fg_receiver: Receiver<Metadata>
    ) {
        // Handle remote metadata fetching tasks in another thread
        let doc_cache = self.clone().doc_cache.clone();
        let providers = self.clone().meta_providers.clone();
        glib::MainContext::default().spawn_local(
            async move {
                use futures::prelude::*;
                // Allow receiver to be mutated, but keep it at the same memory address.
                // See Receiver::next doc for why this is needed.
                let mut receiver = std::pin::pin!(bg_receiver);

                let settings = settings_manager().child("client");
                while let Some(request) = receiver.next().await {
                    match request {
                        CacheTask::AlbumMeta(folder_uri, key) => {
                            let _ = gio::spawn_blocking(clone!(
                                #[strong]
                                fg_sender,
                                #[strong]
                                doc_cache,
                                #[strong]
                                providers,
                                move || {
                                    let res = providers.read().unwrap().get_album_meta(key, None);
                                    if let Some(album) = res {
                                        let _ = doc_cache.write().unwrap().collection::<models::AlbumMeta>("album").insert_one(album);
                                        let _ = fg_sender.send_blocking(Metadata::AlbumMeta(folder_uri));
                                    }
                                    else {
                                        println!("No album meta could be found for {}", &folder_uri);
                                    }
                                }
                            )).await;
                        },
                        CacheTask::ArtistMeta(key, path, thumbnail_path) => {
                            let _ = gio::spawn_blocking(clone!(
                                #[strong]
                                fg_sender,
                                #[strong]
                                doc_cache,
                                #[strong]
                                providers,
                                move || {
                                    // Guaranteed to have this field so just unwrap it
                                    let name = key.get("name").unwrap().as_str().unwrap().to_owned();
                                    let res = providers.read().unwrap().get_artist_meta(key, None);
                                    if let Some(artist) = res {
                                        // Try to download artist avatar too
                                        let res = get_best_image(&artist.image);
                                        if res.is_ok() {
                                            let (hires, thumbnail) = resize_image(res.unwrap());
                                            if !path.exists() || !thumbnail_path.exists() {
                                                if let (Ok(_), Ok(_)) = (
                                                    hires.save(path),
                                                    thumbnail.save(thumbnail_path)
                                                ) {
                                                    let _ = fg_sender.send_blocking(Metadata::ArtistAvatar(name.clone(), false));
                                                }
                                            }
                                        }
                                        else {
                                            println!("[Cache] Failed to download artist avatar: {:?}", res.err());
                                        }
                                        let _ = doc_cache.write().unwrap().collection::<models::ArtistMeta>("artist").insert_one(artist);
                                        let _ = fg_sender.send_blocking(Metadata::ArtistMeta(name));
                                    }

                                    else {
                                        println!("No artist meta could be found for {:?}", &name);
                                    }
                                }
                            )).await;
                        },
                        CacheTask::AlbumArt(folder_uri, key, path, thumbnail_path) => {
                            let _ = gio::spawn_blocking(clone!(
                                #[strong]
                                fg_sender,
                                #[strong]
                                doc_cache,
                                move || {
                                    if let Ok(Some(meta)) = doc_cache
                                        .read()
                                        .unwrap()
                                        .collection::<models::AlbumMeta>("album")
                                        .find_one(key) {
                                            let res = get_best_image(&meta.image);
                                            if res.is_ok() {
                                                let (hires, thumbnail) = resize_image(res.unwrap());
                                                if !path.exists() || !thumbnail_path.exists() {
                                                    if let (Ok(_), Ok(_)) = (
                                                        hires.save(path),
                                                        thumbnail.save(thumbnail_path)
                                                    ) {
                                                        let _ = fg_sender.send_blocking(Metadata::AlbumArt(folder_uri, false));
                                                    }
                                                }
                                            }
                                        }
                                    else {
                                        println!("Cannot download album art: no local album meta could be found for {folder_uri}");
                                    }
                                }
                            )).await;
                            // let thumbnail_path = this.get_path_for(folder_uri, Metadata::AlbumArt(true));
                            // let path = this.get_path_for(folder_uri, Metadata::AlbumArt(false));

                        },
                        // CacheTask::ArtistAvatar(key, path, thumbnail_path) => {
                        //     let _ = gio::spawn_blocking(clone!(
                        //         #[strong]
                        //         fg_sender,
                        //         #[strong]
                        //         doc_cache,
                        //         move || {
                        //             let name = key.get("name").unwrap().as_str().unwrap().to_owned();
                        //             if let Ok(Some(meta)) = doc_cache
                        //                 .read()
                        //                 .unwrap()
                        //                 .collection::<models::ArtistMeta>("artist")
                        //                 .find_one(key) {

                        //                 }
                        //             else {
                        //                 println!("Cannot download artist avatar: no local artist meta could be found for {name}");
                        //             }
                        //         }
                        //     )).await;
                        // }
                    };
                    let _ = glib::timeout_future(Duration::from_millis(
                        (settings.double("meta-provider-delay-between-requests-s") * 1000.0) as u64
                    )).await;
                }
            }
        );
        let this = self.clone();
        // Listen to the background thread.
        glib::MainContext::default().spawn_local(
            async move {
            use futures::prelude::*;
            // Allow receiver to be mutated, but keep it at the same memory address.
            // See Receiver::next doc for why this is needed.
            let mut receiver = std::pin::pin!(fg_receiver);
            while let Some(notify) = receiver.next().await {
                match notify {
                    Metadata::AlbumMeta(folder_uri) => {
                        this.state.emit_with_param("album-meta-downloaded", &folder_uri);
                    }
                    Metadata::ArtistMeta(name) => {
                        this.state.emit_with_param("artist-meta-downloaded", &name);
                    }
                    Metadata::AlbumArt(folder_uri, _) => {
                        this.state.emit_with_param("album-art-downloaded", &folder_uri);
                    }
                    Metadata::ArtistAvatar(name, _) => {
                        this.state.emit_with_param("artist-avatar-downloaded", &name);
                    }
                }
            }
        });
    }

    pub fn get_cache_state(&self) -> CacheState {
        self.state.clone()
    }

    pub fn get_path_for(&self, content_type: Metadata) -> PathBuf {
        match content_type {
            // Returns the full-resolution path.
            // Do not include filename in URI.
            Metadata::AlbumArt(folder_uri, thumbnail) => {
                let hashed = murmur2::hash64(folder_uri).to_string();

                let mut path = self.albumart_path.clone();
                if thumbnail {
                    path.push(hashed + "_thumb.png");
                }
                else {
                    path.push(hashed + ".png");
                }
                path
            },
            Metadata::ArtistAvatar(name, thumbnail) => {
                let hashed = murmur2::hash64(name).to_string();

                let mut path = self.avatar_path.clone();
                if thumbnail {
                    path.push(hashed + "_thumb.png");
                }
                else {
                    path.push(hashed + ".png");
                }
                path
            },
            _ => unreachable!()
        }
    }

    pub fn load_local_album_art(&self, album: &AlbumInfo, thumbnail: bool) -> Option<Texture> {
        // This is a public method to allow other controllers to get album arts for
        // specific songs/albums directly if possible.
        // Without this, they can only get the textures via signals, which have overheads.
        // First try to get from cache
        let folder_uri = album.uri.to_owned();
        let key = (format!("uri:{}", &folder_uri), thumbnail);
        if let Some(tex) = self.image_cache.get(&key) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Cloning GObjects is cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, try loading from disk into cache
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = self.get_path_for(Metadata::AlbumArt(folder_uri, thumbnail));
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
    /// If MPD doesn't have one locally, we'll try fetching from all the enabled metadata providers.
    pub fn ensure_local_album_art(&self, album: &AlbumInfo) {
        let folder_uri = album.uri.to_owned();
        let thumbnail_path = self.get_path_for(Metadata::AlbumArt(folder_uri.clone(), true));
        let path = self.get_path_for(Metadata::AlbumArt(folder_uri.clone(), false));
        if !path.exists() || !thumbnail_path.exists() {
            if self.settings.boolean("mpd-download-album-art") {
                if let Some(sender) = self.mpd_sender.get() {
                    // Queue download from MPD if enabled
                    // Place request with MpdWrapper
                    let _ = sender.send_blocking(MpdMessage::AlbumArt(
                        folder_uri.to_string(),
                        path,
                        thumbnail_path
                    ));
                }
            }
            else {
                // Hop straight to remote providers. For this we'll need to have album metas ready,
                // so schedule that first.
                self.ensure_local_album_meta(album);
                if let Ok(key) = self.get_album_key(album) {
                    self.bg_sender.send_blocking(CacheTask::AlbumArt(folder_uri, key, path, thumbnail_path));
                }
            }
        }
    }

    // TODO: GUI for downloading album arts from external providers.
    /// Batched version of ensure_local_album_art.
    /// The list of folder-level URIs will be deduplicated internally to avoid fetching the same
    /// album art multiple times. This is useful for fetching album arts of songs in the queue,
    /// for example.
    pub fn ensure_local_album_arts(&self, albums: &[&AlbumInfo]) {
        let mut seen = FxHashSet::default();
        for album in albums.iter() {
            let folder_uri = &album.uri;
            if seen.insert(folder_uri.to_owned()) {
                println!("Deduped: {}", folder_uri);
                self.ensure_local_album_art(album);
            }
        }
    }

    fn get_album_key(
        &self,
        album: &AlbumInfo
    ) -> Result<bson::Document, &str> {
        // AlbumInfo has to have either this (preferred)
        let mbid: Option<&str> = album.mbid.as_deref();
        // Or BOTH of these
        let title: Option<&str> = Some(album.title.as_ref());
        let artist: Option<&str> = album.get_artist_tag();
        if let Some(id) = mbid {
            Ok(bson::doc! {
                "mbid": id.to_string()
            })
        }
        else if title.is_some() && artist.is_some() {
            Ok(bson::doc! {
                "name": title.unwrap().to_string(),
                "artist": artist.unwrap()
            })
        }
        else {
            Err("If no mbid is available, both album name and artist must be specified")
        }
    }

    pub fn load_local_album_meta(
        &self,
        album: &AlbumInfo,
    ) -> Option<models::AlbumMeta> {
        // Check whether we have this album cached
        if let Ok(key) = self.get_album_key(album) {
            let result = self.doc_cache.read().unwrap().collection::<models::AlbumMeta>("album").find_one(key);
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
        album: &AlbumInfo,
    ) {
        // Needed for signalling
        let folder_uri: &str = &album.uri;
        // Check whether we have this album cached
        if let Ok(key) = self.get_album_key(album) {
            let result = self.doc_cache.read().unwrap().collection::<models::AlbumMeta>("album").find_one(key.clone());
            if let Ok(response) = result {
                if response.is_none() {
                    self.bg_sender.send_blocking(CacheTask::AlbumMeta(folder_uri.to_owned(), key));
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
        artist: &ArtistInfo
    ) -> Result<bson::Document, &str> {
        // Optional
        let mbid: Option<&str> = artist.mbid.as_deref();
        // Mandatory (used for signaling)
        let name: &str = &artist.name;
        if let Some(id) = mbid {
            Ok(bson::doc! {
                "mbid": id.to_string(),
                "name": name.to_string()
            })
        }
        else {
            Ok(bson::doc! {
                "name": name.to_string()
            })
        }
    }

    pub fn load_local_artist_meta(
        &self,
        artist: &ArtistInfo
    ) -> Option<ArtistMeta> {
        if let Ok(key) = self.get_artist_key(artist) {
            let result = self.doc_cache.read().unwrap().collection::<ArtistMeta>("artist").find_one(key);
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
        artist: &ArtistInfo
    ) {
        // Check whether we have this artist cached
        if let Ok(key) = self.get_artist_key(artist) {
            let result = self.doc_cache.read().unwrap().collection::<ArtistMeta>("artist").find_one(key.clone());
            if let Ok(response) = result {
                if response.is_none() {
                    let path = self.get_path_for(Metadata::ArtistAvatar(artist.name.to_owned(), false));
                    let thumbnail_path = self.get_path_for(Metadata::ArtistAvatar(artist.name.to_owned(), true));
                    let _ = self.bg_sender.send_blocking(CacheTask::ArtistMeta(key, path, thumbnail_path));
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
        artist: &ArtistInfo,
        thumbnail: bool
    ) -> Option<Texture> {
        // First try to get from cache
        let name = &artist.name;
        let cache_key = (format!("artist:{}", name), thumbnail);
        if let Some(tex) = self.image_cache.get(&cache_key) {
            // println!("Cache hit:  {} (thumbnail: {})", folder_uri, thumbnail);
            // Cloning GObjects is cheap since they're just references
            return Some(tex.value().clone());
        }
        // If missed, try loading from disk into cache
        // println!("Cache  miss: {} (thumbnail: {})", folder_uri, thumbnail);
        let path = self.get_path_for(Metadata::ArtistAvatar(name.to_owned(), thumbnail));
        if path.exists() {
            if let Ok(tex) = Texture::from_filename(path) {
                self.image_cache.insert(cache_key, tex.clone(), if thumbnail {1} else {16});
                self.image_cache.wait().unwrap();
                return Some(tex);
            }
        }
        None
    }
}
