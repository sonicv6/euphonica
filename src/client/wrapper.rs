use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    path::PathBuf
};
use rustc_hash::FxHashSet;
use gtk::{gio::prelude::*, glib::BoxedAnyObject};
use futures::executor;
use async_channel::{Sender, Receiver, SendError};
use glib::clone;
use gtk::{glib, gio};
use mpd::{
    client::Client, error::Error, search::{Operation as QueryOperation, Query, Term, Window}, song::Id, Channel, Idle, Subsystem
};
use image::DynamicImage;
use uuid::Uuid;

use crate::{
    common::{Album, AlbumInfo, Artist, ArtistInfo, Song, SongInfo}, meta_providers::Metadata, player::PlaybackFlow, utils
};

use super::state::{ClientState, ConnectionState};

pub fn get_dummy_song(uri: &str) -> mpd::Song {
    // Many of mpd's methods require an impl of trait ToSongPath, which
    // - Is not made public,
    // - Is only implemented by their Song struct, and
    // - Is only for getting the URI anyway.
    mpd::Song {
        file: uri.to_owned(),
        name: None,
        title: None,
        last_mod: None,
        artist: None,
        duration: None,
        place: None,
        range: None,
        tags: Vec::new()
    }
}

// One for each command in mpd's protocol plus a few special ones such
// as Connect and Toggle.
pub enum MpdMessage {
    Connect, // Host and port are always read from gsettings
    Update, // Update DB
    Output(u32, bool), // Set output state. Specify target ID and state to set to.
    SetPlaybackFlow(PlaybackFlow),
    Random(bool),
    Play,
    Pause,
    Add(String), // Add by URI
    PlayPos(u32), // Play song at queue position
    PlayId(u32), // Play song at queue ID
    DeleteId(u32),
    Clear, // Clear queue
    Prev,
    Next,
    Status,
    SeekCur(f64), // Seek current song to last position set by PrepareSeekCur. For some reason the mpd crate calls this "rewind".
    FindAdd(Query<'static>),
    Queue, // Get songs in current queue
    Albums, // Get albums. Will return one by one
    Artists(bool), // Get artists. Will return one by one. If bool flag is true, will parse AlbumArtist tag.
    AlbumContent(String), // Get list of songs with given album tag
    ArtistContent(String), // Get songs and albums of artist with given name
    Volume(i8),
    MixRampDb(f32),
    MixRampDelay(f64),
    Crossfade(f64),
    ReplayGain(mpd::status::ReplayGain),
    Consume(bool),
    GetSticker(String, String, String), // Type, URI, name
    SetSticker(String, String, String, String), // Type, URI, name, value

    // Reserved for cache controller
    // folder-level URI, key doc & paths to write the hires & thumbnail versions
    // Key doc is here so we can query fetching from remote sources with the cache controller in case MPD can't
    // give us an album art.
    AlbumArt(String, bson::Document, PathBuf, PathBuf),

	// Reserved for child thread
	Busy(bool), // A true will be sent when the work queue starts having tasks, and a false when it is empty again.
	Idle(Vec<Subsystem>), // Will only be sent from the child thread
    // Return downloaded & resized album arts (hires and thumbnail respectively)
	AlbumArtDownloaded(String, DynamicImage, DynamicImage),
    AlbumArtNotAvailable(String), // For triggering downloading from other sources
    AlbumBasicInfoDownloaded(AlbumInfo), // Return new album to be added to the list model.
    AlbumSongInfoDownloaded(String, Vec<SongInfo>), // Return songs in the album with the given tag (batched)
    ArtistBasicInfoDownloaded(ArtistInfo), // Return new artist to be added to the list model.
    ArtistSongInfoDownloaded(String, Vec<SongInfo>),  // Return songs of an artist (or had their participation)
    ArtistAlbumBasicInfoDownloaded(String, AlbumInfo),  // Return albums that had this artist in their AlbumArtist tag.
    DBUpdated
}

// Work requests for sending to the child thread.
// Completed results will be reported back via MpdMessage.
#[derive(Debug)]
enum BackgroundTask {
    Update,
    DownloadAlbumArt(String, bson::Document, PathBuf, PathBuf),  // folder-level URI
    FetchAlbums,  // Gradually get all albums
    FetchAlbumSongs(String),  // Get songs of album with given tag
    FetchArtists(bool),  // Gradually get all artists. If bool flag is true, will parse AlbumArtist tag
    FetchArtistSongs(String),  // Get all songs of an artist with given name
    FetchArtistAlbums(String),  // Get all albums of an artist with given name
}

// Thin wrapper around the blocking mpd::Client. It contains two separate client
// objects connected to the same address. One lives on the main thread along
// with the GUI and takes care of sending user commands to the daemon, while the
// other lives on on a child thread and is often in idle mode in order to
// receive all server-side changes, including those resulting from commands from
// other clients, such as MPRIS controls in the notification centre or another
// frontend. Note that this second client will not notify the main thread on
// seekbar progress. That will have to be polled by the main thread.

// Heavy operations such as streaming lots of album arts from a remote server
// should be performed by the background child client, which will receive them
// through an unbounded async_channel serving as a work queue. On each loop,
// the child client checks whether there's anything to handle in the work queue.
// If there is, it will take & handle one item. If the queue is instead empty, it
// will go into idle() mode.

// Once in the idle mode, the child client is blocked and thus cannot check the
// work queue. As such, after inserting a work item into the queue, the main
// thread will also post a message to an mpd inter-client channel also listened
// to by the child client. This will trigger an idle notification for the Message
// subsystem, allowing the child client to break out of the blocking idle.

// The child thread never modifies the main state directly. It instead sends
// messages containing a list of subsystems with updated states to the main thread
// via a bounded async_channel. The main thread receives these messages in an async
// loop, contacts the daemon again to get information for each of the changed
// subsystems, then update the relevant state objects accordingly, saving us the
// trouble of putting state objects behind mutexes.

// Reconnection is a bit convoluted. There is no way to abort the child thread
// from the main one, but we can make the child thread check a flag before idling.
// The child thread will only be able to do so after finishing idling, but
// incidentally, disconnecting the main thread's client will send an idle message,
// unblocking the child thread and allowing it to check the flag.

mod background {
    use super::*;
    pub fn update_mpd_database(client: &mut mpd::Client, sender_to_fg: &Sender<MpdMessage>) {
        if let Ok(_) = client.update() {
            let _ = sender_to_fg.send_blocking(MpdMessage::DBUpdated);
        }
    }

