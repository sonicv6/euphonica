use std::rc::Rc;
use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib,
    CompositeTemplate,
    SingleSelection,
    SignalListItemFactory,
    ListItem,
};
use glib::clone;

use super::{
    Library,
    AlbumCell
};
use crate::{
    common::Album, client::albumart::AlbumArtCache
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/album-view.ui")]
    pub struct AlbumView {
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumView {
        const NAME: &'static str = "SlamprustAlbumView";
        type Type = super::AlbumView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_layout_manager_type::<gtk::BinLayout>();
            // klass.set_css_name("albumview");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AlbumView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for AlbumView {}
}

glib::wrapper! {
    pub struct AlbumView(ObjectSubclass<imp::AlbumView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for AlbumView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl AlbumView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup_gridview(&self, library: Rc<Library>, albumart: Rc<AlbumArtCache>) {
        // Set selection mode
        // TODO: Click to enter album
        let sel_model = SingleSelection::new(Some(library.albums()));
        self.imp().grid_view.set_model(Some(&sel_model));

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `AlbumCell` during setup
        factory.connect_setup(move |_, list_item| {
            let album_cell = AlbumCell::new();
            list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .set_child(Some(&album_cell));
        });
        // Tell factory how to bind `AlbumCell` to one of our Album GObjects
        factory.connect_bind(clone!(@weak albumart as cache => move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Album = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Album>()
                .expect("The item has to be a common::Album.");

            // This album is about to be displayed. Cache its album art (if any) now.
            // Might result in a cache miss, in which case the file will be immediately loaded
            // from disk.
            // Note that this does not trigger any downloading. That's done by the Player
            // controller upon receiving queue updates.
            // Note 2: Album GObjects contain folder-level URIs, so there is no need to strip filename.
            if item.get_cover().is_none() {
                if let Some(tex) = albumart.get_for(&item.get_uri(), false) {
                    item.set_cover(Some(tex));
                }
            }

            // Get `AlbumCell` from `ListItem` (the UI widget)
            let child: AlbumCell = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumCell>()
                .expect("The child has to be an `AlbumCell`.");

            // Within this binding fn is where the cached album art texture gets used.
            child.bind(&item);
        }));


        // When cell goes out of sight, unbind from item to allow reuse with another.
        // Remember to also unset the thumbnail widget's texture to potentially free it from memory.
        factory.connect_unbind(move |_, list_item| {
            // Get `AlbumCell` from `ListItem` (the UI widget)
            let child: AlbumCell = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumCell>()
                .expect("The child has to be an `AlbumCell`.");
            let item: Album = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Album>()
                .expect("The item has to be a common::Album.");
            child.unbind(&item);
        });

        // Set the factory of the list view
        self.imp().grid_view.set_factory(Some(&factory));
    }
}
