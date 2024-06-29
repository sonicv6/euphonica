use std::{
    cell::{Cell, RefCell},
    vec::Vec
};
extern crate mpd;
use mpd::status::{State, Status};
use crate::common::Song;
use gtk::{
    glib,
    gio,
    prelude::*,
    ListItem
};
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
        pub current_song: RefCell<Option<Song>>,
        pub queue: RefCell<gio::ListStore>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Player {
        const NAME: &'static str = "SlamprustPlayer";
        type Type = super::Player;

        fn new() -> Self {
            let queue = RefCell::new(gio::ListStore::new::<Song>());
            Self {
                state: Cell::new(State::Stop),
                position: Cell::new(0.0),
                current_song: RefCell::new(None),
                queue
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
        let old_state = self.imp().state.replace(status.state.clone());
        if old_state != status.state {
            // These properties are affected by the "state" field.
            self.notify("playing");
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
        if let Some(new_queue_place) = status.song {
            // There is now a playing song
            let maybe_old_queue_id = self.get_current_queue_id();
            if (maybe_old_queue_id.is_some() && maybe_old_queue_id.unwrap() != new_queue_place.id.0) || maybe_old_queue_id.is_none() {
                println!("Current song changed to one with ID {}", new_queue_place.id.0);
                // Either old state did not have a playing song or playing song has changed
                // Search for new song in current queue
                for maybe_song in self.queue().iter::<Song>() {
                    let song = maybe_song.unwrap();
                    let queue_id = song.get_queue_id();
                    println!("Searching queue...found ID {}", queue_id);
                    if song.get_queue_id() == new_queue_place.id.0 {
                        println!("Found it in queue!");
                        let _ = self.imp().current_song.replace(Some(song.clone()));
                        self.notify("title");
                        self.notify("artist");
                        self.notify("album");
                        self.notify("duration");
                        break;
                    }
                }
            }
        }
        else {
            // No song is playing. Update state accordingly.
            if let Some(_) = self.imp().current_song.replace(None) {
                self.notify("title");
                self.notify("artist");
                self.notify("album");
                self.notify("duration");
            }
        }
    }

    pub fn update_queue(&self, new_queue: &[mpd::song::Song]) {
        // TODO: Might want to avoid dropping the whole thing
        // TODO: Request album art for each
        // TODO: add asynchronously?
        let queue = self.imp().queue.borrow();
        queue.remove_all();
        // Convert to our internal Song GObjects then add to queue
        let songs: Vec<Song> = new_queue
                .iter()
                .map(|mpd_song| Song::from_mpd_song(mpd_song))
                .collect();
        queue.extend_from_slice(&songs);
        // Downstream widgets should now receive an item-changed signal.
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
            return song.get_duration();  // Can still be 0
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

    pub fn position(&self) -> f64 {
        self.imp().position.get()
    }

    pub fn queue(&self) -> gio::ListStore {
        self.imp().queue.borrow().clone()
    }
}

impl Default for Player {
    fn default() -> Self {
        glib::Object::new()
    }
}