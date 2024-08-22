use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use time::{Date, format_description};
use adw::subclass::prelude::*;
use gtk::{
    gdk,
    prelude::*,
    gio,
    glib,
    CompositeTemplate,
    SignalListItemFactory,
    ListItem,
};
use glib::{
    clone,
    closure_local,
    Binding,
    signal::SignalHandlerId
};

use super::{
    Library,
    ArtistSongRow,
    AlbumCell
};
use crate::{
    common::{Artist, Song, Album},
    cache::{
        Cache,
        CacheState
    },
    client::ClientState
};

mod imp {
    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/library/artist-content-view.ui")]
    pub struct ArtistContentView {
        #[template_child]
        pub avatar: TemplateChild<adw::Avatar>,
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
        #[template_child]
        pub song_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub album_count: TemplateChild<gtk::Label>,

        #[template_child]
        pub infobox_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub collapse_infobox: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub bio_box: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub bio_text: TemplateChild<gtk::Label>,
        #[template_child]
        pub bio_link: TemplateChild<gtk::LinkButton>,
        #[template_child]
        pub bio_attrib: TemplateChild<gtk::Label>,
        // #[template_child]
        // pub runtime: TemplateChild<gtk::Label>,

        // All songs sub-view
        #[template_child]
        pub song_subview: TemplateChild<gtk::ListView>,
        pub song_list: gio::ListStore,
        #[template_child]
        pub replace_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub append_queue: TemplateChild<gtk::Button>,

        // Discography sub-view
        #[template_child]
        pub album_subview: TemplateChild<gtk::GridView>,
        pub album_list: gio::ListStore,

        pub artist: RefCell<Option<Artist>>,
        pub bindings: RefCell<Vec<Binding>>,
        pub avatar_signal_id: RefCell<Option<SignalHandlerId>>,
        pub cache: OnceCell<Rc<Cache>>
    }

    impl Default for ArtistContentView {
        fn default() -> Self {
            Self {
                avatar: TemplateChild::default(),
                name: TemplateChild::default(),
                song_count: TemplateChild::default(),
                album_count: TemplateChild::default(),
                infobox_revealer: TemplateChild::default(),
                collapse_infobox: TemplateChild::default(),
                bio_box: TemplateChild::default(),
                bio_text: TemplateChild::default(),
                bio_link: TemplateChild::default(),
                bio_attrib: TemplateChild::default(),
                // runtime: TemplateChild::default(),
                // All songs sub-view
                song_subview: TemplateChild::default(),
                song_list: gio::ListStore::new::<Song>(),
                replace_queue: TemplateChild::default(),
                append_queue: TemplateChild::default(),
                // Discography sub-view
                album_subview: TemplateChild::default(),
                album_list: gio::ListStore::new::<Album>(),
                artist: RefCell::new(None),
                bindings: RefCell::new(Vec::new()),
                avatar_signal_id: RefCell::new(None),
                cache: OnceCell::new()
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ArtistContentView {
        const NAME: &'static str = "EuphoniaArtistContentView";
        type Type = super::ArtistContentView;
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

    impl ObjectImpl for ArtistContentView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for ArtistContentView {}
}

glib::wrapper! {
    pub struct ArtistContentView(ObjectSubclass<imp::ArtistContentView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for ArtistContentView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ArtistContentView {
    fn update_meta(&self, artist: &Artist) {
        let cache = self.imp().cache.get().unwrap().clone();
        let bio_box = self.imp().bio_box.get();
        let bio_text = self.imp().bio_text.get();
        let bio_link = self.imp().bio_link.get();
        let bio_attrib = self.imp().bio_attrib.get();
        if let Some(meta) = cache.load_local_artist_meta(
            artist.get_mbid().as_deref(),
            Some(artist.get_name()).as_deref()
        ) {
            if let Some(bio) = meta.bio {
                bio_box.set_visible(true);
                bio_text.set_label(&bio.content);
                if let Some(url) = bio.url.as_ref() {
                    bio_link.set_visible(true);
                    bio_link.set_uri(url);
                }
                else {
                    bio_link.set_visible(false);
                }
                bio_attrib.set_label(&bio.attribution);
            }
            else {
                bio_box.set_visible(false);
            }
        }
        else {
            bio_box.set_visible(false);
        }
    }

    #[inline(always)]
    fn setup_info_box(&self, cache: Rc<Cache>) {
        cache.get_cache_state().connect_closure(
            "artist-avatar-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: CacheState, name: String| {
                    if let Some(artist) = this.imp().artist.borrow().as_ref() {
                        if name == artist.get_name() {
                            this.update_avatar(&name);
                        }
                    }
                }
            )
        );
        cache.get_cache_state().connect_closure(
            "artist-meta-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: CacheState, name: String| {
                    if let Some(artist) = this.imp().artist.borrow().as_ref() {
                        if name == artist.get_name() {
                            this.update_meta(artist);
                        }
                    }
                }
            )
        );

        let infobox_revealer = self.imp().infobox_revealer.get();
        let collapse_infobox = self.imp().collapse_infobox.get();
        collapse_infobox
            .bind_property(
                "active",
                &infobox_revealer,
                "reveal-child"
            )
            .transform_to(|_, active: bool| { Some(!active) })
            .transform_from(|_, active: bool| { Some(!active) })
            .bidirectional()
            .sync_create()
            .build();

        infobox_revealer
            .bind_property(
                "child-revealed",
                &collapse_infobox,
                "icon-name"
            )
            .transform_to(|_, revealed| {
                if revealed {
                    return Some("up-symbolic");
                }
                Some("down-symbolic")
            })
            .sync_create()
            .build();
    }

    fn setup_song_subview(&self, library: Library, cache: Rc<Cache>, client_state: ClientState) {
        client_state.connect_closure(
            "artist-songs-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, name: String, songs: glib::BoxedAnyObject| {
                    if let Some(artist) = this.imp().artist.borrow().as_ref() {
                        if name == artist.get_name() {
                            this.add_songs(songs.borrow::<Vec<Song>>().as_ref());
                        }
                    }
                }
            )
        );

