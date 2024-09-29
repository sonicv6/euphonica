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
use mpd::{status::{AudioFormat, State, Status}, Id, ReplayGain};
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

#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Default)]
#[enum_type(name = "EuphoniaPlaybackFlow")]
pub enum PlaybackFlow {
    #[default]
    Sequential,  // Plays through the queue once
    Repeat,      // Loops through the queue
    Single,      // Plays one song then stops & waits for click to play next song
    RepeatSingle // Loops current song
}

impl PlaybackFlow {
    pub fn from_status(st: &mpd::status::Status) -> Self {
        if st.repeat {
            if st.single {
                PlaybackFlow::RepeatSingle
            }
            else {
                PlaybackFlow::Repeat
            }
        }
        else {
            if st.single {
                PlaybackFlow::Single
            }
            else {
                PlaybackFlow::Sequential
            }
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            &PlaybackFlow::Sequential => "playlist-consecutive-symbolic",
            &PlaybackFlow::Repeat => "playlist-repeat-symbolic",
            &PlaybackFlow::Single => "stop-sign-outline-symbolic",
            &PlaybackFlow::RepeatSingle => "playlist-repeat-song-symbolic"
        }
    }

    pub fn next_in_cycle(&self) -> Self {
        match self {
            &PlaybackFlow::Sequential => PlaybackFlow::Repeat,
            &PlaybackFlow::Repeat => PlaybackFlow::Single,
            &PlaybackFlow::Single => PlaybackFlow::RepeatSingle,
            &PlaybackFlow::RepeatSingle => PlaybackFlow::Sequential
        }
    }

    // TODO: translatable
    pub fn description(&self) -> &'static str {
        match self {
            &PlaybackFlow::Sequential => "Sequential",
            &PlaybackFlow::Repeat => "Repeat Queue",
            &PlaybackFlow::Single => "Single Song",
            &PlaybackFlow::RepeatSingle => "Repeat Current Song"
        }
    }
}

fn cycle_replaygain(curr: ReplayGain) -> ReplayGain {
    match curr {
        ReplayGain::Off => ReplayGain::Auto,
        ReplayGain::Auto => ReplayGain::Track,
        ReplayGain::Track => ReplayGain::Album,
        ReplayGain::Album => ReplayGain::Off
    }
}

fn get_replaygain_icon_name(mode: ReplayGain) -> &'static str {
    match mode {
        ReplayGain::Off => "rg-off-symbolic",
        ReplayGain::Auto => "rg-auto-symbolic",
        ReplayGain::Track => "rg-track-symbolic",
        ReplayGain::Album => "rg-album-symbolic"
    }
}

mod imp {
    use super::*;
    use glib::{
        ParamSpec, ParamSpecBoolean, ParamSpecDouble, ParamSpecEnum, ParamSpecFloat, ParamSpecObject, ParamSpecString, ParamSpecUInt, ParamSpecUInt64
    };
    use once_cell::sync::Lazy;

