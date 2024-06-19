use crate::mpd;
use async_channel::{Sender, Receiver};
use glib::clone;
use gtk::glib;

use std::{
    cell::RefCell,
    fmt::{self, Display, Formatter},
    rc::Rc,
    sync::{Arc, Mutex}
};

// TODO: Reorganise these soon into nested enums for clarity
pub enum UiMessage {
    Connect(String, String), // Host and port (both as strings)
	Play,
	Pause,
	GetPlaybackStatus,

}

pub enum MpdMessage {
    PlaybackPosition(u64), // Might want to switch to floats for sub-second updates?
    PlaybackStatus(mpd::status::Status),
    ClientError(String), // TODO: Need to flesh this out into different variants for easier UI-side handling
    NotConnectedError
}

#[derive(Debug)]
pub struct MpdWrapper {
    // For communicating with GUI.
    // Receiver should be of a bounded channel with max size 1 since we're dealing
    // with a single mpd::Client object in a RefCell.
    sender: Sender<MpdMessage>,
    receiver: RefCell<Option<Receiver<UiMessage>>>,
	client: RefCell<Option<mpd::Client>>,
}

impl MpdWrapper {
    pub fn new(sender: Sender<MpdMessage>, r: Receiver<UiMessage>) -> Self {
        let wrapper = Self {
            sender, // to UI
            receiver: RefCell::new(Some(r)), // from UI. Note: RefCell has runtime reference checking
            client: RefCell::new(None)  // Must be initialised later
        };
        wrapper
    }

    fn setup_channel(self: Rc<Self>) {
        let receiver = self.receiver.borrow_mut().take().unwrap();

        // TODO: consider dropping to weak ref
        glib::MainContext::default().spawn_local(clone!(@strong self as this => async move {
            use futures::prelude::*;

            // Allow receiver to be mutated, but keep it at the same memory address.
            let mut receiver = std::pin::pin!(receiver);
            while let Some(request) = receiver.next().await {
                this.respond(request);
            }
        }));
    }

    fn respond(&self, request: UiMessage) -> glib::ControlFlow {
        match request {
            UiMessage::Connect(host, port) => self.connect(&host, &port),
            UiMessage::GetPlaybackStatus => self.status(),
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

    pub fn status(&self) {
        if let Some(mut client) = self.client.borrow_mut().take() {
            if let Ok(res) = client.status() {
                // TODO: don't block main thread while sending this
                self.sender.send(MpdMessage::PlaybackStatus(res));
            }
            else {
                self.sender.send(MpdMessage::ClientError(String::from("Could not get status")));
            }
        }
        else {
            self.sender.send(MpdMessage::NotConnectedError);
        }
    }
}

