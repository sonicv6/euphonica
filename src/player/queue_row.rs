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
    clone,
    signal::SignalHandlerId
};

use crate::common::Song;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/queue-row.ui")]
    pub struct QueueRow {
        #[template_child]
        pub thumbnail: TemplateChild<Image>,
        #[template_child]
        pub song_name: TemplateChild<Label>,
        #[template_child]
        pub playing_indicator: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Song GObject
        pub bindings: RefCell<Vec<Binding>>,
        pub thumbnail_signal_id: RefCell<Option<SignalHandlerId>>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for QueueRow {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "SlamprustQueueRow";
        type Type = super::QueueRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for QueueRow {}

    // Trait shared by all widgets
    impl WidgetImpl for QueueRow {}

    // Trait shared by all boxes
    impl BoxImpl for QueueRow {}
}

glib::wrapper! {
    pub struct QueueRow(ObjectSubclass<imp::QueueRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}


impl Default for QueueRow {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueRow {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn bind(&self, song: &Song) {
        // Get state
        let thumbnail_image = self.imp().thumbnail.get();
        let song_name_label = self.imp().song_name.get();
        let playing_label = self.imp().playing_indicator.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        // let thumbnail_path_binding = song
        //     .bind_property("thumbnail-path", &thumbnail_image, "file")
        //     .sync_create()
        //     .build();
        // bindings.push(thumbnail_path_binding);

        let thumbnail_path_binding = song
            .connect_notify_local(
                Some("thumbnail-path"),
                move |this_song, _| {
                    thumbnail_image.set_from_file(this_song.get_cover_path(true));
                },
            );
        self.imp().thumbnail_signal_id.replace(Some(thumbnail_path_binding));

        let song_name_binding = song
            .bind_property("name", &song_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_name_binding);

        let song_is_playing_binding = song
            .bind_property("is-playing", &playing_label, "visible")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_is_playing_binding);
    }

    pub fn unbind(&self, song: &Song) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().thumbnail_signal_id.take() {
            song.disconnect(id);
        }
    }
}