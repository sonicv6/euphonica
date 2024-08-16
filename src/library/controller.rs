use std::{
    cell::{OnceCell, RefCell},
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
    cache::{
        CacheState,
        Cache
    },
    common::{
        Album,
        AlbumInfo,
        Song
    }
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use glib::{
    clone,
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
        //
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
        pub albums: RefCell<gio::ListStore>,
        pub cache: OnceCell<Rc<Cache>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Library {
        const NAME: &'static str = "EuphoniaLibrary";
        type Type = super::Library;

        fn new() -> Self {
            let albums = RefCell::new(gio::ListStore::new::<Album>());
            Self {
                sender: OnceCell::new(),
                albums,
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
        let cache_state: CacheState = cache.clone().get_cache_state();
        self.imp().cache.set(cache);
        self.imp().sender.set(sender);
        // Connect to ClientState signals that announce completion of requests
        cache_state.connect_closure(
            "album-art-downloaded",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: CacheState, folder_uri: String| {
                    this.update_album_art(&folder_uri);
                }
            )
        );

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
            "album-content-downloaded",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, album: Album, songs: glib::BoxedAnyObject| {
                    this.push_album_content_page(album, songs.borrow::<Vec<Song>>().as_ref());
                }
            )
        );
    }

    pub fn albums(&self) -> gio::ListStore {
        self.imp().albums.borrow().clone()
    }

    pub fn on_album_clicked(&self, album: Album) {
        // Used by AlbumView
        if let Some(sender) = self.imp().sender.get() {
            let _ = sender.send_blocking(MpdMessage::AlbumContent(album.get_info()));
        }
    }

    pub fn add_album(&self, album: Album) {
        let folder_uri = album.get_uri();
        if let Some(cache) = self.imp().cache.get() {
            // Might queue a download but won't load anything into memory just yet.
            cache.ensure_local_album_art(&folder_uri);
        }
        self.imp().albums.borrow().append(
            &album
        );
    }

    pub fn push_album_content_page(&self, album: Album, songs: &[Song]) {
        let song_list = gio::ListStore::new::<Song>();
        song_list.extend_from_slice(songs);
        // Try to get additional info (won't block; page will get notified of
        // result later if one does arrive late).
        if let Some(cache) = self.imp().cache.get() {
            // Might queue a download but won't load anything into memory just yet.
            cache.ensure_local_album_info(
                album.get_mb_album_id(),
                Some(album.get_title()),
                album.get_artist()
            );
        }
        self.emit_by_name::<()>(
            "album-clicked",
            &[
                &album,
                &song_list
            ]
        );
    }

    pub fn update_album_art(&self, folder_uri: &str) {
        // TODO: batch this too.
        // This fn is only for updating album art AFTER the albums have already been displayed
        // in the grid view (for example, after finishing downloading their album arts).
        // Albums whose covers have already been downloaded will not need this fn.
        // Instead, they are loaded on-demand from disk or cache by the grid view.
        // Iterate through the list store to see if we can load album art for any
        if let Some(cache) = self.imp().cache.get() {
            println!("Updating album art for {}", folder_uri);
            if let Some(tex) = cache.load_local_album_art(folder_uri, false) {
                for album in self.imp().albums.borrow().iter::<Album>().flatten() {
                    if album.get_cover().is_none() && album.get_uri() == folder_uri {
                        album.set_cover(Some(tex.clone()));
                    }
                }
            }
        }
    }

    pub fn queue_album(&self, album: Album, replace: bool, play: bool) {
        if let Some(sender) = self.imp().sender.get() {
            if replace {
                let _ = sender.send_blocking(MpdMessage::Clear);
            }
            let mut query = Query::new();
            query.and(Term::Tag(Cow::Borrowed("album")), album.get_title());
            let _ = sender.send_blocking(MpdMessage::FindAdd(query));
            if replace && play {
                let _ = sender.send_blocking(MpdMessage::PlayPos(0));
            }
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