    pub struct Player {
        pub state: Cell<PlaybackState>,
        pub position: Cell<f64>,
        pub current_song: RefCell<Option<Song>>,
        pub queue: gio::ListStore,
        pub format: RefCell<Option<AudioFormat>>,
        pub flow: Cell<PlaybackFlow>,
        pub random: Cell<bool>,
        pub consume: Cell<bool>,
        pub replaygain: Cell<ReplayGain>,
        pub crossfade: Cell<f64>,
        pub mixramp_db: Cell<f32>,
        pub mixramp_delay: Cell<f64>,
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
        // Handle to seekbar polling task
        pub poller_handle: RefCell<Option<glib::JoinHandle<()>>>,
        // Set this to true to pause polling even if PlaybackState is Playing.
        // Used by seekbar widgets.
        pub poll_blocked: Cell<bool>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Player {
        const NAME: &'static str = "EuphoniaPlayer";
        type Type = super::Player;

        fn new() -> Self {
            Self {
                state: Cell::new(PlaybackState::Stopped),
                position: Cell::new(0.0),
                random: Cell::new(false),
                consume: Cell::new(false),
                replaygain: Cell::new(ReplayGain::Off),
                crossfade: Cell::new(0.0),
                mixramp_db: Cell::new(0.0),
                mixramp_delay: Cell::new(0.0),
                current_song: RefCell::new(None),
                queue: gio::ListStore::new::<Song>(),
                format: RefCell::new(None),
                flow: Cell::default(),
                client_sender: OnceCell::new(),
                cache: OnceCell::new(),
                volume: Cell::new(0),
                poller_handle: RefCell::new(None),
                poll_blocked: Cell::new(false)
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
                    ParamSpecEnum::builder::<PlaybackFlow>("playback-flow")
                        .read_only()
                        .build(),
                    ParamSpecString::builder("replaygain").read_only().build(), // use icon name directly to simplify implementation
                    ParamSpecDouble::builder("crossfade").build(), // seconds
                    ParamSpecFloat::builder("mixramp-db").build(),
                    ParamSpecDouble::builder("mixramp-delay").build(), // seconds
                    ParamSpecBoolean::builder("random").build(),
                    ParamSpecBoolean::builder("consume").build(),
                    ParamSpecDouble::builder("position").build(),
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
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "playback-state" => self.state.get().to_value(),
                "playback-flow" => self.flow.get().to_value(),
                "random" => self.random.get().to_value(),
                "consume" => self.consume.get().to_value(),
                "crossfade" => self.crossfade.get().to_value(),
                "mixramp-db" => self.mixramp_db.get().to_value(),
                "mixramp-delay" => self.mixramp_delay.get().to_value(),
                "replaygain" => get_replaygain_icon_name(self.replaygain.get()).to_value(),
                "position" => obj.position().to_value(),
                // These are proxies for Song properties
                "title" => obj.title().to_value(),
                "artist" => obj.artist().to_value(),
                "album" => obj.album().to_value(),
                "album-art" => obj.current_song_album_art(false).to_value(), // High-res version
                "duration" => obj.duration().to_value(),
                "queue-id" => obj.queue_id().to_value(),
                "quality-grade" => obj.quality_grade().to_value(),
                "format-desc" => obj.format_desc().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "crossfade" => {
                    if let Ok(v) = value.get::<f64>() {
                        obj.set_crossfade(v);
                    }
                },
                "mixramp-db" => {
                    if let Ok(v) = value.get::<f32>() {
                        obj.set_mixramp_db(v);
                    }
                },
                "mixramp-delay" => {
                    if let Ok(v) = value.get::<f64>() {
                        obj.set_mixramp_delay(v);
                    }
                },
                "position" => {
                    if let Ok(v) = value.get::<f64>() {
                        obj.set_position(v);
                    }
                },
                "random" => {
                    if let Ok(state) = value.get::<bool>() {
                        obj.set_random(state);
                        // Don't actually set the property here yet.
                        // Idle status will update it later.
                    }
                }
                "consume" => {
                    if let Ok(state) = value.get::<bool>() {
                        obj.set_consume(state);
                        // Don't actually set the property here yet.
                        // Idle status will update it later.
                    }
                }
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
        if new_state == PlaybackState::Playing {
            self.maybe_start_polling();
        }

        let old_state = self.imp().state.replace(new_state);
        if old_state != new_state {
            // These properties are affected by the "state" field.
            self.notify("playback-state");
        }

        let new_rg = status.replaygain.unwrap_or(ReplayGain::Off);
        let old_rg = self.imp().replaygain.replace(new_rg);
        if old_rg != new_rg {
            // These properties are affected by the "state" field.
            self.notify("replaygain");
        }

        let new_flow = PlaybackFlow::from_status(status);
        let old_flow = self.imp().flow.replace(new_flow);
        if old_flow != new_flow {
            self.notify("playback-flow");
        }

        let old_rand = self.imp().random.replace(status.random);
        if old_rand != status.random {
            self.notify("random");
        }

        let old_consume = self.imp().consume.replace(status.consume);
        if old_consume != status.consume {
            self.notify("consume");
        }

        let old_format = self.imp().format.replace(status.audio);
        if old_format != status.audio {
            self.notify("format-desc");
        }

        let old_mixramp_db = self.imp().mixramp_db.replace(status.mixrampdb);
        if old_mixramp_db != status.mixrampdb {
            self.notify("mixramp-db");
        }

        let new_mixramp_delay: f64;
        if let Some(dur) = status.mixrampdelay {
            new_mixramp_delay = dur.as_secs_f64();
        }
        else {
            new_mixramp_delay = 0.0;
        }
        let old_mixramp_delay = self.imp().mixramp_delay.replace(new_mixramp_delay);
        if old_mixramp_delay != new_mixramp_delay {
            self.notify("mixramp-delay");
        }

        let new_crossfade: f64;
        if let Some(dur) = status.crossfade {
            new_crossfade = dur.as_secs_f64();
        }
        else {
            new_crossfade = 0.0;
        }
        let old_crossfade = self.imp().crossfade.replace(new_crossfade);
        if old_crossfade != new_crossfade {
            self.notify("crossfade");
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
            self.set_position(new_position_dur.as_secs_f64());
        } else {
            self.set_position(0.0);
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
        let queue = &self.imp().queue;
        queue.remove_all();
        queue.extend_from_slice(songs);
        if let Some(cache) = self.imp().cache.get() {
            let infos: Vec<&AlbumInfo> = songs
                .into_iter()
                .map(|song| song.get_album())
                .filter(|ao| ao.is_some())
                .map(|info| info.unwrap())
                .collect();
            // Might queue downloads, depending on user settings
            cache.ensure_cached_album_arts(&infos);
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
    pub fn cycle_playback_flow(&self) {
        if let Some(sender) = self.imp().client_sender.get() {
            let next_flow = self.imp().flow.get().next_in_cycle();
            let _ = sender.send_blocking(MpdMessage::SetPlaybackFlow(next_flow));
        }
    }

    pub fn cycle_replaygain(&self) {
        if let Some(sender) = self.imp().client_sender.get() {
            let next_rg = cycle_replaygain(self.imp().replaygain.get());
            let _ = sender.send_blocking(MpdMessage::ReplayGain(next_rg));
        }
    }

    pub fn set_random(&self, new: bool) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Random(new));
        }
    }

    pub fn set_consume(&self, new: bool) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Consume(new));
        }
    }

