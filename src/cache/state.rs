use std::{
    cell::Cell,
    sync::OnceLock
};
use gtk::{
    glib,
    gdk::Texture
};
use glib::{
    prelude::*,
    subclass::{
        prelude::*,
        Signal
    },
    BoxedAnyObject
};

use crate::common::Album;

mod imp {
    // use glib::{
    //     ParamSpec,
    //     ParamSpecBoolean,
    //     ParamSpecEnum
    // };
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct CacheState {}

    #[glib::object_subclass]
    impl ObjectSubclass for CacheState {
        const NAME: &'static str = "EuphoniaCacheState";
        type Type = super::CacheState;
    }

    impl ObjectImpl for CacheState {
        // fn properties() -> &'static [ParamSpec] {
        //     static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
        //         vec![
        //             ParamSpecBoolean::builder("busy").read_only().build(),
        //             ParamSpecEnum::builder::<ConnectionState>("connection-state").read_only().build()
        //         ]
        //     });
        //     PROPERTIES.as_ref()
        // }

        // fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
        //     let obj = self.obj();
        //     match pspec.name() {
        //         "connection-state" => obj.get_connection_state().to_value(),
        //         "busy" => obj.is_busy().to_value(),
        //         _ => unimplemented!(),
        //     }
        // }

        // fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        //     let obj = self.obj();
        //     match pspec.name() {
        //         "connection-state" => {
        //             let state = value.get().expect("Error in CacheState::set_property");
        //             obj.set_connection_state(state);
        //         },
        //         _ => unimplemented!()
        //     }
        // }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Only emitted when a new album art becomes locally available.
                    Signal::builder("album-art-downloaded")
                        .param_types([
                            String::static_type()  // folder URI
                        ])
                        .build(),
                    Signal::builder("album-info-downloaded")
                        .param_types([
                            String::static_type()  // album tag
                        ])
                        .build(),
                    Signal::builder("artist-info-downloaded")
                        .param_types([
                            String::static_type()  // artist tag
                        ])
                        .build()
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct CacheState(ObjectSubclass<imp::CacheState>);
}

impl Default for CacheState {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl CacheState {
    // Convenience emit wrapper
    pub fn emit_with_param(&self, name: &str, tag: &str) {
        self.emit_by_name::<()>(
            name,
            &[
                &tag
            ]
        );
    }
}
