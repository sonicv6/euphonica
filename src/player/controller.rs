use std::{
    cell::RefCell,
    fmt::{self, Display, Formatter},
    rc::Rc,
};
use gtk::glib;
use async_channel::{Sender};
use crate::client::wrapper::{MpdMessage};

// TODO: consider whether this is even needed now that state has been moved
// to the client module.
#[derive(Debug)]
pub struct PlayerController {
    // Note to noob self: Sender::send...() only takes &self
    sender: Sender<MpdMessage>
}

impl PlayerController {
    pub fn new(sender: Sender<MpdMessage>) -> Rc<Self> {
        let ctl = Rc::new(Self {
            sender
        });

        ctl
    }
}