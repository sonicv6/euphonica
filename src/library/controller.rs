use std::{
    cell::OnceCell,
    rc::Rc,
    vec::Vec,
    sync::OnceLock,
    borrow::Cow
};
use async_channel::Sender;
use crate::{
    client::MpdMessage,
    cache::Cache,
    common::{
        Album,
        Artist
    }
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use glib::subclass::Signal;

use adw::subclass::prelude::*;

use mpd::{search::Operation, Query, Term};

mod imp {
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
        

        pub cache: OnceCell<Rc<Cache>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Library {
        const NAME: &'static str = "EuphoniaLibrary";
        type Type = super::Library;

        fn new() -> Self {
            Self {
                sender: OnceCell::new(),
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
    pub fn setup(&self, sender: Sender<MpdMessage>, cache: Rc<Cache>) {
        let _ = self.imp().cache.set(cache);
        let _ = self.imp().sender.set(sender);
    }

    /// Get all the information available about an album & its contents (won't block;
    /// UI will get notified of result later if one does arrive late).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_album(&self, album: &Album) {
        if let Some(cache) = self.imp().cache.get() {
            cache.ensure_cached_album_meta(album.get_info());
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

    /// Queue all songs of an artist. TODO: allow specifying order.
    pub fn queue_artist(&self, artist: Artist, use_albumartist: bool, replace: bool, play: bool) {
        if let Some(sender) = self.imp().sender.get() {
            if replace {
                let _ = sender.send_blocking(MpdMessage::Clear);
            }
            let mut query = Query::new();
            query.and_with_op(
                Term::Tag(Cow::Borrowed(
                    if use_albumartist { "albumartist" } else { "artist" }
                )),
                Operation::Contains,
                artist.get_name().to_owned()
            );
            let _ = sender.send_blocking(MpdMessage::FindAdd(query));
            if replace && play {
                let _ = sender.send_blocking(MpdMessage::PlayPos(0));
            }
        }
    }

    /// Get all the information available about an artist (won't block;
    /// UI will get notified of result later via signals).
    /// TODO: implement provider daisy-chaining on the cache side
    pub fn init_artist(&self, artist: Artist) {
        if let Some(cache) = self.imp().cache.get() {
            cache.ensure_cached_artist_meta(artist.get_info());
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
