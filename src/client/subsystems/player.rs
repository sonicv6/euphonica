use std::cell::{Cell, RefCell};

use gtk::{gdk, glib, prelude::*, subclass::prelude::*};
use crate::mpd::{
    status::{Status, State},
    song::Song
};

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecBoolean,
        // ParamSpecDouble,
        // ParamSpecObject,
        ParamSpecString,
        ParamSpecUInt64,
    };
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug)]
    pub struct PlayerState {
        pub state: Cell<State>,
        pub position: Cell<u64>,
        pub current_song: RefCell<Option<Song>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlayerState {
        const NAME: &'static str = "SlamprustPlayerState";
        type Type = super::PlayerState;

        fn new() -> Self {
            Self {
                state: Cell::new(State::Stop),
                position: Cell::new(0),
                current_song: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for PlayerState {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::builder("is_playing").read_only().build(),
                    ParamSpecUInt64::builder("position").read_only().build(),
                    ParamSpecString::builder("title").read_only().build(),
                    ParamSpecString::builder("artist").read_only().build(),
                    ParamSpecString::builder("album").read_only().build(),
                    ParamSpecUInt64::builder("duration").read_only().build(),
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
                "playing" => obj.is_playing().to_value(),
                "position" => obj.position().to_value(),

                // These are proxies for Song properties
                "title" => obj.title().to_value(),
                "artist" => obj.artist().to_value(),
                // "album" => obj.album().to_value(),
                "duration" => obj.duration().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}


// PlayerState is a GObject that we can use to bind to
// widgets and other objects; it contains the current
// state of the audio player: song metadata, playback
// position and duration, etc.
glib::wrapper! {
    pub struct PlayerState(ObjectSubclass<imp::PlayerState>);
}


impl PlayerState {
    // Update functions
    // These all have side-effects of notifying listeners of changes to the
    // GObject properties, which in turn are read from this struct's fields.

    // Main update function. MPD's protocol has a single "status" commands
    // that returns everything at once. This update function will take what's
    // relevant and update the GObject properties accordingly.
    pub fn update_status(&self, status: &Status) {
        let old_state = self.imp().state.replace(status.state.clone());
        if old_state != status.state {
            // These properties are affected by the "state" field.
            // TODO: more granular notifications
            self.notify("playing");
            self.notify("position");
        }
    }

    // Song update function, corresponding to the "currentsong" command.
    pub fn update_current_song(&self, song: &Option<Song>) {
        let old_song = self.imp().current_song.replace(song.clone());
        if old_song != *song {
            // These properties is affected by the "current_song" field.
            // TODO: more granular notifications
            self.notify("title");
            self.notify("artist");
            self.notify("duration");
        }
    }

    // Here we try to define getters and setters in terms of the GObject
    // properties as defined above in mod imp {} instead of the actual
    // internal fields.
    pub fn title(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.title.clone();
        }

        None
    }

    pub fn artist(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.artist.clone();
        }

        None
    }

    // pub fn album(&self) -> Option<String> {
    //     if let Some(song) = &*self.imp().current_song.borrow() {
    //         return song.album.clone();
    //     }

    //     None
    // }

    pub fn duration(&self) -> u64 {
        if let Some(song) = &*self.imp().current_song.borrow() {
            if let Some(duration) = song.duration {
                duration.as_secs();
            }
            return 0
        }
        0
    }

    pub fn is_playing(&self) -> bool {
        let playback_state = self.imp().state.get();
        matches!(playback_state, State::Play)
    }

    pub fn set_playback_state(&self, playback_state: &State) {
        let old_state = self.imp().state.replace(*playback_state);
        if old_state != *playback_state {
            self.notify("playing");
        }
    }

    // pub fn current_song(&self) -> Option<Song> {
    //     (*self.imp().current_song.borrow()).as_ref().cloned()
    // }

    pub fn position(&self) -> u64 {
        self.imp().position.get()
    }

    // pub fn set_position(&self, position: u64) {
    //     self.imp().position.replace(position);
    //     self.notify("position");
    // }
}

impl Default for PlayerState {
    fn default() -> Self {
        glib::Object::new()
    }
}