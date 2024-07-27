use std::{
    rc::Rc,
    cell::Cell,
    cmp::Ordering
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{
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
    common::Album,
    client::albumart::AlbumArtCache,
    utils::{settings_manager, g_cmp_str_options, g_cmp_options}
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/album-view.ui")]
    pub struct AlbumView {
        #[template_child]
        pub nav_view: TemplateChild<adw::NavigationView>,

        // Search & filter widgets
        #[template_child]
        pub sort_dir: TemplateChild<gtk::Image>,
        #[template_child]
        pub sort_mode: TemplateChild<gtk::Label>,
        #[template_child]
        pub search_btn: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub search_mode: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        // Content
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub content_view: TemplateChild<AlbumContentView>,

        // Search & filter models
        pub search_filter: gtk::CustomFilter,
        pub sorter: gtk::CustomSorter,
        // Keep last length to optimise search
        // If search term is now longer, only further filter still-matching
        // items.
        // If search term is now shorter, only check non-matching items to see
        // if they now match.
        pub last_search_len: Cell<usize>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumView {
        const NAME: &'static str = "EuphoniaAlbumView";
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
        Self::new()
    }
}

impl AlbumView {
    pub fn new() -> Self {
        let res: Self = glib::Object::new();

        res
    }

    pub fn setup(&self, library: Library, albumart: Rc<AlbumArtCache>) {
        self.setup_sort();
        self.setup_search();
        self.setup_gridview(library.clone(), albumart);

        let content_view = self.imp().content_view.get();
        content_view.setup(library.clone());
        self.imp().content_page.connect_hidden(move |_| {
            content_view.unbind();
            content_view.clear_content();
        });
        self.bind_state(library);
    }

    fn setup_sort(&self) {
        // TODO: use albumsort & albumartistsort tags where available
        // Setup sort widget & actions
        let settings = settings_manager();
        let state = settings.child("state").child("albumview");
        let library_settings = settings.child("library");
        let actions = gio::SimpleActionGroup::new();
        actions.add_action(
            &state.create_action("sort-by")
        );
        actions.add_action(
            &state.create_action("sort-direction")
        );
        self.insert_action_group("albumview", Some(&actions));
        let sort_dir = self.imp().sort_dir.get();
        state
            .bind(
                "sort-direction",
                &sort_dir,
                "icon-name"
            )
            .get_only()
            .mapping(|dir, _| {
                match dir.get::<String>().unwrap().as_ref() {
                    "asc" => Some("view-sort-ascending-symbolic".to_value()),
                    _ => Some("view-sort-descending-symbolic".to_value())
                }
            })
            .build();
        let sort_mode = self.imp().sort_mode.get();
        state
            .bind(
                "sort-by",
                &sort_mode,
                "label",
            )
            .get_only()
            .mapping(|val, _| {
                // TODO: i18n
                match val.get::<String>().unwrap().as_ref() {
                    "album-title" => Some("Album title".to_value()),
                    "album-artist" => Some("AlbumArtist".to_value()),
                    "release-date" => Some("Release date".to_value()),
                    _ => unreachable!()
                }
            })
            .build();
        self.imp().sorter.set_sort_func(
            clone!(
                #[strong]
                library_settings,
                #[strong]
                state,
                move |obj1, obj2| {
                    let album1 = obj1
                        .downcast_ref::<Album>()
                        .expect("Sort obj has to be a common::Album.");

                    let album2 = obj2
                        .downcast_ref::<Album>()
                        .expect("Sort obj has to be a common::Album.");

                    // Should we sort ascending?
                    let asc = state.enum_("sort-direction") > 0;
                    // Should the sorting be case-sensitive, i.e. uppercase goes first?
                    let case_sensitive = library_settings.boolean("sort-case-sensitive");
                    // Should nulls be put first or last?
                    let nulls_first = library_settings.boolean("sort-nulls-first");

                    // Vary behaviour depending on sort menu
                    match state.enum_("sort-by") {
                        // Refer to the org.euphonia.Euphonia.sortby enum the gschema
                        3 => {
                            // Album title
                            g_cmp_str_options(
                                Some(album1.get_title()).as_deref(),
                                Some(album2.get_title()).as_deref(),
                                nulls_first,
                                asc,
                                case_sensitive
                            )
                        }
                        4 => {
                            // AlbumArtist
                            g_cmp_str_options(
                                album1.get_artist().as_deref(),
                                album2.get_artist().as_deref(),
                                nulls_first,
                                asc,
                                case_sensitive
                            )
                        }
                        5 => {
                            // Release date
                            g_cmp_options(
                                album1.get_release_date().as_ref(),
                                album2.get_release_date().as_ref(),
                                nulls_first,
                                asc
                            )
                        }
                        _ => unreachable!()
                    }
                }
            )
        );

        // Update when changing sort settings
        state.connect_changed(
            Some("sort-by"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| {
                    println!("Updating sort...");
                    this.imp().sorter.changed(gtk::SorterChange::Different);
                }
            )
        );
        state.connect_changed(
            Some("sort-direction"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| {
                    println!("Flipping sort...");
                    // Don't actually sort, just flip the results :)
                    this.imp().sorter.changed(gtk::SorterChange::Inverted);
                }
            )
        );
    }

    fn setup_search(&self) {
        // Set up search filter
        self.imp().search_filter.set_filter_func(
            clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                false,
                move |obj| {
                    let album = obj
                        .downcast_ref::<Album>()
                        .expect("Search obj has to be a common::Album.");

                    let search_term = this.imp().search_entry.text().to_lowercase();
                    if search_term.is_empty() {
                        return true;
                    }

                    // Vary behaviour depending on dropdown
                    match this.imp().search_mode.selected() {
                        // Keep these indices in sync with the GtkStringList in the UI file
                        0 => {
                            // Match either album title or AlbumArtist (not artist tag)
                            if album.get_title().to_lowercase().contains(&search_term) {
                                return true;
                            }
                            if let Some(artist) = album.get_artist() {
                                return artist
                                    .to_lowercase()
                                    .contains(&search_term);
                            }
                            false
                        }
                        1 => {
                            // Match only album title
                            album.get_title().to_lowercase().contains(&search_term)
                        }
                        2 => {
                            // Match only AlbumArtist (albums without such tag will never match)
                            if let Some(artist) = album.get_artist() {
                                return artist
                                    .to_lowercase()
                                    .contains(&search_term);
                            }
                            false
                        }
                        _ => unreachable!()
                    }
                }
            )
        );

        // TODO: Maybe let user choose case-sensitivity too (too verbose?)
        // Connect search entry to filter. Filter will later be put in GtkSearchModel.
        // That GtkSearchModel will listen to the filter's changed signal.
        let search_entry = self.imp().search_entry.get();
        search_entry.connect_search_changed(
            clone!(
                #[weak(rename_to = this)]
                self,
                move |entry| {
                    let text = entry.text();
                    let new_len = text.len();
                    let old_len = this.imp().last_search_len.replace(new_len);
                    match new_len.cmp(&old_len) {
                        Ordering::Greater => {
                            this.imp().search_filter.changed(gtk::FilterChange::MoreStrict);
                        }
                        Ordering::Less => {
                            this.imp().search_filter.changed(gtk::FilterChange::LessStrict);
                        }
                        Ordering::Equal => {
                            this.imp().search_filter.changed(gtk::FilterChange::Different);
                        }
                    }
                }
            )
        );

        let search_mode = self.imp().search_mode.get();
        search_mode.connect_notify_local(
            Some("selected"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, _| {
                    println!("Changed search mode");
                    this.imp().search_filter.changed(gtk::FilterChange::Different);
                }
            )
        );
    }

    fn bind_state(&self, library: Library) {
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
                let content_view = this.imp().content_view.get();
                content_view.set_album(album, song_list);
                this.imp().nav_view.push_by_tag("content");
            })
        );
    }

    fn setup_gridview(&self, library: Library, albumart: Rc<AlbumArtCache>) {
        // Setup search bar
        let search_bar = self.imp().search_bar.get();
        let search_entry = self.imp().search_entry.get();
        search_bar.connect_entry(&search_entry);

        let search_btn = self.imp().search_btn.get();
        search_btn
            .bind_property(
                "active",
                &search_bar,
                "search-mode-enabled"
            )
            .sync_create()
            .build();

        // Chain search & sort. Put sort after search to reduce number of sort items.
        let search_model = gtk::FilterListModel::new(Some(library.albums()), Some(self.imp().search_filter.clone()));
        search_model.set_incremental(true);
        let sort_model = gtk::SortListModel::new(Some(search_model), Some(self.imp().sorter.clone()));
        sort_model.set_incremental(true);
        let sel_model = SingleSelection::new(Some(sort_model));

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
        factory.connect_bind(
            clone!(
                #[weak]
                albumart,
                move |_, list_item| {
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
