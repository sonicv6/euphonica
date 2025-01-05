use std::{
    cell::OnceCell,
    rc::Rc,
    vec::Vec,
    sync::OnceLock,
    borrow::Cow
};
use crate::{
    cache::Cache,
    client::{BackgroundTask, MpdWrapper},
    common::{
        Album, Artist, INode, Song
    }
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use glib::subclass::Signal;

use adw::subclass::prelude::*;

use mpd::{search::Operation, Query, Term, error::Error as MpdError};

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Library {
        pub client: OnceCell<Rc<MpdWrapper>>,
        // Each view gets their own list.
        // Album retrieval routine:
        // 1. Library sends request for albums to wrapper
        // 2. Wrapper forwards request to background client
        // 3. Background client gets list of unique album tags
        // 4. For each album tag:
        // 4.1. Get the first song with that tag
        // 4.2. Infer folder_uri, sound quality, albumartist, etc. & pack into AlbumInfo class.
        // 4.3. Send AlbumInfo class to main thread via MpdMessage.
        // 4.4. Wrapper tells Library controller to create an Album GObject with that AlbumInfo &
        // append to the list store.

        pub cache: OnceCell<Rc<Cache>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Library {
        const NAME: &'static str = "EuphonicaLibrary";
        type Type = super::Library;

        fn new() -> Self {
            Self {
                client: OnceCell::new(),
                cache: OnceCell::new()
            }
        }
    }

    impl ObjectImpl for Library {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("album-clicked")
                        .param_types([Album::static_type(), gio::ListStore::static_type()])
                        .build()
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct Library(ObjectSubclass<imp::Library>);
}

impl Default for Library {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Library {
    pub fn setup(&self, client: Rc<MpdWrapper>, cache: Rc<Cache>) {
        let _ = self.imp().cache.set(cache);
        let _ = self.imp().client.set(client);
    }

    fn client(&self) -> &Rc<MpdWrapper> {
        self.imp().client.get().unwrap()
    }

    /// Get all the information available about an album & its contents (won't block;
    /// UI will get notified of result later if one does arrive late).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_album(&self, album: &Album) {
        if let Some(cache) = self.imp().cache.get() {
            cache.ensure_cached_album_meta(album.get_info());
        }
        self.client().queue_background(BackgroundTask::FetchAlbumSongs(album.get_title().to_owned()));
    }

    /// Queue specific songs
    pub fn queue_songs(&self, songs: &[Song], replace: bool, play: bool) {
        // TODO: support executing this atomically as a command list
        if replace {
            self.client().clear_queue();
        }
        self.client().add_multi(&songs.iter().map(|s| s.get_uri().to_owned()).collect::<Vec<String>>());
        if replace && play {
            self.client().play_at(0, false);
        }
    }

    /// Queue all songs in a given album by track order.
    pub fn queue_album(&self, album: Album, replace: bool, play: bool) {
        if replace {
            self.client().clear_queue();
        }
        let mut query = Query::new();
        query.and(Term::Tag(Cow::Borrowed("album")), album.get_title().to_owned());
        self.client().find_add(query);
        if replace && play {
            self.client().play_at(0, false);
        }
    }

    /// Queue all songs of an artist. TODO: allow specifying order.
    pub fn queue_artist(&self, artist: Artist, use_albumartist: bool, replace: bool, play: bool) {
        if replace {
            self.client().clear_queue();
        }
        let mut query = Query::new();
        query.and_with_op(
            Term::Tag(Cow::Borrowed(
                if use_albumartist { "albumartist" } else { "artist" }
            )),
            Operation::Contains,
            artist.get_name().to_owned()
        );
        self.client().find_add(query);
        if replace && play {
            self.client().play_at(0, false);
        }
    }

    /// Get all the information available about an artist (won't block;
    /// UI will get notified of result later via signals).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_artist(&self, artist: Artist) {
        if let Some(cache) = self.imp().cache.get() {
            cache.ensure_cached_artist_meta(artist.get_info());
        }
        self.client().get_artist_content(artist.get_name().to_owned());
    }

    /// Queue a song or folder (when recursive == true) for playback.
    pub fn queue_uri(&self, uri: &str, replace: bool, play: bool, recursive: bool) {
        if replace {
            self.client().clear_queue();
        }
        self.client().add(uri.to_owned(), recursive);
        if replace && play {
            self.client().play_at(0, false);
        }
    }

    /// Queue a playlist for playback.
    pub fn get_playlists(&self) -> Vec<INode> {
        self.client().get_playlists()
    }

    pub fn init_playlist(&self, name: &str) {
        self.client().queue_background(BackgroundTask::FetchPlaylistSongs(name.to_owned()));
    }

    /// Queue a playlist for playback.
    pub fn queue_playlist(&self, name: &str, replace: bool, play: bool) {
        if replace {
            self.client().clear_queue();
        }
        let _ = self.client().load_playlist(name);
        if replace && play {
            self.client().play_at(0, false);
        }
    }

    pub fn rename_playlist(&self, old_name: &str, new_name: &str) -> Result<(), Option<MpdError>>{
        self.client().rename_playlist(old_name, new_name)
    }

    pub fn get_folder_contents(&self, uri: &str) {
        self.client().queue_background(BackgroundTask::FetchFolderContents(uri.to_owned()));
    }

    pub fn init_albums(&self) {
        self.client().queue_background(BackgroundTask::FetchAlbums);
    }

    pub fn init_artists(&self, use_albumartists: bool) {
        self.client().queue_background(BackgroundTask::FetchArtists(use_albumartists));
    }
}
