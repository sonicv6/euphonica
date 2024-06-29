extern crate mpd;
use crate::player::Player;
use futures::executor;
use mpd::{
    client::Client,
    Subsystem,
    Idle,
    error::Error
};
use std::sync::atomic::{AtomicBool, Ordering};
use async_channel::{Sender, Receiver};
use glib::{clone, SourceId, MainContext};
use gtk::{glib, gio};

use std::{
    cell::RefCell,
    fmt::{self, Display, Formatter},
    rc::Rc,
    sync::{Arc, Mutex}
};

// One for each command in mpd's protocol plus a few special ones such
// as Connect and Toggle.
#[derive(Debug)]
pub enum MpdMessage {
    Connect(String, String), // Host and port (both as strings)
	Play,
	Toggle, // The "pause" command but renamed since it's a misnomer
	Status(bool), // If true, will also query current song
	SeekCur(f64), // Seek current song to last position set by PrepareSeekCur. For some reason the mpd crate calls this "rewind".
	CurrentSong,
	AlbumArt(String), // Contains URI of song WITHOUT filename
	Idle(Vec<Subsystem>), // Will only be sent from the child thread
	Queue, // Get songs in current queue
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
    player: Rc<Player>,

    // For receiving user commands from UI or child thread
    receiver: RefCell<Option<Receiver<MpdMessage>>>,
    // Corresponding sender, for cloning into child thread.
    sender: Sender<MpdMessage>,
    // The main client living on the main thread. Every single method of
    // mpd::Client is mutating so we'll just rely on a RefCell for now.
	main_client: RefCell<Option<Client>>,
    // Handle to the child thread.
	bg_handle: RefCell<Option<gio::JoinHandle<()>>>,
	stop_flag: Arc<AtomicBool>, // used to tell the child thread to stop looping
}

impl MpdWrapper {
    pub fn new(
        player: Rc<Player>,
        sender: Sender<MpdMessage>,
        receiver: RefCell<Option<Receiver<MpdMessage>>>
    ) -> Rc<Self> {
        let wrapper = Rc::new(Self {
            player,
            receiver, // from UI. Note: RefCell has runtime reference checking
            sender,
            main_client: RefCell::new(None),  // Must be initialised later
            bg_handle: RefCell::new(None),  // Will be spawned later
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        // For future noob self: these are shallow
        wrapper.clone().setup_channel();
        wrapper
    }

    fn start_bg_thread(self: Rc<Self>, host: &str, port: &str) {
        let bg_sender = self.sender.clone();
        let stop_flag = self.stop_flag.clone();
        if let Ok(mut client) =  Client::connect(format!("{}:{}", host, port)) {
            let bg_handle = gio::spawn_blocking(move || {
                println!("Starting idle loop...");
                loop {
                    if stop_flag.load(Ordering::Relaxed) {
                        println!("Stop flag is true, terminating background thread...");
                        let _ = client.close();
                        break;
                    }
                    if let Ok(changes) = client.wait(&[]) {
                        println!("Change: {:?}", changes);
                        let _ = bg_sender.send_blocking(MpdMessage::Idle(changes));
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
        glib::MainContext::default().spawn_local(clone!(@strong self as this => async move {
            use futures::prelude::*;
            // Allow receiver to be mutated, but keep it at the same memory address.
            // See Receiver::next doc for why this is needed.
            let mut receiver = std::pin::pin!(receiver);
            while let Some(request) = receiver.next().await {
                this.clone().respond(request).await;
            }
        }));
    }

    async fn respond(self: Rc<Self>, request: MpdMessage) -> glib::ControlFlow {
        match request {
            MpdMessage::Connect(host, port) => self.connect(&host, &port).await,
            MpdMessage::Status(update_current_song) => self.get_status(update_current_song),
            MpdMessage::CurrentSong => self.get_current_song(),
            MpdMessage::Idle(changes) => self.handle_idle_changes(changes).await,
            MpdMessage::SeekCur(position) => self.seek_current_song(position),
            _ => {}
        }
        glib::ControlFlow::Continue
    }

    async fn handle_idle_changes(self: Rc<Self>, changes: Vec<Subsystem>) {
        for subsystem in changes {
            match subsystem {
                Subsystem::Player => {
                    // For now idle signals from Player will also invoke currentsong.
                    // Might want to keep track of songid to avoid this when possible.
                    self.clone().get_status(true);
                }
                // Else just skip. More features to come.
                _ => {}
            }
        }
    }

    fn init_state(self: Rc<Self>) {
        self.clone().get_status(true);
        self.clone().get_current_queue();
    }

    async fn connect(self: Rc<Self>, host: &str, port: &str) {
        // Close current clients
        if let Some(mut main_client) = self.main_client.borrow_mut().take() {
            println!("Closing existing clients");
            // First, set stop_flag to true
            self.stop_flag.store(true, Ordering::Relaxed);
            // Child thread might have stopped by now if there are idle messages,
            // but that's not guaranteed.
            // Now close the main client, which will trigger an idle message.
            let _ = main_client.close();
            // Now the child thread really should have read the stop_flag.
            // Wait for it to stop.
            if let Some(handle) = self.bg_handle.take() {
                let _ = handle.await;
            }
        }
        println!("Connecting to {}:{}", host, port);
        self.stop_flag.store(false, Ordering::Relaxed);
        if let Ok(c) = mpd::Client::connect(format!("{}:{}", host, port)) {
            self.main_client.replace(Some(c));
            self.clone().init_state();
            self.start_bg_thread(host, port);
        }
    }

    pub fn get_status(self: Rc<Self>, update_current_song: bool) {
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

        if update_current_song {
            self.get_current_song();
        }
    }

    pub fn get_current_song(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(maybe_song) = client.currentsong() {
                // Let each state update their respective properties
                self.player.update_current_song(&maybe_song);
            }
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
            if let Ok(queue) = client.queue() {
                self.player.update_queue(&queue);
            }
        }
    }
}

impl Drop for MpdWrapper {
    fn drop(&mut self) {
        if let Some(mut main_client) = self.main_client.borrow_mut().take() {
            println!("App closed. Closing clients...");
            // First, set stop_flag to true
            self.stop_flag.store(true, Ordering::Relaxed);
            // Child thread might have stopped by now if there are idle messages,
            // but that's not guaranteed.
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

