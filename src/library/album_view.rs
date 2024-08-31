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

use glib::clone;

use super::{
    Library,
    AlbumCell,
    AlbumContentView
};
use crate::{
    common::Album,
    cache::Cache,
    client::ClientState,
    utils::{settings_manager, g_cmp_str_options, g_cmp_options, g_search_substr}
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/library/album-view.ui")]
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

    pub fn setup(&self, library: Library, cache: Rc<Cache>, client_state: ClientState) {
        self.setup_sort();
        self.setup_search();
        self.setup_gridview(library.clone(), cache.clone());

        let content_view = self.imp().content_view.get();
        content_view.setup(library.clone(), cache, client_state);
        self.imp().content_page.connect_hidden(move |_| {
            content_view.unbind();
        });
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
                    "album-title" => Some("Title".to_value()),
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
                                Some(album1.get_title()),
                                Some(album2.get_title()),
                                nulls_first,
                                asc,
                                case_sensitive
                            )
                        }
                        4 => {
                            // AlbumArtist
                            g_cmp_str_options(
                                album1.get_artist_str().as_deref(),
                                album2.get_artist_str().as_deref(),
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
        let settings = settings_manager();
        let library_settings = settings.child("library");
        // Set up search filter
        self.imp().search_filter.set_filter_func(
            clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                library_settings,
                #[upgrade_or]
                true,
                move |obj| {
                    let album = obj
                        .downcast_ref::<Album>()
                        .expect("Search obj has to be a common::Album.");

                    let search_term = this.imp().search_entry.text();
                    if search_term.is_empty() {
                        return true;
                    }

                    // Should the searching be case-sensitive?
                    let case_sensitive = library_settings.boolean("search-case-sensitive");
                    // Vary behaviour depending on dropdown
                    match this.imp().search_mode.selected() {
                        // Keep these indices in sync with the GtkStringList in the UI file
                        0 => {
                            // Match either album title or AlbumArtist (not artist tag)
                            g_search_substr(
                                Some(album.get_title()),
                                &search_term,
                                case_sensitive
                            ) || g_search_substr(
                                album.get_artist_str().as_deref(),
                                &search_term,
                                case_sensitive
                            )
                        }
                        1 => {
                            // Match only album title
                            g_search_substr(
                                Some(album.get_title()),
                                &search_term,
                                case_sensitive
                            )
                        }
                        2 => {
                            // Match only AlbumArtist (albums without such tag will never match)
                            g_search_substr(
                                album.get_artist_str().as_deref(),
                                &search_term,
                                case_sensitive
                            )
                        }
                        _ => true
                    }
                }
            )
        );

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

    fn on_album_clicked(&self, album: Album, library: Library) {
        // - Upon receiving click signal, get the list item at the indicated activate index.
        // - Extract album from that list item.
        // - Bind AlbumContentView to that album. This will cause the AlbumContentView to start listening
        //   to the cache & client (MpdWrapper) states for arrival of album arts, contents & metadata.
        // - Try to ensure existence of local metadata by queuing download if necessary. Since
        //   AlbumContentView is now listening to the relevant signals, it will immediately update itself
        //   in an asynchronous manner.
        // - Schedule client to fetch all songs with this album tag in the same manner.
        // - Now we can push the AlbumContentView. At this point, it must already have been bound to at
        //   least the album's basic information (title, artist, etc). If we're lucky, it might also have
        //   its song list and wiki initialised, but that's not mandatory.
        // NOTE: We do not ensure local album art again in the above steps, since we have already done so
        // once when adding this album to the ListStore for the GridView.
        let content_view = self.imp().content_view.get();
        content_view.bind(album.clone());
        library.init_album(album);
        self.imp().nav_view.push_by_tag("content");
    }

    fn setup_gridview(&self, library: Library, cache: Rc<Cache>) {
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

        // Tell factory how to bind `AlbumCell` to one of our Album GObjects.
        // If this cell is being bound to an album, that means it might be displayed.
        // As such, we'll also make it listen to the cache controller for any new
        // album art downloads. This ensures we will never have to iterate through
        // the entire grid to update album arts (only visible or nearly visible cells
        // will be updated, thus yielding a constant update cost).
        factory.connect_bind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
                // Get `Song` from `ListItem` (that is, the data side)
                let item: Album = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .item()
                    .and_downcast::<Album>()
                    .expect("The item has to be a common::Album.");

                // This album cell is about to be displayed. Try to ensure that we
                // have a local copy of its album art from MPD.
                // No need to dedupe since we're guaranteed that the same album never
                // appears twice in the GridView anyway.
                cache.ensure_local_album_art(item.get_info());

                // Get `AlbumCell` from `ListItem` (the UI widget)
                let child: AlbumCell = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<AlbumCell>()
                    .expect("The child has to be an `AlbumCell`.");

                // Within this binding fn is where the cached album art texture gets used.
                child.bind(&item, cache);
            })
        );


        // When cell goes out of sight, unbind from item to allow reuse with another.
        // Remember to also unset the thumbnail widget's texture to potentially free it from memory.
        factory.connect_unbind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
                // Get `AlbumCell` from `ListItem` (the UI widget)
                let child: AlbumCell = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<AlbumCell>()
                    .expect("The child has to be an `AlbumCell`.");
                // Un-listen to cache, so that we don't update album art for cells that are not in view
                child.unbind(cache);
            })
        );

        // Set the factory of the list view
        self.imp().grid_view.set_factory(Some(&factory));

        // Setup click action
        self.imp().grid_view.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            library,
            move |grid_view, position| {
                let model = grid_view.model().expect("The model has to exist.");
                let album = model
                    .item(position)
                    .and_downcast::<Album>()
                    .expect("The item has to be a `common::Album`.");
                println!("Clicked on {:?}", &album);
                this.on_album_clicked(album, library);
            })
        );
    }
}