        // TODO
        // let replace_queue_btn = self.imp().replace_queue.get();
        // replace_queue_btn.connect_clicked(
        //     clone!(
        //         #[strong(rename_to = this)]
        //         self,
        //         #[weak]
        //         library,
        //         move |_| {
        //             if let Some(artist) = this.imp().artist.borrow().as_ref() {
        //                 library.queue_songs(artist.clone(), true, true);
        //             }
        //         }
        //     )
        // );
        // let append_queue_btn = self.imp().append_queue.get();
        // append_queue_btn.connect_clicked(
        //     clone!(
        //         #[strong(rename_to = this)]
        //         self,
        //         #[weak]
        //         library,
        //         move |_| {
        //             if let Some(artist) = this.imp().artist.borrow().as_ref() {
        //                 library.queue_artist(artist.clone(), false, false);
        //             }
        //         }
        //     )
        // );

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `ArtistSongRow` during setup
        factory.connect_setup(clone!(
            #[weak]
            library,
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");
                let song_row = ArtistSongRow::new(
                    library,
                    &item
                );
                item.set_child(Some(&song_row));
            }
        ));
        // Tell factory how to bind `ArtistSongRow` to one of our Artist GObjects
        factory.connect_bind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
                // Get `Song` from `ListItem` (that is, the data side)
                let item: Song = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .item()
                    .and_downcast::<Song>()
                    .expect("The item has to be a common::Song.");

                // Get `ArtistSongRow` from `ListItem` (the UI widget)
                let child: ArtistSongRow = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<ArtistSongRow>()
                    .expect("The child has to be an `ArtistSongRow`.");

