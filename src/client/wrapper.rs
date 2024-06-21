use crate::mpd;
use async_channel::{Sender, Receiver};
use glib::clone;
use gtk::glib;
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

#[derive(Debug)]
pub struct MpdWrapper {
    pub player_state: Rc<PlayerState>,
    receiver: RefCell<Option<Receiver<MpdMessage>>>,
    // Every single method of mpd::Client is mutating so we'll just rely on a
    // RefCell for now.
	client: RefCell<Option<mpd::Client>>,
}

impl MpdWrapper {
    pub fn new(receiver: RefCell<Option<Receiver<MpdMessage>>>) -> Rc<Self> {
        // Set up state objects (one for each of mpd's subsystems that we use)
        // TODO: init states to current mpd status.
        let player_state = Rc::new(PlayerState::default());

        let wrapper = Rc::new(Self {
            player_state,
            receiver, // from UI. Note: RefCell has runtime reference checking
            client: RefCell::new(None)  // Must be initialised later
        });

        // For future noob self: this is shallow
        wrapper.clone().setup_channel();

        wrapper
    }

    fn setup_channel(self: Rc<Self>) {
        let receiver = self.receiver.borrow_mut().take().unwrap();
        glib::MainContext::default().spawn_local(clone!(@strong self as this => async move {
            use futures::prelude::*;

            // Allow receiver to be mutated, but keep it at the same memory address.
            // See Receiver::next doc for why this is needed.
            let mut receiver = std::pin::pin!(receiver);
            while let Some(request) = receiver.next().await {
                this.respond(request);
            }
        }));
    }

    fn respond(&self, request: MpdMessage) -> glib::ControlFlow {
        match request {
            MpdMessage::Connect(host, port) => self.connect(&host, &port),
            MpdMessage::Status => self.get_status(),
            _ => {}
        }
        glib::ControlFlow::Continue
    }

    pub fn connect(&self, host: &str, port: &str) {
        // FIXME: this might freeze the UI if connection takes too long
        // Consider making async.
        if let Ok(c) = mpd::Client::connect(format!("{}:{}", host, port)) {
            self.client.replace(Some(c));
        }
        // TODO: return connection error if any
    }

    pub fn get_status(&self) {
        if let Some(mut client) = self.client.borrow_mut().take() {
            if let Ok(status) = client.status() {
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
        if let Some(mut client) = self.client.borrow_mut().take() {
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

