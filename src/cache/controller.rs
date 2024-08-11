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
use once_cell::sync::Lazy;
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
use async_channel::{Sender, Receiver};

use crate::{
    utils::settings_manager,
    client::{MpdMessage, ClientState}
};

use super::CacheState;

#[derive(Debug)]
pub enum CacheMessage {
    AlbumArt(String, bool), // folder-level URI. If bool is true then will return 256x thumbnail.
    AlbumWiki(String), // album name, not URI
    ArtistAvatar(String), // Artist tag only
    ArtistBio(String), // album name, not URI
    AlbumArtistAvatar(String), // for AlbumArtist tag only
    AlbumArtistBio(String), // album name, not URI
}

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
    mpd_sender: Sender<MpdMessage>,
    // receiver: RefCell<Receiver<CacheMessage>>,
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
        let mut albumart_path = app_cache_path.clone();
        albumart_path.push("albumart");
        create_dir_all(&albumart_path).expect("ERROR: cannot create albumart cache folder");

        let mut avatar_path = app_cache_path.clone();
        avatar_path.push("avatar");
        create_dir_all(&avatar_path).expect("ERROR: cannot create albumart cache folder");

        // TODO: figure out max cost based on user-selectable RAM limit
        // TODO: figure out cost of textures based on user-selectable resolution
        let image_cache = stretto::Cache::new(10240, 1024).unwrap();

        let res = Rc::new(Self {
            albumart_path,
            avatar_path,
            image_cache,
            mpd_sender,
            state: CacheState::default(),
            settings: settings_manager().child("client")
        });

        res.clone().bind_state(mpd_state);

        res
    }

    pub fn get_cache_state(&self) -> CacheState {
        self.state.clone()
    }

    fn bind_state(self: Rc<Self>, client_state: ClientState) {
        client_state.connect_closure(
            "album-art-downloaded",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, folder_uri: String| {
                    this.state.emit_album_art_downloaded(&folder_uri);
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

    pub fn get_album_art(&self, folder_uri: &str, thumbnail: bool) -> Option<Texture> {
        // Convenience method to either load locally or queue download from remote sources.
        if let Some(tex) = self.load_local_album_art(folder_uri, thumbnail) {
            return Some(tex);
        }
        // Album art not available locally
        self.ensure_local_album_art(folder_uri);
        // TODO: Implement Lastfm album art downloading too (but prioritise MPD version if available)
        None
    }
}