                // Within this binding fn is where the cached artist avatar texture gets used.
                child.bind(&item, cache);
            }
        ));


        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `ArtistSongRow` from `ListItem` (the UI widget)
            let child: ArtistSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<ArtistSongRow>()
                .expect("The child has to be an `ArtistSongRow`.");
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().song_subview.set_factory(Some(&factory));
        let sel_model = gtk::NoSelection::new(Some(self.imp().song_list.clone()));
        self.imp().song_subview.set_model(Some(&sel_model));
    }

    fn setup_album_subview(&self, library: Library, cache: Rc<Cache>, client_state: ClientState) {
        // TODO: handle click (switch to album tab & push album content page)
        // Unlike songs, we receive albums one by one.
        client_state.connect_closure(
            "artist-album-basic-info-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, name: String, album: Album| {
                    if let Some(artist) = this.imp().artist.borrow().as_ref() {
                        if name == artist.get_name() {
                            this.add_album(album);
                        }
                    }
                }
            )
        );

        // Set up factory
        let factory = SignalListItemFactory::new();
        factory.connect_setup(
            move |_, list_item| {
                let item = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem");
                // TODO: refactor album cells to use expressions too
                let album_cell = AlbumCell::new();
                item.set_child(Some(&album_cell));
            }
        );
        factory.connect_bind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
                let item: Album = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .item()
                    .and_downcast::<Album>()
                    .expect("The item has to be a common::Album.");
                let child: AlbumCell = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<AlbumCell>()
                    .expect("The child has to be an `AlbumCell`.");

                // Within this binding fn is where the cached artist avatar texture gets used.
                child.bind(&item, cache);
            }
        ));

        factory.connect_unbind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
            let child: AlbumCell = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumCell>()
                .expect("The child has to be an `AlbumCell`.");
            child.unbind(cache);
        }));

        // Set the factory of the list view
        self.imp().album_subview.set_factory(Some(&factory));
        let sel_model = gtk::NoSelection::new(Some(self.imp().album_list.clone()));
        self.imp().album_subview.set_model(Some(&sel_model));
    }

    pub fn setup(&self, library: Library, cache: Rc<Cache>, client_state: ClientState) {
        let _ = self.imp().cache.set(cache.clone());
        self.setup_info_box(cache.clone());
        self.setup_song_subview(library.clone(), cache.clone(), client_state.clone());
        self.setup_album_subview(library, cache, client_state);
    }

    /// Returns true if an avatar was successfully retrieved.
    /// On false, we will want to call cache.ensure_local_album_art()
    fn update_avatar(&self, name: &str) -> bool {
        if let Some(cache) = self.imp().cache.get() {
            if let Some(tex) = cache.load_local_artist_avatar(
                name, false
            ) {
                self.imp().avatar.set_custom_image(Some(&tex));
                return true;
            }
            else {
                self.imp().avatar.set_custom_image(Option::<&gdk::Texture>::None);
                return false;
            }
        }
        false
    }

    pub fn bind(&self, artist: Artist) {
        println!("Binding to artist: {:?}", &artist);
        self.update_meta(&artist);
        let name = artist.get_name();
        if !self.update_avatar(name) {
            if let Some(cache) = self.imp().cache.get() {
                cache.ensure_local_artist_meta(artist.get_mbid(), Some(name));
            }
        }

        let name_label = self.imp().name.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        let name_binding = artist
            .bind_property("name", &name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(name_binding);

        // Save reference to artist object
        self.imp().artist.borrow_mut().replace(artist);
    }

    pub fn unbind(&self) {
        println!("Artist content page hidden. Unbinding...");
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().avatar_signal_id.take() {
            if let Some(cache) = self.imp().cache.get() {
                cache.get_cache_state().disconnect(id);
            }
        }
        // Unset metadata widgets
        self.imp().bio_box.set_visible(false);
        self.clear_content();
    }

    fn add_album(&self, album: Album) {
        self.imp().album_list.append(&album);
        self.imp().album_count.set_label(&self.imp().album_list.n_items().to_string());
    }

    pub fn add_songs(&self, songs: &[Song]) {
        self.imp().song_list.extend_from_slice(songs);
        self.imp().song_count.set_label(&self.imp().song_list.n_items().to_string());
    }

    fn clear_content(&self) {
        self.imp().song_list.remove_all();
        self.imp().album_list.remove_all();
    }
}
