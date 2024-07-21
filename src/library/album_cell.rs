use std::{
    cell::RefCell,
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

use crate::common::Album;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/album-cell.ui")]
    pub struct AlbumCell {
        #[template_child]
        pub cover: TemplateChild<Image>,  // Use high-resolution version
        #[template_child]
        pub title: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Album GObject
        pub bindings: RefCell<Vec<Binding>>,
        pub cover_signal_id: RefCell<Option<SignalHandlerId>>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for AlbumCell {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaAlbumCell";
        type Type = super::AlbumCell;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for AlbumCell {}

    // Trait shared by all widgets
    impl WidgetImpl for AlbumCell {}

    // Trait shared by all boxes
    impl BoxImpl for AlbumCell {}
}

glib::wrapper! {
    pub struct AlbumCell(ObjectSubclass<imp::AlbumCell>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for AlbumCell {
    fn default() -> Self {
        Self::new()
    }
}

impl AlbumCell {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn bind(&self, album: &Album) {
        // Get state
        let cover_image = self.imp().cover.get();
        let title_label = self.imp().title.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        // Set once first (like sync_create)
        let cover = album.get_cover();

        if cover.is_some() {
            // Don't set to None as that will still override the placeholder resource.
            cover_image.set_paintable(cover.as_ref());
        }
        let cover_binding = album
            .connect_notify_local(
                Some("cover"),
                move |this_album, _| {
                    let cover = this_album.get_cover();
                    if cover.is_some() {
                        // Don't set to None as that will still override the placeholder resource.
                        cover_image.set_paintable(cover.as_ref());
                    }
                },
            );
        self.imp().cover_signal_id.replace(Some(cover_binding));

        let title_binding = album
            .bind_property("title", &title_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(title_binding);
    }

    pub fn unbind(&self, album: &Album) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().cover_signal_id.take() {
            album.disconnect(id);
        }
    }
}