    pub fn download_album_art(
        client: &mut mpd::Client,
        sender_to_cache: &Sender<Metadata>,
        uri: String,
        key: bson::Document,
        path: PathBuf,
        thumbnail_path: PathBuf
    ) {
        if let Ok(bytes) = client.albumart(&get_dummy_song(&uri)) {
            println!("Downloaded album art for {:?}", uri);
            if let Some(dyn_img) = utils::read_image_from_bytes(bytes) {
                let (hires, thumb) = utils::resize_image(dyn_img);
                if !path.exists() || !thumbnail_path.exists() {
                    if let (Ok(_), Ok(_)) = (
                        hires.save(path),
                        thumb.save(thumbnail_path)
                    ) {
                        sender_to_cache.send_blocking(Metadata::AlbumArt(uri, false)).expect(
                            "Cannot notify main cache of album art download result."
                        );
                    }
                }
            }
        }
        else {
            // Fetch from local sources instead.
            sender_to_cache.send_blocking(Metadata::AlbumArtNotAvailable(uri, key)).expect(
                "Album art not available from MPD, but cannot notify cache of this."
            );
        }
    }

    fn fetch_albums_by_query<F>(
        client: &mut mpd::Client,
        query: &Query,
        respond: F
    ) where
        F: Fn(AlbumInfo) -> Result<(), SendError<MpdMessage>>
    {
        // TODO: batched windowed retrieval
        // Get list of unique album tags
        // Will block child thread until info for all albums have been retrieved.
        if let Ok(tag_list) = client
            .list(&Term::Tag(Cow::Borrowed("album")), query) {
            for tag in &tag_list {
                if let Ok(mut songs) = client.find(
                    Query::new()
                        .and(Term::Tag(Cow::Borrowed("album")), tag),
                    Window::from((0, 1))
                ) {
                    if !songs.is_empty() {
                        let info = SongInfo::from(std::mem::take(&mut songs[0]))
                            .into_album_info()
                            .unwrap_or_default();
                        let _ = respond(info);
                    }
                }
            }
        }
    }

