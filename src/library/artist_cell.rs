use std::{
    cell::RefCell,
    rc::Rc
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate
};
use adw;
use glib::{
    closure_local,
    Object,
    signal::SignalHandlerId
};

use crate::{
    common::Artist,
    cache::{
        Cache,
        CacheState
    }
};

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecString
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/library/artist-cell.ui")]
    pub struct ArtistCell {
        #[template_child]
        pub avatar: TemplateChild<adw::Avatar>,  // Use high-resolution version
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        pub avatar_signal_id: RefCell<Option<SignalHandlerId>>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for ArtistCell {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaArtistCell";
        type Type = super::ArtistCell;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ArtistCell {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("name").build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "name" => obj.get_name().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "name" => {
                    if let Ok(name) = value.get::<&str>() {
                        self.name.set_label(name);
                    }
                    obj.notify("name");
                }
                _ => unimplemented!()
            }
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for ArtistCell {}

    // Trait shared by all boxes
    impl BoxImpl for ArtistCell {}
}

glib::wrapper! {
    pub struct ArtistCell(ObjectSubclass<imp::ArtistCell>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for ArtistCell {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtistCell {
    pub fn new() -> Self {
        Object::builder().build()
    }

    fn update_artist_avatar(&self, tag: &str, cache: Rc<Cache>) {
        self.imp().avatar.set_custom_image(
            cache.load_local_artist_avatar(tag, false).as_ref()
        );
    }

    pub fn get_name(&self) -> glib::GString {
        self.imp().name.label()
    }

    pub fn set_name(&self, name: &str) {
        self.imp().name.set_label(name);
    }

    pub fn bind(&self, artist: &Artist, cache: Rc<Cache>) {
        // Get state
        // Set once first (like sync_create)
        self.update_artist_avatar(artist.get_name(), cache.clone());
        let avatar_binding = cache.get_cache_state().connect_closure(
            "artist-avatar-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                artist,
                #[weak]
                cache,
                move |_: CacheState, tag: String| {
                    if artist.get_name() == tag {
                        this.update_artist_avatar(&tag, cache)
                    }
                }
            )
        );
        self.imp().avatar_signal_id.replace(Some(avatar_binding));
    }

    pub fn unbind(&self, cache: Rc<Cache>) {
        // Stop listening to cache (not displaying anything right now)
        if let Some(id) = self.imp().avatar_signal_id.take() {
            cache.get_cache_state().disconnect(id);
        }
    }
}
