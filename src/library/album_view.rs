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

use glib::{
    clone,
    closure_local
};

use super::{
    Library,
    AlbumCell,
    AlbumContentView
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
        pub nav_view: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>,
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

    pub fn setup(&self, library: Rc<Library>, albumart: Rc<AlbumArtCache>) {
        self.setup_gridview(library.clone(), albumart);
        self.bind_state(library);
    }

    pub fn bind_state(&self, library: Rc<Library>) {
        // Here we will listen to the album-clicked signal of Library.
        // Upon receiving that signal, create a new AlbumContentView page and push it onto the stack.
        // The view (AlbumView):

        // - Upon receiving click signal, get the list item at the indicated activate index.
        // - Extract album from that list item.
        // - Extract a non-GObject AlbumInfo sub-struct from that album object (implemented as GObject).
        // - Call a method of the Library controller, passing that AlbumInfo struct.
        // The controller (Library):
        // - When called with that AlbumInfoStruct, send that AlbumInfo to client wrapper via MpdMessage.
        //   This is why we had to extract the AlbumInfo struct out instead of sending the whole Album object:
        //   GObjects are not thread-safe, and while this action is not multithreaded, the MpdMessage enum
        //   has to remain thread safe as a whole since we're also using it to send results from the child
        //   client back to the main one. As such, the MpdMessage enum cannot carry any GObject in any of
        //   its variants, not just the variants used by child threads.
        // - Client fetches all songs with album tag matching given name in AlbumInfo.
        // - Client replies by calling another method of the Library controller & passing the list of songs
        //   it received, since the Library controller did not directly call any method of the client
        //   (it used a message instead) and as such cannot receive results in the normal return-value way.
        // Back to controller (Library):
        // - Upon being called by client wrapper with that list of songs, reconstruct the album GObject,
        //   construct a gio::ListStore of those Songs, then send them both over a custom signal. The
        //   reason we're back to albums instead of AlbumInfos is that signal parameters must be GObjects
        //   (or sth implementing glib::ToValue trait).
        // Back to the view (AlbumView):
        // - Listen to that custom signal. Upon that signal triggering, construct an AlbumContentView,
        //   populate it with the songs, then push it to the NavigationView inside AlbumView.
        let this = self.clone();
        library.connect_closure(
            "album-clicked",
            false,
            closure_local!(move |_: Library, album: Album, song_list: gio::ListStore| {
                let content_view = AlbumContentView::new(album, song_list);
                let content_page = adw::NavigationPage::builder()
                    .child(&content_view)
                    .title("Album Info")
                    .build();
                this.imp().nav_view.push(&content_page);
            })
        );
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

        // Setup click action
        self.imp().grid_view.connect_activate(move |grid_view, position| {
            // Get `IntegerObject` from model
            let model = grid_view.model().expect("The model has to exist.");
            let album = model
                .item(position)
                .and_downcast::<Album>()
                .expect("The item has to be a `common::Album`.");

            // Increase "number" of `IntegerObject`
            library.on_album_clicked(album);
        });
    }
}
