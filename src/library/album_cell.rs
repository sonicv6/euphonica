use std::{
    cell::RefCell,
    rc::Rc
};
use gtk::{
    glib,
    gdk,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    Label,
    Image
};
use glib::{
    closure_local,
    Object,
    Binding,
    signal::SignalHandlerId
};

use crate::{
    cache::{
        placeholders::ALBUMART_PLACEHOLDER, Cache, CacheState
    }, common::{
        Album, AlbumInfo, QualityGrade
    }
};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/library/album-cell.ui")]
    pub struct AlbumCell {
        #[template_child]
        pub cover: TemplateChild<gtk::Picture>,  // Use high-resolution version
        #[template_child]
        pub title: TemplateChild<Label>,
        #[template_child]
        pub artist: TemplateChild<Label>,
        #[template_child]
        pub quality_grade: TemplateChild<Image>,
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

    fn update_album_art(&self, info: &AlbumInfo, cache: Rc<Cache>) {
        if let Some(tex) = cache.load_local_album_art(info, false) {
            self.imp().cover.set_paintable(Some(&tex));
        }
        else {
            self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER))
        }
    }

    pub fn bind(&self, album: &Album, cache: Rc<Cache>) {
        // Get state
        let title_label = self.imp().title.get();
        let artist_label = self.imp().artist.get();
        let quality_grade = self.imp().quality_grade.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        // Set once first (like sync_create)
        self.update_album_art(album.get_info(), cache.clone());
        let cover_binding = cache.get_cache_state().connect_closure(
            "album-art-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                album,
                #[weak]
                cache,
                move |_: CacheState, folder_uri: String| {
                    if album.get_uri() == folder_uri {
                        this.update_album_art(album.get_info(), cache)
                    }
                }
            )
        );
        self.imp().cover_signal_id.replace(Some(cover_binding));

        let title_binding = album
            .bind_property("title", &title_label, "label")
            .transform_to(|_, val: Option<String>| {
                if val.is_some() {
                    return val;
                }
                Some("(untagged)".to_string())
            })
            .sync_create()
            .build();
        // Save binding
        bindings.push(title_binding);

        let artist_binding = album
            .bind_property("artist", &artist_label, "label")
            .transform_to(|_, val: Option<String>| {
                if val.is_some() {
                    return val;
                }
                Some("Unknown Artist".to_string())
            })
            .sync_create()
            .build();
        // Save binding
        bindings.push(artist_binding);

        let quality_grade_binding = album
            .bind_property(
                "quality-grade",
                &quality_grade,
                "icon-name"
            )
            .transform_to(|_, grade: QualityGrade| {
                Some(grade.to_icon_name())}
            )
            .sync_create()
            .build();
        // Save binding
        bindings.push(quality_grade_binding);
    }

    pub fn unbind(&self, cache: Rc<Cache>) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().cover_signal_id.take() {
            cache.get_cache_state().disconnect(id);
        }
    }
}
