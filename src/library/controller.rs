use std::{
    cell::OnceCell,
    rc::Rc,
    vec::Vec,
    sync::OnceLock,
    borrow::Cow
};
use async_channel::Sender;
use crate::{
    client::{
        ClientState,
        MpdMessage
    },
    cache::Cache,
    common::{
        Album,
        Artist
    },
    utils::settings_manager
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use glib::{
    closure_local,
    subclass::Signal
};

use adw::subclass::prelude::*;

use mpd::{Query, Term};

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecObject,
        // ParamSpecString,
        // ParamSpecUInt,
        // ParamSpecUInt64,
        // ParamSpecDouble,
        // ParamSpecEnum
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Debug)]
    pub struct Library {
        pub sender: OnceCell<Sender<MpdMessage>>,
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
        pub albums: gio::ListStore,
        pub artists: gio::ListStore,
        pub cache: OnceCell<Rc<Cache>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Library {
        const NAME: &'static str = "EuphoniaLibrary";
        type Type = super::Library;

        fn new() -> Self {
            let albums = gio::ListStore::new::<Album>();
            let artists = gio::ListStore::new::<Artist>();
            Self {
                sender: OnceCell::new(),
                albums,
                artists,
                cache: OnceCell::new()
            }
        }
    }

    impl ObjectImpl for Library {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecObject::builder::<gio::ListStore>("albums").read_only().build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "albums" => obj.albums().to_value(),
                _ => unimplemented!(),
            }
        }

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
    pub fn setup(&self, sender: Sender<MpdMessage>, client_state: ClientState, cache: Rc<Cache>) {
        let _ = self.imp().cache.set(cache);
        let _ = self.imp().sender.set(sender);
        // Connect to ClientState signals that announce completion of requests
        client_state.connect_closure(
            "album-basic-info-downloaded",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, album: Album| {
                    this.add_album(album);
                }
            )
        );
        client_state.connect_closure(
            "artist-basic-info-downloaded",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, artist: Artist| {
                    this.add_artist(artist);
                }
            )
        );
    }

    /// Get a reference to the Album ListStore.
    pub fn albums(&self) -> gio::ListStore {
        self.imp().albums.clone()
    }

    /// Add an album to the ListStore.
    pub fn add_album(&self, album: Album) {
        let folder_uri = album.get_uri();
        if let Some(cache) = self.imp().cache.get() {
            // Might queue a download but won't load anything into memory just yet.
            cache.ensure_local_album_art(&folder_uri);
        }
        self.imp().albums.append(
            &album
        );
    }

    /// Get all the information available about an album & its contents (won't block;
    /// UI will get notified of result later if one does arrive late).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_album(&self, album: Album) {
        if settings_manager().child("client").boolean("use-lastfm") {
            if let Some(cache) = self.imp().cache.get() {
                cache.ensure_local_album_meta(
                    album.get_mbid(),
                    Some(album.get_title()),
                    album.get_artist_str().as_deref(),
                    album.get_uri().as_ref()
                );
            }
        }
        if let Some(sender) = self.imp().sender.get() {
            let _ = sender.send_blocking(MpdMessage::AlbumContent(album.get_title().to_owned()));
        }
    }

    /// Queue all songs in a given album by track order.
    pub fn queue_album(&self, album: Album, replace: bool, play: bool) {
        if let Some(sender) = self.imp().sender.get() {
            if replace {
                let _ = sender.send_blocking(MpdMessage::Clear);
            }
            let mut query = Query::new();
            query.and(Term::Tag(Cow::Borrowed("album")), album.get_title().to_owned());
            let _ = sender.send_blocking(MpdMessage::FindAdd(query));
            if replace && play {
                let _ = sender.send_blocking(MpdMessage::PlayPos(0));
            }
        }
    }

    /// Get a reference to the Artist ListStore.
    pub fn artists(&self) -> gio::ListStore {
        self.imp().artists.clone()
    }

    /// Add an artist to the ListStore.
    pub fn add_artist(&self, artist: Artist) {
        // if let Some(cache) = self.imp().cache.get() {
        //     // Might queue a download but won't load anything into memory just yet.
        //     cache.ensure_local_artist_avatar(artist.get_name());
        // }
        self.imp().artists.append(
            &artist
        );
    }

    /// Get all the information available about an artist (won't block;
    /// UI will get notified of result later via signals).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_artist(&self, artist: Artist) {
        if settings_manager().child("client").boolean("use-lastfm") {
            if let Some(cache) = self.imp().cache.get() {
                cache.ensure_local_artist_meta(
                    artist.get_mbid(),
                    artist.get_name()
                );
            }
        }
        if let Some(sender) = self.imp().sender.get() {
            // Will get both albums (Discography sub-view) and songs (All Songs sub-view)
            let _ = sender.send_blocking(MpdMessage::ArtistContent(artist.get_name().to_owned()));
        }
    }

    pub fn queue_uri(&self, uri: &str, replace: bool, play: bool) {
        if let Some(sender) = self.imp().sender.get() {
            if replace {
                let _ = sender.send_blocking(MpdMessage::Clear);
            }
            let _ = sender.send_blocking(MpdMessage::Add(uri.to_owned()));
            if replace && play {
                let _ = sender.send_blocking(MpdMessage::PlayPos(0));
            }
        }
    }
}
