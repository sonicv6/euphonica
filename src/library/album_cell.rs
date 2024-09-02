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
    use glib::{
        ParamSpec,
        ParamSpecString,
        ParamSpecEnum
    };
    use once_cell::sync::Lazy;
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
    impl ObjectImpl for AlbumCell {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("title").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecString::builder("quality-grade").build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            match pspec.name() {
                "title" => self.title.label().to_value(),
                "artist" => self.artist.label().to_value(),
                "quality-grade" => self.quality_grade.icon_name().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "title" => {
                    if let Ok(title) = value.get::<&str>() {
                        self.title.set_label(title);
                        obj.notify("title");
                    }
                }
                "artist" => {
                    if let Ok(artist) = value.get::<&str>() {
                        self.artist.set_label(artist);
                        obj.notify("artist");
                    }
                }
                "quality-grade" => {
                    if let Ok(icon_name) = value.get::<&str>() {
                        self.quality_grade.set_icon_name(Some(icon_name));
                        self.quality_grade.set_visible(true);
                    }
                    else {
                        self.quality_grade.set_icon_name(None);
                        self.quality_grade.set_visible(false);
                    }
                }
                _ => unimplemented!()
            }
        }
    }

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

impl AlbumCell {
    pub fn new(item: &gtk::ListItem) -> Self {
        let res: Self = Object::builder().build();
        res.setup(item);
        res
    }

    #[inline(always)]
    pub fn setup(&self, item: &gtk::ListItem) {
        item
            .property_expression("item")
            .chain_property::<Album>("title")
            .bind(self, "title", gtk::Widget::NONE);

        item
            .property_expression("item")
            .chain_property::<Album>("artist")
            .bind(self, "artist", gtk::Widget::NONE);

        item
            .property_expression("item")
            .chain_property::<Album>("quality-grade")
            .bind(self, "quality-grade", gtk::Widget::NONE);
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
        // The string properties are bound using property expressions in setup().
        // Here we only need to manually bind to the cache controller to fetch album art.
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
    }

    pub fn unbind(&self, cache: Rc<Cache>) {
        if let Some(id) = self.imp().cover_signal_id.take() {
            cache.get_cache_state().disconnect(id);
        }
    }
}
