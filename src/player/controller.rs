extern crate mpd;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    vec::Vec,
    path::PathBuf
};
use async_channel::Sender;
use mpd::status::{State, Status};
use crate::{
    common::Song,
    client::albumart::{AlbumArtCache, strip_filename_linux},
    client::MpdMessage
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use gtk::gdk::Texture;
use adw::subclass::prelude::*;

#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Default)]
#[enum_type(name = "EuphoniaPlaybackState")]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecObject,
        ParamSpecString,
        ParamSpecUInt,
        ParamSpecUInt64,
        ParamSpecDouble,
        ParamSpecEnum
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Debug)]
    pub struct Player {
        pub state: Cell<PlaybackState>,
        pub position: Cell<f64>,
        pub current_song: RefCell<Option<Song>>,
        pub queue: RefCell<gio::ListStore>,
        pub albumart: RefCell<Option<Rc<AlbumArtCache>>>,
        pub sender: RefCell<Option<Sender<MpdMessage>>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Player {
        const NAME: &'static str = "EuphoniaPlayer";
        type Type = super::Player;

        fn new() -> Self {
            let queue = RefCell::new(gio::ListStore::new::<Song>());
            Self {
                state: Cell::new(PlaybackState::Stopped),
                position: Cell::new(0.0),
                current_song: RefCell::new(None),
                queue,
                albumart: RefCell::new(None),
                sender: RefCell::new(None)
            }
        }
    }

    impl ObjectImpl for Player {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecEnum::builder::<PlaybackState>("playback-state").read_only().build(),
                    ParamSpecDouble::builder("position").read_only().build(),
                    ParamSpecString::builder("title").read_only().build(),
                    ParamSpecString::builder("artist").read_only().build(),
                    ParamSpecString::builder("album").read_only().build(),
                    ParamSpecObject::builder::<Texture>("album-art").read_only().build(), // Will use high-resolution version
                    ParamSpecUInt64::builder("duration").read_only().build(),
                    ParamSpecUInt::builder("queue-id").read_only().build(),
                    // ParamSpecObject::builder::<gdk::Texture>("cover")
                    //     .read_only()
                    //     .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "playback-state" => obj.playback_state().to_value(),
                "position" => obj.position().to_value(),
                // These are proxies for Song properties
                "title" => obj.title().to_value(),
                "artist" => obj.artist().to_value(),
                "album" => obj.album().to_value(),
                "album-art" => obj.album_art().to_value(), // High-res version
                "duration" => obj.duration().to_value(),
                "queue-id" => obj.queue_id().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Player(ObjectSubclass<imp::Player>);
}

impl Default for Player {
    fn default() -> Self {
        glib::Object::new()
    }
}


impl Player {
    pub fn setup(&self, sender: Sender<MpdMessage>, albumart: Rc<AlbumArtCache>) {
        self.imp().albumart.replace(Some(albumart));
        self.imp().sender.replace(Some(sender));
    }
    // Update functions
    // These all have side-effects of notifying listeners of changes to the
    // GObject properties, which in turn are read from this struct's fields.
    // Signals will be sent for properties whose values have changed, even though
    // we will be receiving updates for many properties at once.

    // Main update function. MPD's protocol has a single "status" commands
    // that returns everything at once. This update function will take what's
    // relevant and update the GObject properties accordingly.
    fn get_current_queue_id(&self) -> Option<u32> {
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if song.is_queued() {
                return Some(song.get_queue_id());
            }
            return None
        }
        None
    }

    pub fn update_status(&self, status: &Status) {
        let new_state = match status.state {
            State::Play => PlaybackState::Playing,
            State::Pause => PlaybackState::Paused,
            State::Stop => PlaybackState::Stopped
        };
        let old_state = self.imp().state.replace(new_state);
        if old_state != new_state {
            // These properties are affected by the "state" field.
            self.notify("playback-state");
        }
        // If stopped, remove playing indicator
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if new_state == PlaybackState::Stopped {
                song.set_is_playing(false);
            }
            else {
                song.set_is_playing(true);
            }
        }

        if let Some(new_position_dur) = status.elapsed {
            let new_position = new_position_dur.as_secs_f64();
            let old_position = self.imp().position.replace(new_position);
            if old_position != new_position {
                self.notify("position");
            }
        }
        else {
            let old_position = self.imp().position.replace(0.0);
            if old_position != 0.0 {
                self.notify("position");
            }
        }

        // Queue always gets updated first before Player by idle.
        // This allows us to be sure that the new current song is already in
        // our local queue.
        // Note to self: since GObjects are boxed & reference-counted, even clearing
        // the queue will not remove the current song (yet).
        if let Some(new_queue_place) = status.song {
            // There is now a playing song
            let maybe_old_queue_id = self.get_current_queue_id();
            if (maybe_old_queue_id.is_some() && maybe_old_queue_id.unwrap() != new_queue_place.id.0) || maybe_old_queue_id.is_none() {
                // Remove playing indicator from old song
                if let Some(old_song) = self.imp().current_song.borrow().as_ref() {
                    old_song.set_is_playing(false);
                }
                // Either old state did not have a playing song or playing song has changed
                // Search for new song in current queue
                for maybe_song in self.queue().iter::<Song>() {
                    let song = maybe_song.unwrap();
                    if song.get_queue_id() == new_queue_place.id.0 {
                        let _ = self.imp().current_song.replace(Some(song.clone()));
                        self.notify("title");
                        self.notify("artist");
                        self.notify("album");
                        self.notify("album-art");
                        self.notify("duration");
                        break;
                    }
                }
                // If playing, indicate so at the new song
                if let Some(new_song) = self.imp().current_song.borrow().as_ref() {
                    if self.imp().state.get() != PlaybackState::Stopped {
                        new_song.set_is_playing(true);
                    }
                }
            }
        }
        else {
            // No song is playing. Update state accordingly.
            #[allow(clippy::redundant_pattern_matching)]
            if let Some(_) = self.imp().current_song.replace(None) {
                self.notify("title");
                self.notify("artist");
                self.notify("album");
                self.notify("album-art");
                self.notify("duration");
            }
        }
    }

    pub fn update_queue(&self, new_queue: &[mpd::song::Song]) {
        // TODO: use diffs instead of refreshing the whole queue
        let queue = self.imp().queue.borrow();
        queue.remove_all();
        // Convert to our internal Song GObjects
        let songs: Vec<Song> = new_queue
                .iter()
                .map(Song::from_mpd_song)
                .collect();
        // Ensure we have local copies of all the album arts.
        // This does not load them into memory yet. That only happens when QueueRow is displayed.
        // TODO: batch download requests
        if let Some(albumart) = self.imp().albumart.borrow().as_ref() {
            if let Some(sender) = self.imp().sender.borrow().as_ref() {
                for song in &songs {
                    let uri = &song.get_uri();
                    let folder_uri = strip_filename_linux(uri);
                    if !albumart.get_thumbnail_path_for(folder_uri).exists() {
                        let _ = sender.send_blocking(MpdMessage::AlbumArt(folder_uri.to_owned()));
                    }
                }
            }
        }
        queue.extend_from_slice(&songs);
        // Downstream widgets should now receive an item-changed signal.
    }

    pub fn update_album_art(&self, folder_uri: &str) {
        // TODO: batch this too.
        // This fn is only for updating album art AFTER the songs have already been displayed
        // in the queue (for example, after finishing downloading their album arts).
        // Songs whose album arts have already been downloaded will not need this fn.
        // Instead, their album arts are loaded on-demand from disk or cache by the queue view.
        // Iterate through the queue to see if we can load album art for any
        if let Some(albumart) = self.imp().albumart.borrow().as_ref() {
            if let Some(tex) = albumart.get_for(folder_uri, true) {
                for song in self.imp().queue.borrow().iter::<Song>().flatten() {
                    if song.get_thumbnail().is_none() && strip_filename_linux(&song.get_uri()) == folder_uri {
                        song.set_thumbnail(Some(tex.clone()));
                        // If currently playing, then also update the sidebar.
                        // Note: sidebar will use the high-resolution verison.
                        if song.is_playing() {
                            self.notify("album-art");
                        }
                    }
                }
            }
        }
    }

    // Here we try to define getters and setters in terms of the GObject
    // properties as defined above in mod imp {} instead of the actual
    // internal fields.
    pub fn title(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_name();
        }
        None
    }

    pub fn artist(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_artist();
        }
        None
    }

    pub fn album(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_album();
        }
        None
    }

    pub fn album_art(&self) -> Option<Texture> {
        // Use high-resolution version
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if let Some(albumart) = self.imp().albumart.borrow().as_ref() {
                let uri = song.get_uri();
                let folder_uri = strip_filename_linux(&uri);
                return albumart.get_for(folder_uri, false);
            }
            return None;
        }
        None
    }

    pub fn duration(&self) -> u64 {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_duration();  // Can still be 0
        }
        0
    }

    pub fn queue_id(&self) -> u32 {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_queue_id();
        }
        // Should never match a real song.
        u32::MAX
    }

    pub fn playback_state(&self) -> PlaybackState {
        self.imp().state.get()
    }

    pub fn position(&self) -> f64 {
        self.imp().position.get()
    }

    pub fn queue(&self) -> gio::ListStore {
        self.imp().queue.borrow().clone()
    }

    pub fn clear_queue(&self) {
        if let Some(sender) = self.imp().sender.borrow().as_ref() {
            let _ = sender.send_blocking(MpdMessage::Clear);
        }
    }

    pub fn on_song_clicked(&self, song: Song) {
        if let Some(sender) = self.imp().sender.borrow().as_ref() {
            let _ = sender.send_blocking(MpdMessage::PlayId(song.get_queue_id()));
        }
    }
}
