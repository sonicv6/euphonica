use std::{
    cell::{Cell, RefCell},
    rc::Rc
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    Label,
    Image
};
use glib::{
    Object,
    Binding,
    signal::SignalHandlerId
};

use crate::common::Song;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/album-song-row.ui")]
    pub struct AlbumSongRow {
        #[template_child]
        pub track_index: TemplateChild<Label>,
        #[template_child]
        pub song_name: TemplateChild<Label>,
        // Compilation/best-of albums usually have different artists.
        #[template_child]
        pub artist_name: TemplateChild<Label>,
        #[template_child]
        pub duration: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Song GObject
        pub bindings: RefCell<Vec<Binding>>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for AlbumSongRow {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "SlamprustAlbumSongRow";
        type Type = super::AlbumSongRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for AlbumSongRow {}

    // Trait shared by all widgets
    impl WidgetImpl for AlbumSongRow {}

    // Trait shared by all boxes
    impl BoxImpl for AlbumSongRow {}
}

glib::wrapper! {
    pub struct AlbumSongRow(ObjectSubclass<imp::AlbumSongRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for AlbumSongRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AlbumSongRow {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn bind(&self, song: &Song) {
        // Get state
        let song_name_label = self.imp().song_name.get();
        let artist_name_label = self.imp().artist_name.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        let song_name_binding = song
            .bind_property("name", &song_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_name_binding);

        let artist_name_binding = song
            .bind_property("artist", &artist_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(artist_name_binding);
    }

    pub fn unbind(&self) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
    }
}
