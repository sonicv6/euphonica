use std::cell::Cell;
use gtk::glib;
use glib::{
    prelude::*,
    subclass::prelude::*
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "EuphoniaConnectionState")]
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
        ParamSpecEnum
    };
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct ClientState {
        pub connection_state: Cell<ConnectionState>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClientState {
        const NAME: &'static str = "EuphoniaClientState";
        type Type = super::ClientState;
    }

    impl ObjectImpl for ClientState {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecEnum::builder::<ConnectionState>("connection-state").build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "connection-state" => obj.get_connection_state().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "connection-state" => {
                    let state = value.get().expect("Error in ClientState::set_property");
                    obj.set_connection_state(state);
                },
                _ => unimplemented!()
            }
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

    pub fn set_connection_state(&self, new_state: ConnectionState) {
        let old_state = self.imp().connection_state.replace(new_state);
        if old_state != new_state {
            self.notify("connection-state");
        }
    }
}
