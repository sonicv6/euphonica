use std::{
    cell::Cell,
    sync::OnceLock
};
use gtk::glib;
use glib::{
    prelude::*,
    subclass::{
        prelude::*,
        Signal
    },
    BoxedAnyObject
};

use crate::common::{Album, Artist};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "EuphonicaConnectionState")]
pub enum ConnectionState {
    #[default]
    NotConnected,
    Connecting,
    Unauthenticated,  // TCP stream set up but no/wrong password.
    Connected
}

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecBoolean,
        ParamSpecEnum
    };
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct ClientState {
        pub connection_state: Cell<ConnectionState>,
        // Used to indicate that the background client is busy.
        pub busy: Cell<bool>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClientState {
        const NAME: &'static str = "EuphonicaClientState";
        type Type = super::ClientState;

        fn new() -> Self {
            Self {
                connection_state: Cell::default(),
                busy: Cell::new(false)
            }
        }
    }

    impl ObjectImpl for ClientState {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::builder("busy").read_only().build(),
                    ParamSpecEnum::builder::<ConnectionState>("connection-state").read_only().build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "connection-state" => obj.get_connection_state().to_value(),
                "busy" => obj.is_busy().to_value(),
                _ => unimplemented!(),
            }
        }

        // fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        //     let obj = self.obj();
        //     match pspec.name() {
        //         "connection-state" => {
        //             let state = value.get().expect("Error in ClientState::set_property");
        //             obj.set_connection_state(state);
        //         },
        //         _ => unimplemented!()
        //     }
        // }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("sticker-downloaded")
                        .param_types([
                            String::static_type(),         // Type
                            String::static_type(),         // URI
                            String::static_type(),         // name
                            String::static_type()          // value
                        ])
                        .build(),
                    Signal::builder("sticker-not-found")
                        .param_types([
                            String::static_type(),         // Type
                            String::static_type(),         // URI
                            String::static_type(),         // name
                        ])
                        .build(),
                    Signal::builder("sticker-db-disabled")
                        .build(),
                    Signal::builder("album-art-downloaded")
                        .param_types([
                            String::static_type(),         // folder URI
                            BoxedAnyObject::static_type(), // hires
                            BoxedAnyObject::static_type()  // thumbnail
                        ])
                        .build(),
                    Signal::builder("album-art-not-available")
                        .param_types([
                            String::static_type(),  // folder URI
                        ])
                        .build(),
                    Signal::builder("outputs-changed")
                        .param_types([BoxedAnyObject::static_type()])  // Vec<mpd::output::Output>
                        .build(),
                    // Enough information about this album has been downloaded to display it
                    // as a thumbnail in the album view
                    Signal::builder("album-basic-info-downloaded")
                        .param_types([Album::static_type()])
                        .build(),
                    // A chunk of an album's songs have been retrieved. Emit this
                    // to make AlbumContentView append this chunk.
                    Signal::builder("album-songs-downloaded")
                        .param_types([
                            String::static_type(),
                            BoxedAnyObject::static_type()  // Vec<Song>
                        ])
                        .build(),
                    // ArtistInfo downloaded. Should probably queue metadata retrieval.
                    Signal::builder("artist-basic-info-downloaded")
                        .param_types([Artist::static_type()])
                        .build(),
                    // A chunk of an artist's songs have been retrieved. Emit this
                    // to make ArtistContentView append this chunk.
                    Signal::builder("artist-songs-downloaded")
                        .param_types([
                            String::static_type(),
                            BoxedAnyObject::static_type()  // Vec<Song>
                        ])
                        .build(),
                    Signal::builder("artist-album-basic-info-downloaded")
                        .param_types([
                            String::static_type(),
                            Album::static_type()
                        ])
                        .build(),
                    Signal::builder("status-changed")
                        .param_types([BoxedAnyObject::static_type()])
                        .build(),
                    Signal::builder("queue-changed")
                        .param_types([BoxedAnyObject::static_type()])  // Vec<Song>
                        .build(),
                    Signal::builder("queue-replaced")
                        .param_types([BoxedAnyObject::static_type()])  // Vec<Song>
                        .build(),
                    Signal::builder("folder-contents-downloaded")
                        .param_types([
                            str::static_type(), // corresponding path
                            BoxedAnyObject::static_type() // Vec<INode>
                        ])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct ClientState(ObjectSubclass<imp::ClientState>);
}

impl Default for ClientState {
    fn default() -> Self {
        glib::Object::new()
    }
}


impl ClientState {
    pub fn get_connection_state(&self) -> ConnectionState {
        self.imp().connection_state.get()
    }

    pub fn is_busy(&self) -> bool {
        self.imp().busy.get()
    }

    pub fn set_connection_state(&self, new_state: ConnectionState) {
        let old_state = self.imp().connection_state.replace(new_state);
        if old_state != new_state {
            self.notify("connection-state");
        }
    }

    pub fn set_busy(&self, new_busy: bool) {
        let old_busy = self.imp().busy.replace(new_busy);
        if old_busy != new_busy {
            self.notify("busy");
        }
    }

    // Convenience emit wrappers
    pub fn emit_result<T: ToValue>(&self, signal_name: &str, val: T) {
        self.emit_by_name::<()>(
            signal_name,
            &[
                &val
            ]
        )
    }

    pub fn emit_boxed_result<T: 'static>(&self, signal_name: &str, to_box: T) {
        // T must be owned or static
        self.emit_by_name::<()>(signal_name, &[
            &BoxedAnyObject::new(to_box)
        ]);
    }
}