    fn fetch_songs_by_query<F>(
        client: &mut mpd::Client,
        query: &Query,
        respond: F
    ) where
        F: Fn(Vec<SongInfo>) -> Result<(), SendError<MpdMessage>>
    {
        // TODO: batched windowed retrieval
        let songs: Vec<SongInfo> = client
            .find(query, Window::from((0, 4096)))
            .unwrap()
            .iter_mut()
            .map(|mpd_song| {
                SongInfo::from(std::mem::take(mpd_song))
            })
            .collect();
        if !songs.is_empty() {
            let _ = respond(songs);
        }
    }

    pub fn fetch_all_albums(
        client: &mut mpd::Client,
        sender_to_fg: &Sender<MpdMessage>
    ) {
        fetch_albums_by_query(
            client,
            &Query::new(),
            |info| {
                sender_to_fg.send_blocking(
                    MpdMessage::AlbumBasicInfoDownloaded(
                        info
                    )
                )
            }
        );
    }

    pub fn fetch_albums_of_artist(
        client: &mut mpd::Client,
        sender_to_fg: &Sender<MpdMessage>,
        artist_name: String,
    ) {
        fetch_albums_by_query(
            client,
            Query::new().and_with_op(
                Term::Tag(Cow::Borrowed("artist")),
                QueryOperation::Contains,
                artist_name.clone()
            ),
            |info| {
                sender_to_fg.send_blocking(
                    MpdMessage::ArtistAlbumBasicInfoDownloaded(
                        artist_name.clone(),
                        info
                    )
                )
            }
        );
    }

    pub fn fetch_album_songs(
        client: &mut mpd::Client,
        sender_to_fg: &Sender<MpdMessage>,
        tag: String
    ) {
        fetch_songs_by_query(
            client,
            Query::new().and(Term::Tag(Cow::Borrowed("album")), tag.clone()),
            |songs| {
                sender_to_fg.send_blocking(
                    MpdMessage::AlbumSongInfoDownloaded(
                        tag.clone(),
                        songs
                    )
                )
            }
        );
    }

