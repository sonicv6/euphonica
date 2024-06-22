use crate::mpd;
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
use super::subsystems::player::PlayerState;

use std::{
    cell::RefCell,
    fmt::{self, Display, Formatter},
    rc::Rc,
    sync::{Arc, Mutex}
};

// One for each command in mpd's protocol plus a few special ones such
// as Connect and Toggle.
pub enum MpdMessage {
    Connect(String, String), // Host and port (both as strings)
	Play,
	Toggle, // the "pause" command but renamed since it's a misnomer
	Status,
}

// Thin wrapper around the blocking mpd::Client. It contains two separate client
// objects connected to the same address. One lives on the main thread along
// with the GUI and takes care of sending user commands to the daemon, while the
// other lives on on a child thread and is is always in idle mode in order to
// receive all server-side changes, including those resulting from commands from
// other clients, such as MPRIS controls in the notification centre or another
// frontend. Note that this second client will not notify the main thread on
// seekbar progress. That will have to be polled by the main thread.

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
    // State objects
    player_state: PlayerState,

    // For receiving user commands from UI
    ui_receiver: RefCell<Option<Receiver<MpdMessage>>>,
    // For getting state change notifications from the child thread
    bg_sender: Sender<Vec<Subsystem>>, // kept here first since the child thread won't be spawned right away
    bg_receiver: RefCell<Option<Receiver<Vec<Subsystem>>>>,
    // The main client living on the main thread. Every single method of
    // mpd::Client is mutating so we'll just rely on a RefCell for now.
	main_client: RefCell<Option<Client>>,
    // Handle to the child thread.
	bg_handle: RefCell<Option<gio::JoinHandle<()>>>,
	stop_flag: Arc<AtomicBool>, // used to tell the child thread to stop looping
}

impl MpdWrapper {
    pub fn new(receiver: RefCell<Option<Receiver<MpdMessage>>>) -> Rc<Self> {
        let (bg_sender, bg_r): (Sender<Vec<Subsystem>>, Receiver<Vec<Subsystem>>) = async_channel::bounded(1);
        let wrapper = Rc::new(Self {
            player_state: PlayerState::default(),
            ui_receiver: receiver, // from UI. Note: RefCell has runtime reference checking
            bg_sender,
            bg_receiver: RefCell::new(Some(bg_r)),
            main_client: RefCell::new(None),  // Must be initialised later
            bg_handle: RefCell::new(None),  // Will be spawned later
            stop_flag: Arc::new(AtomicBool::new(false))
        });

        // For future noob self: these are shallow
        wrapper.clone().setup_ui_channel();
        wrapper
    }

    fn start_bg_thread(self: Rc<Self>, host: &str, port: &str) {
        let bg_sender = self.bg_sender.clone();
        let stop_flag = self.stop_flag.clone();
        if let Ok(mut client) =  Client::connect(format!("{}:{}", host, port)) {
            let bg_handle = gio::spawn_blocking(move || {
                println!("Starting idle loop...");
                loop {
                    if stop_flag.load(Ordering::Relaxed) {
                        println!("Stop flag is true, terminating background thread...");
                        client.close();
                        break;
                    }
                    if let Ok(changes) = client.wait(&[]) {
                        println!("Change: {:?}", changes);
                        bg_sender.send_blocking(changes);
                    }
                }
            });
            self.bg_handle.replace(Some(bg_handle));
        }
        else {
            println!("Warning: failed to spawn a background client. The background thread will not be spawned. UI might become desynchronised from the daemon.");
        }
    }

    fn setup_ui_channel(self: Rc<Self>) {
        // Set up a listener to the receiver we got from Application.
        // This will be the loop that handles user interaction.
        // For this to work, we need to interrupt the idle loop to handle the
        // interaction, then restart it afterwards.
        let receiver = self.ui_receiver.borrow_mut().take().unwrap();
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

    pub fn get_player_state(&self) -> &PlayerState {
        // Only allow references
        &self.player_state
    }

    async fn respond(self: Rc<Self>, request: MpdMessage) -> glib::ControlFlow {
        match request {
            MpdMessage::Connect(host, port) => self.connect(&host, &port).await,
            MpdMessage::Status => self.get_status(),
            _ => {}
        }
        glib::ControlFlow::Continue
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
            main_client.close();
            // Now the child thread really should have read the stop_flag.
            // Wait for it to stop.
            if let Some(handle) = self.bg_handle.take() {
                handle.await;
            }
        }
        println!("Connecting to {}:{}", host, port);
        self.stop_flag.store(false, Ordering::Relaxed);
        if let Ok(c) = mpd::Client::connect(format!("{}:{}", host, port)) {
            self.main_client.replace(Some(c));
            self.clone().get_status();
            self.start_bg_thread(host, port);
        }
    }

    pub fn get_status(self: Rc<Self>) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(status) = client.status() {
                println!("Playback state is {:?}", status.state);
                // Let each state update their respective properties
                self.player_state.update_status(&status);
            }
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }

    pub fn get_current_song(&self) {
        if let Some(client) = self.main_client.borrow_mut().as_mut() {
            if let Ok(cs) = client.currentsong() {
                // Let each state update their respective properties
                self.player_state.update_current_song(&cs);
            }
            // TODO: handle error
        }
        else {
            // TODO: handle error
        }
    }
}

