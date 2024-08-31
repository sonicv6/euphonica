extern crate mpd;
use crate::{
    cache::Cache,
    client::{ClientState, MpdMessage},
    common::{AlbumInfo, QualityGrade, Song},
    utils::prettify_audio_format,
};
use adw::subclass::prelude::*;
use async_channel::Sender;
use glib::{closure_local, subclass::Signal, BoxedAnyObject};
use gtk::gdk::Texture;
use gtk::{gio, glib, prelude::*};
use mpd::status::{AudioFormat, State, Status};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
    sync::OnceLock,
    vec::Vec,
};

#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Default)]
#[enum_type(name = "EuphoniaPlaybackState")]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

mod imp {
    use super::*;
    use glib::{
        ParamSpec, ParamSpecDouble, ParamSpecEnum, ParamSpecObject, ParamSpecString, ParamSpecUInt,
        ParamSpecUInt64,
    };
    use once_cell::sync::Lazy;

    pub struct Player {
        pub state: Cell<PlaybackState>,
        pub position: Cell<f64>,
        pub current_song: RefCell<Option<Song>>,
        pub queue: RefCell<gio::ListStore>,
        pub format: RefCell<Option<AudioFormat>>,
        // Rounded version, for sending to MPD.
        // Changes not big enough to cause an integer change
        // will not be sent to MPD.
        pub volume: Cell<i8>,
        pub client_sender: OnceCell<Sender<MpdMessage>>,
        // Direct reference to the cache object for fast path to
        // album arts (else we'd have to wait for signals, then
        // loop through the whole queue & search for songs matching
        // that album URI to update their arts).
        pub cache: OnceCell<Rc<Cache>>,
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
                format: RefCell::new(None),
                client_sender: OnceCell::new(),
                cache: OnceCell::new(),
                volume: Cell::new(0),
            }
        }
    }

    impl ObjectImpl for Player {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecEnum::builder::<PlaybackState>("playback-state")
                        .read_only()
                        .build(),
                    ParamSpecDouble::builder("position").read_only().build(),
                    ParamSpecString::builder("title").read_only().build(),
                    ParamSpecString::builder("artist").read_only().build(),
                    ParamSpecString::builder("album").read_only().build(),
                    ParamSpecObject::builder::<Texture>("album-art")
                        .read_only()
                        .build(), // Will use high-resolution version
                    ParamSpecUInt64::builder("duration").read_only().build(),
                    ParamSpecUInt::builder("queue-id").read_only().build(),
                    ParamSpecEnum::builder::<QualityGrade>("quality-grade")
                        .read_only()
                        .build(),
                    ParamSpecString::builder("format-desc").read_only().build(),
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
                "album-art" => obj.current_song_album_art().to_value(), // High-res version
                "duration" => obj.duration().to_value(),
                "queue-id" => obj.queue_id().to_value(),
                "quality-grade" => obj.quality_grade().to_value(),
                "format-desc" => obj.format_desc().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("outputs-changed")
                        .param_types([BoxedAnyObject::static_type()])
                        .build(),
                    // Reserved for EXTERNAL changes (i.e. changes made by this client won't
                    // emit this).
                    Signal::builder("volume-changed")
                        .param_types([i8::static_type()])
                        .build(),
                ]
            })
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
    pub fn setup(
        &self,
        client_sender: Sender<MpdMessage>,
        client_state: ClientState,
        cache: Rc<Cache>,
    ) {
        let _ = self.imp().client_sender.set(client_sender);
        let _ = self.imp().cache.set(cache);
        // Connect to ClientState signals that announce completion of requests
        client_state.connect_closure(
            "status-changed",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, boxed: BoxedAnyObject| {
                    this.update_status(&boxed.borrow());
                }
            ),
        );
        client_state.connect_closure(
            "queue-changed",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, boxed: BoxedAnyObject| {
                    this.update_queue(boxed.borrow::<Vec<Song>>().as_ref());
                }
            ),
        );
        client_state.connect_closure(
            "outputs-changed",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |_: ClientState, boxed: BoxedAnyObject| {
                    // Forward to bar
                    this.update_outputs(boxed);
                }
            ),
        );
    }

    // Utility functions
    fn send(&self, msg: MpdMessage) -> Result<(), &str> {
        if let Some(sender) = self.imp().client_sender.get() {
            let res = sender.send_blocking(msg);
            if res.is_err() {
                return Err("Sender error");
            }
            return Ok(());
        }
        Err("Could not borrow sender")
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
            return None;
        }
        None
    }

    pub fn update_status(&self, status: &Status) {
        let new_state = match status.state {
            State::Play => PlaybackState::Playing,
            State::Pause => PlaybackState::Paused,
            State::Stop => PlaybackState::Stopped,
        };

        let old_state = self.imp().state.replace(new_state);
        if old_state != new_state {
            // These properties are affected by the "state" field.
            self.notify("playback-state");
        }

        let old_format = self.imp().format.replace(status.audio);
        if old_format != status.audio {
            self.notify("format-desc");
        }

        // If stopped, remove playing indicator
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if new_state == PlaybackState::Stopped {
                song.set_is_playing(false);
            } else {
                song.set_is_playing(true);
            }
        }

        if let Some(new_position_dur) = status.elapsed {
            let new_position = new_position_dur.as_secs_f64();
            let old_position = self.imp().position.replace(new_position);
            if old_position != new_position {
                self.notify("position");
            }
        } else {
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
            if (maybe_old_queue_id.is_some() && maybe_old_queue_id.unwrap() != new_queue_place.id.0)
                || maybe_old_queue_id.is_none()
            {
                // Remove playing indicator from old song
                if let Some(old_song) = self.imp().current_song.borrow().as_ref() {
                    old_song.set_is_playing(false);
                }
                // Either old state did not have a playing song or playing song has changed
                // Search for new song in current queue
                for maybe_song in self.queue().iter::<Song>() {
                    let song = maybe_song.unwrap();
                    if song.get_queue_id() == new_queue_place.id.0 {
                        let maybe_old_song = self.imp().current_song.replace(Some(song.clone()));
                        self.notify("title");
                        self.notify("artist");
                        self.notify("duration");
                        self.notify("quality-grade");
                        self.notify("format-desc");
                        // Avoid needlessly changing album art as it will cause the whole
                        // bar to redraw (blurred background).
                        if let Some(old_song) = maybe_old_song {
                            if song.get_album() != old_song.get_album() {
                                self.notify("album");
                                self.notify("album-art");
                            }
                        } else {
                            self.notify("album");
                            self.notify("album-art");
                        }
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
        } else {
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

        // Handle volume changes (might be external)
        // TODO: Find a way to somewhat responsively update volume to external
        // changes at all times rather than relying on the seekbar poller.
        let new_vol = status.volume;
        let old_vol = self.imp().volume.replace(new_vol);
        if old_vol != new_vol {
            self.emit_by_name::<()>("volume-changed", &[&new_vol]);
        }
    }

    pub fn update_queue(&self, songs: &[Song]) {
        // TODO: use diffs instead of refreshing the whole queue
        let queue = self.imp().queue.borrow();
        queue.remove_all();
        queue.extend_from_slice(songs);
        if let Some(cache) = self.imp().cache.get() {
            let infos: Vec<&AlbumInfo> = songs
            .into_iter()
            .map(|song| song.get_album())
            .filter(|ao| ao.is_some())
            .map(|info| info.unwrap())
            .collect();
            // Might queue downloads, depending on user settings, but will not
            // actually load anything into memory just yet.
            cache.ensure_local_album_arts(&infos);
        }
        // Downstream widgets should now receive an item-changed signal.
    }

    fn update_outputs(&self, outputs: BoxedAnyObject) {
        self.emit_by_name::<()>("outputs-changed", &[&outputs]);
    }

    pub fn set_output(&self, id: u32, state: bool) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Output(id, state));
        }
    }

    // Here we try to define getters and setters in terms of the GObject
    // properties as defined above in mod imp {} instead of the actual
    // internal fields.
    pub fn title(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return Some(song.get_name().to_owned());
        }
        None
    }

    pub fn artist(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_artist_str();
        }
        None
    }

    pub fn album(&self) -> Option<String> {
        if let Some(song) = &*self.imp().current_song.borrow() {
            if let Some(album) = song.get_album() {
                return Some(album.title.clone());
            }
            return None;
        }
        None
    }

    pub fn current_song_album_art(&self) -> Option<Texture> {
        // Use high-resolution version
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if let Some(cache) = self.imp().cache.get() {
                if let Some(album) = song.get_album() {
                    return cache.load_local_album_art(album, false);
                }
            }
            return None;
        }
        None
    }

    pub fn quality_grade(&self) -> QualityGrade {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_quality_grade();
        }
        QualityGrade::Unknown
    }

    pub fn format_desc(&self) -> Option<String> {
        if let Some(format) = &*self.imp().format.borrow() {
            return Some(prettify_audio_format(format));
        }
        None
    }

    pub fn duration(&self) -> u64 {
        if let Some(song) = &*self.imp().current_song.borrow() {
            return song.get_duration(); // Can still be 0
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

    pub fn toggle_playback(&self) {
        // If state is stopped, there won't be a "current song".
        // To start playing, instead of using the "pause" command to toggle,
        // we need to explicitly tell MPD to start playing the first song in
        // the queue.

        match self.playback_state() {
            PlaybackState::Stopped => {
                // Check if queue is not empty
                if self.queue().n_items() > 0 {
                    // Start playing first song in queue.
                    if let Err(msg) = self.send(MpdMessage::PlayPos(0)) {
                        println!("{}", msg);
                    }
                } else {
                    println!("Queue is empty; nothing to play");
                }
            }
            PlaybackState::Playing => {
                if let Err(msg) = self.send(MpdMessage::Pause) {
                    println!("{}", msg);
                }
            }
            PlaybackState::Paused => {
                if let Err(msg) = self.send(MpdMessage::Play) {
                    println!("{}", msg);
                }
            }
        }
    }

    pub fn clear_queue(&self) {
        if let Err(msg) = self.send(MpdMessage::Clear) {
            println!("{}", msg);
        }
    }

    pub fn set_volume(&self, val: i8) {
        let old_vol = self.imp().volume.replace(val);
        if old_vol != val {
            if let Err(msg) = self.send(MpdMessage::Volume(val)) {
                println!("{}", msg);
            }
        }
    }

    pub fn on_song_clicked(&self, song: Song) {
        if let Err(msg) = self.send(MpdMessage::PlayId(song.get_queue_id())) {
            println!("{}", msg);
        }
    }
}