    pub fn fetch_artists(
        client: &mut mpd::Client,
        sender_to_fg: &Sender<MpdMessage>,
        use_album_artist: bool
    ) {
        // Fetching artists is a bit more involved: artist tags usually contain multiple artists.
        // For the same reason, one artist can appear in multiple tags.
        // Here we'll reuse the artist parsing code in our SongInfo struct and put parsed
        // ArtistInfos in a Set to deduplicate them.
        let tag_type: &'static str = if use_album_artist {
            "albumartist"
        } else {
            "artist"
        };
        let mut already_parsed: FxHashSet<String> = FxHashSet::default();
        if let Ok(tag_list) = client.list(&Term::Tag(Cow::Borrowed(tag_type)), &Query::new()) {
            // TODO: Limit tags to only what we need locally
            for tag in &tag_list {
                if let Ok(mut songs) = client.find(
                    Query::new()
                        .and(Term::Tag(Cow::Borrowed(tag_type)), tag),
                    Window::from((0, 1))
                ) {
                    if !songs.is_empty() {
                        let first_song = SongInfo::from(std::mem::take(&mut songs[0]));
                        let artists = first_song.into_artist_infos();
                        // println!("Got these artists: {artists:?}");
                        for artist in artists.into_iter() {
                            if already_parsed.insert(artist.name.clone()) {
                                // println!("Never seen {artist:?} before, inserting...");
                                let _ = sender_to_fg.send_blocking(
                                    MpdMessage::ArtistBasicInfoDownloaded(
                                        artist
                                    )
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn fetch_songs_of_artist(
        client: &mut mpd::Client,
        sender_to_fg: &Sender<MpdMessage>,
        name: String
    ) {
        fetch_songs_by_query(
            client,
            Query::new()
                .and_with_op(
                    Term::Tag(Cow::Borrowed("artist")),
                    QueryOperation::Contains,
                    name.clone()
                ),
            |songs| {
                sender_to_fg.send_blocking(
                    MpdMessage::ArtistSongInfoDownloaded(
                        name.clone(),
                        songs
                    )
                )
            }
        );
    }
}

#[derive(Debug)]
pub struct MpdWrapper {
    // Corresponding sender, for cloning into child thread.
    sender: Sender<MpdMessage>,
    // The main client living on the main thread. Every single method of
    // mpd::Client is mutating so we'll just rely on a RefCell for now.
    main_client: RefCell<Option<Client>>,
    // The state GObject, used for communicating client status & changes to UI elements
    state: ClientState,
    // Handle to the child thread.
    bg_handle: RefCell<Option<gio::JoinHandle<()>>>,
    bg_channel: Channel, // For waking up the child client
    bg_sender: RefCell<Option<Sender<BackgroundTask>>>, // For sending tasks to background thread
    meta_sender: Sender<Metadata> // For sending album arts to cache controller
}

impl MpdWrapper {
    pub fn new(meta_sender: Sender<Metadata>) -> Rc<Self> {
        // Set up channels for communication with client object
        let (
            sender,
            receiver
        ): (Sender<MpdMessage>, Receiver<MpdMessage>) = async_channel::unbounded();
        let ch_name = Uuid::new_v4().simple().to_string();
        println!("Channel name: {}", &ch_name);
        let wrapper = Rc::new(Self {
            sender,
            state: ClientState::default(),
            main_client: RefCell::new(None),  // Must be initialised later
            bg_handle: RefCell::new(None),  // Will be spawned later
            bg_channel: Channel::new(&ch_name).unwrap(),
            bg_sender: RefCell::new(None),
            meta_sender
        });

        // For future noob self: these are shallow
        wrapper.clone().setup_channel(receiver);
        wrapper
    }

    pub fn get_sender(self: Rc<Self>) -> Sender<MpdMessage> {
        self.sender.clone()
    }

    pub fn get_client_state(self: Rc<Self>) -> ClientState {
        self.state.clone()
    }

    fn start_bg_thread(self: Rc<Self>, addr: &str) {
        let sender_to_fg = self.sender.clone();
        let (bg_sender, bg_receiver) = async_channel::unbounded::<BackgroundTask>();
        let meta_sender = self.meta_sender.clone();
        self.bg_sender.replace(Some(bg_sender));
        if let Ok(mut client) =  Client::connect(addr) {
            client.subscribe(self.bg_channel.clone()).expect(
                "Child thread could not subscribe to inter-client channel!"
            );
            let bg_handle = gio::spawn_blocking(move || {
                println!("Starting idle loop...");
                let mut prev_size: usize = bg_receiver.len();
                'outer: loop {
                    // Check if there is work to do
                    if !bg_receiver.is_empty() {
                        if prev_size == 0 {
                            // We have tasks now, set state to busy
                            prev_size = bg_receiver.len();
                            let _ = sender_to_fg.send_blocking(MpdMessage::Busy(true));
                        }
                        // TODO: Take one task for each loop
                        if let Ok(task) = bg_receiver.recv_blocking() {
                            // println!("Got task: {:?}", task);
                            match task {
                                BackgroundTask::Update => {
                                    background::update_mpd_database(
                                        &mut client, &sender_to_fg
                                    )
                                }
                                BackgroundTask::DownloadAlbumArt(uri, key, path, thumbnail_path) => {
                                    background::download_album_art(
                                        &mut client, &meta_sender, uri, key, path, thumbnail_path
                                    )
                                }
                                BackgroundTask::FetchAlbums => {
                                    background::fetch_all_albums(
                                        &mut client,
                                        &sender_to_fg
                                    )
                                }
                                BackgroundTask::FetchAlbumSongs(tag) => {
                                    background::fetch_album_songs(
                                        &mut client, &sender_to_fg, tag
                                    )
                                }
                                BackgroundTask::FetchArtists(use_albumartist) => {
                                    background::fetch_artists(
                                        &mut client, &sender_to_fg, use_albumartist
                                    )
                                }
                                BackgroundTask::FetchArtistSongs(name) => {
                                    background::fetch_songs_of_artist(
                                        &mut client, &sender_to_fg, name
                                    )
                                }
                                BackgroundTask::FetchArtistAlbums(name) => {
                                    background::fetch_albums_of_artist(
                                        &mut client, &sender_to_fg, name
                                    )
                                }
                            }
                        }
                    }
                    else {
                        if prev_size > 0 {
                            // No more tasks
                            prev_size = 0;
                            let _ = sender_to_fg.send_blocking(MpdMessage::Busy(false));
                        }
                        // If not, go into idle mode
                        if let Ok(changes) = client.wait(&[]) {
                            println!("Change: {:?}", changes);
                            if changes.contains(&Subsystem::Message) {
                                if let Ok(msgs) = client.readmessages() {
                                    for msg in msgs {
                                        let content = msg.message.as_str();
                                        println!("Received msg: {}", content);
                                        match content {
                                            // More to come
                                            "STOP" => {break 'outer}
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            let _ = sender_to_fg.send_blocking(MpdMessage::Idle(changes));
                        }
                    }
                }
            });
            self.bg_handle.replace(Some(bg_handle));
        }
        else {
            // Since many features now run in the child thread, it is no longer acceptable
            // to run without one.
            panic!("Could not spawn a child thread for the background client!")
        }
    }

    fn setup_channel(self: Rc<Self>, receiver: Receiver<MpdMessage>) {
        // Set up a listener to the receiver we got from Application.
        // This will be the loop that handles user interaction and idle updates.
        glib::MainContext::default().spawn_local(clone!(
            #[weak(rename_to = this)]
            self,
            async move {
            use futures::prelude::*;
            // Allow receiver to be mutated, but keep it at the same memory address.
            // See Receiver::next doc for why this is needed.
            let mut receiver = std::pin::pin!(receiver);
            while let Some(request) = receiver.next().await {
                this.clone().respond(request).await;
            }
        }));

        // Set up a ping loop. Main client does not use idle mode, so it needs to ping periodically.
        // If there is no client connected, it will simply skip pinging.
        let conn = utils::settings_manager().child("client");
        let ping_interval = conn.uint("mpd-ping-interval-s");
        glib::MainContext::default().spawn_local(clone!(
            #[weak(rename_to = this)]
            self,
            async move {
            loop {
                if let Some(client) = this.main_client.borrow_mut().as_mut() {
                    let res = client.ping();
                    if res.is_ok() {
                        println!("[KeepAlive]");
                    }
                    else {
                        println!("[KeepAlive] [FATAL] Could not ping mpd. The connection might have already timed out, or the daemon might have crashed.");
                        break;
                    }
                }
                else {
                    println!("[KeepAlive] There is no client currently running. Won't ping.");
                }
                glib::timeout_future_seconds(ping_interval).await;
            }
        }));
    }

    async fn respond(self: Rc<Self>, request: MpdMessage) -> glib::ControlFlow {
        // println!("Received MpdMessage {:?}", request);
        match request {
            MpdMessage::Connect => self.connect().await,
            MpdMessage::Update => self.queue_task(BackgroundTask::Update),
            MpdMessage::Output(id, state) => self.set_output(id, state),
            MpdMessage::Volume(vol) => self.volume(vol),
            MpdMessage::Crossfade(fade) => self.set_crossfade(fade),
            MpdMessage::MixRampDb(db) => self.set_mixramp_db(db),
            MpdMessage::MixRampDelay(delay) => self.set_mixramp_delay(delay),
            MpdMessage::Status => self.get_status(),
            MpdMessage::Add(uri) => self.add(uri.as_ref()),
            MpdMessage::SetPlaybackFlow(flow) => self.set_playback_flow(flow),
            MpdMessage::ReplayGain(mode) => self.set_replaygain(mode),
            MpdMessage::Random(state) => self.set_random(state),
            MpdMessage::Consume(state) => self.set_consume(state),
            MpdMessage::Play => self.pause(false),
            MpdMessage::PlayId(id) => self.play_at(id, true),
            MpdMessage::DeleteId(id) => self.delete_at(id, true),
            MpdMessage::PlayPos(pos) => self.play_at(pos, false),
            MpdMessage::Pause => self.pause(true),
            MpdMessage::Prev => self.prev(),
            MpdMessage::Next => self.next(),
            MpdMessage::Clear => self.clear_queue(),
            MpdMessage::Idle(changes) => self.handle_idle_changes(changes).await,
            MpdMessage::SeekCur(position) => self.seek_current_song(position),
            MpdMessage::Queue => self.get_current_queue(),
            MpdMessage::Albums => self.queue_task(BackgroundTask::FetchAlbums),
            MpdMessage::AlbumArt(folder_uri, key, path, thumbnail_path) => {
                self.queue_task(
                    BackgroundTask::DownloadAlbumArt(folder_uri.to_owned(), key, path, thumbnail_path)
                );
            },
            MpdMessage::AlbumContent(tag) => {
                // For now we only have songs.
                // In the future we might want to have additional types of per-album content,
                // such as participant artists.
                self.queue_task(BackgroundTask::FetchAlbumSongs(tag))
            }
            MpdMessage::Artists(use_albumartist) => {
                self.queue_task(BackgroundTask::FetchArtists(use_albumartist));
            }
            MpdMessage::ArtistContent(name) => self.get_artist_content(name),
            MpdMessage::FindAdd(terms) => self.find_add(terms),

            // Result messages from child thread
            MpdMessage::AlbumArtDownloaded(folder_uri, hires, thumb) => self.state.emit_by_name::<()>(
                "album-art-downloaded",
                &[
                    &folder_uri,
                    &BoxedAnyObject::new(hires),
                    &BoxedAnyObject::new(thumb),
                ]
            ),
            MpdMessage::GetSticker(typ, uri, name) => self.get_sticker(&typ, &uri, &name),
            MpdMessage::SetSticker(typ, uri, name, value) => self.set_sticker(&typ, &uri, &name, &value),
            MpdMessage::AlbumArtNotAvailable(folder_uri) => self.state.emit_result(
                "album-art-not-available",
                folder_uri
            ),
            MpdMessage::AlbumBasicInfoDownloaded(info) => self.on_album_downloaded(
                "album-basic-info-downloaded",
                None,
                info
            ),
            MpdMessage::AlbumSongInfoDownloaded(tag, songs) => self.on_songs_downloaded(
                "album-songs-downloaded",
                tag,
                songs
            ),
            MpdMessage::ArtistBasicInfoDownloaded(info) => self.state.emit_result(
                "artist-basic-info-downloaded",
                Artist::from(info)
            ),
            MpdMessage::ArtistSongInfoDownloaded(name, songs) => self.on_songs_downloaded(
                "artist-songs-downloaded",
                name,
                songs
            ),
            MpdMessage::ArtistAlbumBasicInfoDownloaded(artist_name, album_info) => self.on_album_downloaded(
                "artist-album-basic-info-downloaded",
                Some(artist_name),
                album_info
            ),
            MpdMessage::DBUpdated => {},
            MpdMessage::Busy(busy) => self.state.set_busy(busy),
        }
        glib::ControlFlow::Continue
    }

    async fn handle_idle_changes(&self, changes: Vec<Subsystem>) {
        for subsystem in changes {
            match subsystem {
                Subsystem::Player | Subsystem::Options => {
                    // No need to get current song separately as we'll just pull it
                    // from the queue
                    self.get_status();
                }
                Subsystem::Queue => {
                    // Retrieve entire queue for now, since there's no way to know
                    // specifically what changed
                    self.get_current_queue();
                }
                Subsystem::Output => {
                    self.get_outputs();
                }
                // More to come
                _ => {}
            }
        }
    }

    fn queue_task(&self, task: BackgroundTask) {
        if let Some(sender) = self.bg_sender.borrow().as_ref() {
            sender.send_blocking(task).expect("Cannot queue background task");
            if let Some(client) = self.main_client.borrow_mut().as_mut() {
                // Wake background thread
                let _ = client.sendmessage(self.bg_channel.clone(), "WAKE");
            }
            else {
                println!("Warning: cannot wake child thread. Task might be delayed.");
            }
        }
        else {
            panic!("Cannot queue background task (background sender not initialised)");
        }
    }

    fn init_state(&self) {
        self.queue_task(BackgroundTask::FetchAlbums);
        self.queue_task(BackgroundTask::FetchArtists(false));
        self.get_outputs();
        // Get queue first so we can look for current song in it later
        self.get_current_queue();
        self.get_status();
    }

    async fn connect(self: Rc<Self>) {
        // Close current clients
        if let Some(mut main_client) = self.main_client.borrow_mut().take() {
            println!("Closing existing clients");
            // Stop child thread by sending a "STOP" message through mpd itself
            let _ = main_client.sendmessage(self.bg_channel.clone(), "STOP");
            // Now close the main client
            let _ = main_client.close();
        }
        // self.state.set_connection_state(ConnectionState::NotConnected);
        // Wait for child client to stop.
        if let Some(handle) = self.bg_handle.take() {
            let _ = handle.await;
            println!("Stopped all clients successfully.");
        }
        let conn = utils::settings_manager().child("client");

        let addr = format!("{}:{}", conn.string("mpd-host"), conn.uint("mpd-port"));
        println!("Connecting to {}", &addr);
        self.state.set_connection_state(ConnectionState::Connecting);
        let addr_clone = addr.clone();
        let handle = gio::spawn_blocking(move || {
            mpd::Client::connect(addr_clone)
        }).await;
        if let Ok(Ok(client)) = handle {
            self.main_client.replace(Some(client));
            self.main_client
                .borrow_mut()
                .as_mut()
                .unwrap()
                .subscribe(self.bg_channel.clone())
                .expect("Could not connect to an inter-client channel for child thread wakeups!");
            self.clone().start_bg_thread(addr.as_ref());
            self.init_state();
            self.state.set_connection_state(ConnectionState::Connected);
        }
        else {
            self.state.set_connection_state(ConnectionState::NotConnected);
        }
    }

    pub fn add(&self, uri: &str) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.push(get_dummy_song(uri));
        }
    }

    pub fn volume(&self, vol: i8) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.volume(vol);
        }
    }

    fn get_outputs(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(outputs) = client.outputs() {
                self.state.emit_boxed_result("outputs-changed", outputs);
            }
        }
    }

    fn set_output(&self, id: u32, state: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.output(id, state);
        }
    }

    fn get_sticker(&self, typ: &str, uri: &str, name: &str) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let res = client.sticker(typ, uri, name);
            if let Ok(sticker) = res {
                self.state.emit_by_name::<()>("sticker-downloaded", &[
                    &typ.to_value(),
                    &uri.to_value(),
                    &name.to_value(),
                    &sticker.to_value()
                ]);
            }
            else if let Err(error) = res {
                match error {
                    Error::Server(server_err) => {
                        if server_err.detail.contains("disabled") {
                            self.state.emit_by_name::<()>("sticker-db-disabled", &[]);
                        }
                        else if server_err.detail.contains("no such sticker") {
                            self.state.emit_by_name::<()>("sticker-not-found", &[
                                &typ.to_value(),
                                &uri.to_value(),
                                &name.to_value(),
                            ]);
                        }
                    }
                    _ => {
                        // Not handled yet
                    }
                }
            }
        }
    }

    fn set_sticker(&self, typ: &str, uri: &str, name: &str, value: &str) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.set_sticker(typ, uri, name, value);
        }
    } 

    pub fn get_status(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(status) = client.status() {
                // Let each state update their respective properties
                self.state.emit_boxed_result("status-changed", status);
            }
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }

    pub fn set_playback_flow(&self, flow: PlaybackFlow) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            match flow {
                PlaybackFlow::Sequential => {
                    let _ = client.repeat(false);
                    let _ = client.single(false);
                }
                PlaybackFlow::Repeat => {
                    let _ = client.repeat(true);
                    let _ = client.single(false);
                }
                PlaybackFlow::Single => {
                    let _ = client.repeat(false);
                    let _ = client.single(true);
                }
                PlaybackFlow::RepeatSingle => {
                    let _ = client.repeat(true);
                    let _ = client.single(true);
                }
            }
        }
    }

