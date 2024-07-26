use std::{
    borrow::Cow,
    io::Cursor,
    cell::RefCell,
    rc::Rc,
    path::PathBuf
};
use gtk::gio::prelude::*;
use futures::executor;
use async_channel::{Sender, Receiver};
use glib::clone;
use gtk::{glib, gio};

use crate::{
    utils,
    player::Player,
    library::Library,
    common::{AlbumInfo, Song}
};

use super::{
    albumart::{strip_filename_linux, AlbumArtCache},
    state::{ClientState, ConnectionState}
};

use mpd::{
    client::Client,
    search::{Term, Query, Window},
    song::Id,
    Subsystem,
    Idle,
    Channel
};
use image::{
    io::Reader as ImageReader,
    imageops::FilterType
};
use uuid::Uuid;

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

pub fn read_image_from_bytes(bytes: Vec<u8>) -> Option<image::DynamicImage> {
    if let Ok(reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() {
        if let Ok(dyn_img) = reader.decode() {
            return Some(dyn_img);
        }
        return None;
    }
    None
}

// One for each command in mpd's protocol plus a few special ones such
// as Connect and Toggle.
pub enum MpdMessage {
    Connect, // Host and port are always read from gsettings
	Play,
    Pause,
    PlayPos(u32), // Play song at queue position
    PlayId(u32), // Play song at queue ID
    Clear, // Clear queue
    Prev,
    Next,
	Status,
	SeekCur(f64), // Seek current song to last position set by PrepareSeekCur. For some reason the mpd crate calls this "rewind".
	AlbumArt(String),
    FindAdd(Query<'static>),
	Queue, // Get songs in current queue
    Albums, // Get albums. Will return one by one
    Album(AlbumInfo), // Get list of songs in album with given tag name

	// Reserved for child thread
	Idle(Vec<Subsystem>), // Will only be sent from the child thread
	AlbumArtDownloaded(String), // Notify which album art was downloaded and where it is
    AlbumInfo(AlbumInfo) // Return new album to be added to the list model.
}

// Work requests for sending to the child thread.
// Completed results will be reported back via MpdMessage.
#[derive(Debug)]
enum BackgroundTask {
    DownloadAlbumArt(String, PathBuf, PathBuf),  // With folder-level URL for querying and cache paths to write to (full-res & thumb)
    FetchAlbums  // Gradually get all albums
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

#[derive(Debug)]
pub struct MpdWrapper {
    // References to controllers
    player: Player,
    library: Library,
    // For receiving user commands from UI or child thread
    receiver: RefCell<Option<Receiver<MpdMessage>>>,
    // Corresponding sender, for cloning into child thread.
    sender: Sender<MpdMessage>,
    albumart: Rc<AlbumArtCache>,
    // The main client living on the main thread. Every single method of
    // mpd::Client is mutating so we'll just rely on a RefCell for now.
	main_client: RefCell<Option<Client>>,
    // The state GObject, used for communicating client status & changes to UI elements
    state: Rc<ClientState>,
    // Handle to the child thread.
	bg_handle: RefCell<Option<gio::JoinHandle<()>>>,
	bg_channel: Channel, // For waking up the child client
	bg_sender: RefCell<Option<Sender<BackgroundTask>>>, // For sending tasks to background thread
}

impl MpdWrapper {
    pub fn new(
        player: Player,
        library: Library,
        sender: Sender<MpdMessage>,
        receiver: RefCell<Option<Receiver<MpdMessage>>>,
        albumart: Rc<AlbumArtCache>
    ) -> Rc<Self> {
        let ch_name = Uuid::new_v4().simple().to_string();
        println!("Channel name: {}", &ch_name);
        let wrapper = Rc::new(Self {
            player,
            library,
            receiver, // from UI. Note: RefCell has runtime reference checking
            sender,
            albumart,
            state: Rc::new(ClientState::default()),
            main_client: RefCell::new(None),  // Must be initialised later
            bg_handle: RefCell::new(None),  // Will be spawned later
            bg_channel: Channel::new(&ch_name).unwrap(),
            bg_sender: RefCell::new(None)
        });

        // For future noob self: these are shallow
        wrapper.clone().setup_channel();
        wrapper
    }

    pub fn get_client_state(self: Rc<Self>) -> Rc<ClientState> {
        self.state.clone()
    }

    fn start_bg_thread(self: Rc<Self>, addr: &str) {
        let sender_to_fg = self.sender.clone();
        let (bg_sender, bg_receiver) = async_channel::unbounded::<BackgroundTask>();
        self.bg_sender.replace(Some(bg_sender));
        if let Ok(mut client) =  Client::connect(addr) {
            client.subscribe(self.bg_channel.clone()).expect("Child thread could not subscribe to inter-client channel!");
            let bg_handle = gio::spawn_blocking(move || {
                println!("Starting idle loop...");
                'outer: loop {
                    // Check if there is work to do
                    if !bg_receiver.is_empty() {
                        // TODO: Take one task for each loop
                        if let Ok(task) = bg_receiver.recv_blocking() {
                            // println!("Got task: {:?}", task);
                            match task {
                                BackgroundTask::DownloadAlbumArt(uri, cache_path, thumb_cache_path) => {
                                    // Check if already cached. This usually happens when
                                    // multiple songs using the same un-cached album art
                                    // were placed into the work queue.
                                    if cache_path.exists() {
                                        println!("{:?} already cached, won't download again", uri);
                                    }
                                    else if let Ok(bytes) = client.albumart(&get_dummy_song(&uri)) {
                                        println!("Downloaded album art for {:?}", uri);
                                        if let Some(dyn_img) = read_image_from_bytes(bytes) {
                                            // Might want to make all of these configurable.
                                            let _ = dyn_img.resize(256, 256, FilterType::CatmullRom).save(&cache_path);
                                            let  _= dyn_img.thumbnail(64, 64).save(&thumb_cache_path);
                                        }
                                        sender_to_fg.send_blocking(MpdMessage::AlbumArtDownloaded(uri)).expect(
                                            "Warning: cannot notify main client of new album art."
                                        );
                                    }
                                }
                                BackgroundTask::FetchAlbums => {
                                    // Get list of unique album tags
                                    // Will block child thread until info for all albums have been retrieved.
                                    if let Ok(tag_list) = client.list(&Term::Tag(Cow::Borrowed("album")), &Query::new()) {
                                        for tag in &tag_list {
                                            if let Ok(mut songs) = client.find(
                                                Query::new()
                                                    .and(Term::Tag(Cow::Borrowed("album")), tag),
                                                Window::from((0, 1))
                                            ) {
                                                if !songs.is_empty() {
                                                    let first_song = Song::from(std::mem::take(&mut songs[0]));
                                                    let _ = sender_to_fg.send_blocking(MpdMessage::AlbumInfo(
                                                        AlbumInfo::new(
                                                            strip_filename_linux(&first_song.get_uri()),
                                                            tag.as_str(),
                                                            first_song.get_album_artist().as_deref()
                                                        )
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    else {
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
            println!("Warning: failed to spawn a background client. The background thread will not be spawned. UI might become desynchronised from the daemon.");
        }
    }

    fn setup_channel(self: Rc<Self>) {
        // Set up a listener to the receiver we got from Application.
        // This will be the loop that handles user interaction and idle updates.
        let receiver = self.receiver.borrow_mut().take().unwrap();
        glib::MainContext::default().spawn_local(clone!(
            #[strong(rename_to = this)]
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
            #[strong(rename_to = this)]
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
            MpdMessage::Status => self.get_status(),
            MpdMessage::Play => self.pause(false),
            MpdMessage::PlayId(id) => self.play_at(id, true),
            MpdMessage::PlayPos(pos) => self.play_at(pos, false),
            MpdMessage::Pause => self.pause(true),
            MpdMessage::Prev => self.prev(),
            MpdMessage::Next => self.next(),

            MpdMessage::Clear => self.clear_queue(),
            MpdMessage::Idle(changes) => self.handle_idle_changes(changes).await,
            MpdMessage::SeekCur(position) => self.seek_current_song(position),
            MpdMessage::Queue => self.get_current_queue(),
            MpdMessage::Album(info) => self.get_songs_in_album(info),
            MpdMessage::AlbumArt(folder_uri) => {
                let cache_path = self.albumart.get_path_for(&folder_uri);
                let thumb_cache_path = self.albumart.get_thumbnail_path_for(&folder_uri);
                self.queue_task(
                    BackgroundTask::DownloadAlbumArt(
                        folder_uri.to_owned(),
                        cache_path.clone(),
                        thumb_cache_path.clone()
                    )
                );
            },
            MpdMessage::FindAdd(terms) => self.find_add(terms),
            MpdMessage::AlbumArtDownloaded(folder_uri) => self.notify_album_art(&folder_uri),
            MpdMessage::AlbumInfo(info) => self.notify_album_info(info),
            _ => {}
        }
        glib::ControlFlow::Continue
    }

    async fn handle_idle_changes(&self, changes: Vec<Subsystem>) {
        for subsystem in changes {
            match subsystem {
                Subsystem::Player => {
                    // No need to get current song separately as we'll just pull it
                    // from the queue
                    self.get_status();
                }
                Subsystem::Queue => {
                    // Retrieve entire queue for now, since there's no way to know
                    // specifically what changed
                    self.get_current_queue();
                }
                // Else just skip. More features to come.
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

    pub fn get_status(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(status) = client.status() {
                // Let each state update their respective properties
                self.player.update_status(&status);
            }
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }
    pub fn pause(self: Rc<Self>, is_pause: bool) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            // TODO: Make it stop/play base on toggle
            let _ = client.pause(is_pause);
            // TODO: handle error
        }
        else {
            // TODO: handle error
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
            // TODO: Make it stop/play base on toggle
            if is_id {
                client.switch(Id(id_or_pos)).expect("Could not switch song");
            }
            else {
                client.switch(id_or_pos).expect("Could not switch song");
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
                self.player.update_queue(&mut queue);
            }
        }
    }

    pub fn get_songs_in_album(&self, info: AlbumInfo) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            let songs: Vec<Song> = client.find(
                Query::new().and(Term::Tag(Cow::Borrowed("album")), info.title()),
                Window::from((0, 4096))
            ).unwrap().iter_mut().map(|mpd_song| {Song::from(std::mem::take(mpd_song))}).collect();

            if !songs.is_empty() {
                // Notify library to push new nav page
                self.library.push_album_content_page(info, songs);
            }
        }
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

    pub fn notify_album_art(&self, folder_uri: &str) {
        self.player.update_album_art(folder_uri);
        self.library.update_album_art(folder_uri);
    }

    pub fn notify_album_info(&self, info: AlbumInfo) {
        self.library.add_album_info(info);
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

