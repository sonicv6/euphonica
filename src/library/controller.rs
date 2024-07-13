use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    vec::Vec,
    path::PathBuf
};
use async_channel::Sender;
use crate::{
    common::{Album, AlbumInfo},
    client::albumart::{AlbumArtCache, strip_filename_linux},
    client::MpdMessage
};
use gtk::{
    glib,
    gio,
    prelude::*,
};
use gtk::gdk::Texture;
use adw::subclass::prelude::*;

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecObject,
        ParamSpecString,
        ParamSpecUInt,
        ParamSpecUInt64,
        ParamSpecDouble,
        ParamSpecEnum
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Debug)]
    pub struct Library {
        pub sender: RefCell<Option<Sender<MpdMessage>>>,
        // Each view gets their own list.
        //
        // Album retrieval routine:
        // 1. Library sends request for albums to wrapper
        // 2. Wrapper forwards request to background client
        // 3. Background client gets list of unique album tags
        // 4. For each album tag:
        // 4.1. Get the first song with that tag
        // 4.2. Infer folder_uri, sound quality, albumartist, etc. & pack into AlbumInfo class.
        // 4.3. Send AlbumInfo class to main thread via MpdMessage.
        // 4.4. Wrapper tells Library controller to create an Album GObject with that AlbumInfo &
        // append to the list store.
        pub albums: RefCell<gio::ListStore>,
        pub albumart: RefCell<Option<Rc<AlbumArtCache>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Library {
        const NAME: &'static str = "SlamprustLibrary";
        type Type = super::Library;

        fn new() -> Self {
            let albums = RefCell::new(gio::ListStore::new::<Album>());
            Self {
                sender: RefCell::new(None),
                albums,
                albumart: RefCell::new(None)
            }
        }
    }

    impl ObjectImpl for Library {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecObject::builder::<gio::ListStore>("albums").read_only().build()
                    // ParamSpecObject::builder::<gdk::Texture>("cover")
                    //     .read_only()
                    //     .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "albums" => obj.albums().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Library(ObjectSubclass<imp::Library>);
}

impl Default for Library {
    fn default() -> Self {
        glib::Object::new()
    }
}


impl Library {
    pub fn setup(&self, sender: Sender<MpdMessage>, albumart: Rc<AlbumArtCache>) {
        self.imp().albumart.replace(Some(albumart));
        self.imp().sender.replace(Some(sender));
    }

    pub fn albums(&self) -> gio::ListStore {
        self.imp().albums.borrow().clone()
    }

    pub fn add_album_info(&self, info: AlbumInfo) {
        println!("Adding album: {:?}", info);
        let album = Album::from_info(info);
        let folder_uri = album.get_uri();
        if let Some(albumart) = self.imp().albumart.borrow().as_ref() {
            if let Some(sender) = self.imp().sender.borrow().as_ref() {
                if !albumart.get_path_for(&folder_uri).exists() {
                    println!("Albumart not locally available, will download");
                    let _ = sender.send_blocking(MpdMessage::AlbumArt(folder_uri.to_owned()));
                }
            }
        }
        self.imp().albums.borrow().append(
            &album
        );
    }
}