    pub fn set_crossfade(&self, fade: f64) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.crossfade(fade as i64);
        }
    }

    pub fn set_replaygain(&self, mode: mpd::status::ReplayGain) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.replaygain(mode);
        }
    }

    pub fn set_mixramp_db(&self, db: f32) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.mixrampdb(db);
        }
    }

    pub fn set_mixramp_delay(&self, delay: f64) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.mixrampdelay(delay);
        }
    }

    pub fn set_random(&self, state: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.random(state);
        }
    }

    pub fn set_consume(&self, state: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.consume(state);
        }
    }

    pub fn pause(self: Rc<Self>, is_pause: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.pause(is_pause);
        }
    }
    pub fn prev(self: Rc<Self>) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            // TODO: Make it stop/play base on toggle
            let _ = client.prev();
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }

    pub fn next(self: Rc<Self>) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            // TODO: Make it stop/play base on toggle
            let _ = client.next();
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }

    pub fn play_at(self: Rc<Self>, id_or_pos: u32, is_id: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if is_id {
                client.switch(Id(id_or_pos)).expect("Could not switch song");
            }
            else {
                client.switch(id_or_pos).expect("Could not switch song");
            }
        }
    }

    pub fn delete_at(self: Rc<Self>, id_or_pos: u32, is_id: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if is_id {
                client.delete(Id(id_or_pos)).expect("Could not delete song from queue");
            }
            else {
                client.delete(id_or_pos).expect("Could not delete song from queue");
            }
        }
    }

    pub fn clear_queue(self: Rc<Self>) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            // TODO: Make it stop/play base on toggle
            let _ = client.clear();
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }

    pub fn seek_current_song(&self, position: f64) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let _ = client.rewind(position);
            // If successful, should trigger an idle message for Player
        }
    }

    pub fn get_current_queue(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(mut queue) = client.queue() {
                let songs: Vec<Song> = queue
                    .iter_mut()
                    .map(|mpd_song| {Song::from(std::mem::take(mpd_song))})
                    .collect();
                self.state.emit_boxed_result("queue-changed", songs);
            }
        }
    }

    fn on_songs_downloaded(
        &self,
        signal_name: &str,
        tag: String,
        songs: Vec<SongInfo>
    ) {
        if !songs.is_empty() {
            // Append to listener lists
            self.state.emit_by_name::<()>(
                signal_name,
                &[
                    &tag,
                    &BoxedAnyObject::new(songs.into_iter().map(Song::from).collect::<Vec<Song>>())
                ]
            );
        }
    }

    fn on_album_downloaded(
        &self,
        signal_name: &str,
        tag: Option<String>,
        info: AlbumInfo
    ) {
        // Append to listener lists
        if let Some(tag) = tag {
            self.state.emit_by_name::<()>(
                signal_name,
                &[
                    &tag,
                    &Album::from(info)
                ]
            );
        }
        else {
            self.state.emit_by_name::<()>(
                signal_name,
                &[
                    &Album::from(info)
                ]
            );
        }
    }

    fn on_artist_downloaded(
        &self,
        signal_name: &str,
        tag: Option<String>,  // For future features, such as fetching artists in album content view
        info: ArtistInfo
    ) {
        // Append to listener lists
        if let Some(tag) = tag {
            self.state.emit_by_name::<()>(
                signal_name,
                &[
                    &tag,
                    &BoxedAnyObject::new(Artist::from(info))
                ]
            );
        }
        else {
            self.state.emit_by_name::<()>(
                signal_name,
                &[
                    &BoxedAnyObject::new(Artist::from(info))
                ]
            );
        }
    }

    pub fn get_artist_content(&self, name: String) {
        // For artists, we will need to find by substring to include songs and albums that they
        // took part in
        self.queue_task(BackgroundTask::FetchArtistSongs(name.clone()));
        self.queue_task(BackgroundTask::FetchArtistAlbums(name.clone()));
    }

    pub fn find_add(&self, query: Query) {
        // Convert back to mpd::search::Query
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            // println!("Running findadd query: {:?}", &terms);
            // let mut query = Query::new();
            // for term in terms.into_iter() {
            //     query.and(term.0.into(), term.1);
            // }
            client.findadd(&query).expect("Failed to run query!");
        }
    }
}

impl Drop for MpdWrapper {
    fn drop(&mut self) {
        if let Some(mut main_client) = self.main_client.borrow_mut().take() {
            println!("App closed. Closing clients...");
            // First, send stop message
            let _ = main_client.sendmessage(self.bg_channel.clone(), "STOP");
            // Now close the main client, which will trigger an idle message.
            let _ = main_client.close();
            // Now the child thread really should have read the stop_flag.
            // Wait for it to stop.
            if let Some(handle) = self.bg_handle.take() {
                let _ = executor::block_on(handle);
            }
        }
    }
}

