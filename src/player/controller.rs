use std::cell::{Cell, RefCell};
extern crate mpd;
use mpd::status::{State, Status};
use crate::client::common::song::Song;
use gtk::glib;
use gtk::prelude::*;
use adw::subclass::prelude::*;
use async_channel::{Sender};
use crate::client::wrapper::{MpdMessage};

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecBoolean,
        // ParamSpecDouble,
        // ParamSpecObject,
        ParamSpecString,
        ParamSpecUInt,
        ParamSpecUInt64,
        ParamSpecDouble,
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Debug)]
    pub struct Player {
        pub state: Cell<State>,
        pub position: Cell<f64>,
        // TODO: Only call currentsong when detecting song change.
        // This is used for detecting song changes.
        // Only when the current song has been changed will we need to call
        // currentsong.
        // pub current_song_id: RefCell<Option<u32>>,
        // As returned by currentsong
        pub current_song: RefCell<Option<Song>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Player {
        const NAME: &'static str = "SlamprustPlayer";
        type Type = super::Player;

        fn new() -> Self {
            Self {
                state: Cell::new(State::Stop),
                position: Cell::new(0.0),
                current_song: RefCell::new(None)
            }
        }
    }

    impl ObjectImpl for Player {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::builder("playing").read_only().build(),
                    ParamSpecDouble::builder("position").read_only().build(),
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

glib::wrapper! {
    pub struct Player(ObjectSubclass<imp::Player>);
}


impl Player {
    // Update functions
    // These all have side-effects of notifying listeners of changes to the
    // GObject properties, which in turn are read from this struct's fields.
    // Signals will be sent for properties whose values have changed, even though
    // we will be receiving updates for many properties at once.

    // Main update function. MPD's protocol has a single "status" commands
    // that returns everything at once. This update function will take what's
    // relevant and update the GObject properties accordingly.
    pub fn update_status(&self, status: &Status) {
        let old_state = self.imp().state.replace(status.state.clone());
        if old_state != status.state {
            // These properties are affected by the "state" field.
            self.notify("playing");
        }

        if let Some(new_position_dur) = status.elapsed {
            let new_position = new_position_dur.as_secs_f64();
            println!("New position: {}", new_position);
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
    }

    // Song update function, corresponding to the "currentsong" command.
    pub fn update_current_song(&self, maybe_mpd_song: &Option<mpd::song::Song>) {
        if let Some(mpd_song) = maybe_mpd_song {
            let new_song = Song::from_mpd_song(&mpd_song);
            let old_song = self.imp().current_song.replace(Some(new_song));
            if
                old_song.is_none() != self.imp().current_song.borrow().is_none() ||
                old_song.as_ref().unwrap() != self.imp().current_song.borrow().as_ref().unwrap()
            {
                self.notify("title");
                self.notify("artist");
                self.notify("duration");
            }
        }
        else if self.imp().current_song.borrow().is_some() {
            let _ = self.imp().current_song.replace(None);
        }
    }

    // Here we try to define getters and setters in terms of the GObject
    // properties as defined above in mod imp {} instead of the actual
    // internal fields.
    pub fn title(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return Some(song.get_name().clone());
        }
        None
    }

    pub fn artist(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            if let Some(artist) = song.get_artist() {
                return Some(artist.clone());
            }
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
            return song.get_duration().as_secs();  // Can still be 0
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

    pub fn position(&self) -> f64 {
        self.imp().position.get()
    }

    // pub fn set_position(&self, position: u64) {
    //     self.imp().position.replace(position);
    //     self.notify("position");
    // }
}

impl Default for Player {
    fn default() -> Self {
        glib::Object::new()
    }
}