    pub fn set_crossfade(&self, new: f64) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Crossfade(new));
        }
    }

    pub fn set_mixramp_db(&self, new: f32) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::MixRampDb(new));
        }
    }

    pub fn set_mixramp_delay(&self, new: f64) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::MixRampDelay(new));
        }
    }

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

    pub fn current_song_album_art(&self, thumbnail: bool) -> Option<Texture> {
        // Use high-resolution version
        if let Some(song) = self.imp().current_song.borrow().as_ref() {
            if let Some(cache) = self.imp().cache.get() {
                if let Some(album) = song.get_album() {
                    // Should have been scheduled by queue updates
                    return cache.load_cached_album_art(album, thumbnail, false);
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

    pub fn position(&self) -> f64 {
        self.imp().position.get()
    }

    /// Set new position. Only sets the property (does not send a seek command to MPD yet).
    /// To apply this new position, call seek().
    pub fn set_position(&self, new: f64) {
        let old = self.imp().position.replace(new);
        if new != old {
            self.notify("position");
        }
    }

    /// Seek to current position. Called when the seekbar is released.
    pub fn seek(&self) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::SeekCur(self.position()));
        }
    }

    pub fn queue(&self) -> gio::ListStore {
        self.imp().queue.clone()
    }

    pub fn toggle_playback(&self) {
        // If state is stopped, there won't be a "current song".
        // To start playing, instead of using the "pause" command to toggle,
        // we need to explicitly tell MPD to start playing the first song in
        // the queue.

        match self.imp().state.get() {
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

    pub fn prev_song(&self) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Prev);
        }
    }

    pub fn next_song(&self) {
        if let Some(sender) = self.imp().client_sender.get() {
            let _ = sender.send_blocking(MpdMessage::Next);
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

    pub fn remove_song_id(&self, id: u32) {
        if let Err(msg) = self.send(MpdMessage::DeleteId(id)) {
            println!("{}", msg);
        }
    }

    /// Periodically poll for player progress to update seekbar.
    /// Won't start a new loop if there is already one or when polling is blocked by a seekbar.
    pub fn maybe_start_polling(&self) {
        let this = self.clone();
        let sender = self.imp().client_sender.get().expect("Fatal: Player controller not set up").clone();
        if self.imp().poller_handle.borrow().is_none() && !self.imp().poll_blocked.get() {
            let poller_handle = glib::MainContext::default().spawn_local(async move {
                loop {
                    // Don't poll if not playing
                    if this.imp().state.get() != PlaybackState::Playing {
                        break;
                    }
                    // Skip poll if channel is full
                    if !sender.is_full() {
                        let _ = sender.send_blocking(MpdMessage::Status);
                    }
                    glib::timeout_future_seconds(1).await;
                }
            });
            self.imp().poller_handle.replace(Some(poller_handle));
        }
        else if self.imp().poll_blocked.get() {
            println!("Polling blocked");
        }
    }

    /// Stop poller loop. Seekbar should call this when being interacted with.
    pub fn stop_polling(&self) {
        if let Some(handle) = self.imp().poller_handle.take() {
            handle.abort();
        }
    }

    pub fn block_polling(&self) {
        let _ = self.imp().poll_blocked.replace(true);
    }

    pub fn unblock_polling(&self) {
        let _ = self.imp().poll_blocked.replace(false);
    }
}
