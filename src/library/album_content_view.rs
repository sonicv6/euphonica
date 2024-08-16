use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use time::{Date, format_description};
use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    gdk,
    glib,
    CompositeTemplate,
    SignalListItemFactory,
    ListItem,
};
use gdk::Texture;
use glib::{
    clone,
    Binding,
    signal::SignalHandlerId
};

use super::{
    Library,
    AlbumSongRow
};
use crate::{
    utils::format_secs_as_duration,
    common::{Album, Song},
    cache::Cache,
    lastfm::models
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/album-content-view.ui")]
    pub struct AlbumContentView {
        #[template_child]
        pub infobox_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub collapse_infobox: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub cover: TemplateChild<gtk::Image>,
        #[template_child]
        pub content: TemplateChild<gtk::ListView>,
        #[template_child]
        pub title: TemplateChild<gtk::Label>,
        #[template_child]
        pub artist: TemplateChild<gtk::Label>,
        #[template_child]
        pub release_date: TemplateChild<gtk::Label>,
        #[template_child]
        pub track_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub runtime: TemplateChild<gtk::Label>,
        #[template_child]
        pub replace_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub append_queue: TemplateChild<gtk::Button>,

        pub album: RefCell<Option<Album>>,
        pub bindings: RefCell<Vec<Binding>>,
        pub cover_signal_id: RefCell<Option<SignalHandlerId>>,
        pub cache: OnceCell<Rc<Cache>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumContentView {
        const NAME: &'static str = "EuphoniaAlbumContentView";
        type Type = super::AlbumContentView;
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

    impl ObjectImpl for AlbumContentView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for AlbumContentView {}
}

glib::wrapper! {
    pub struct AlbumContentView(ObjectSubclass<imp::AlbumContentView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for AlbumContentView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl AlbumContentView {
    pub fn setup(&self, library: Library, cache: Rc<Cache>) {
        let _ = self.imp().cache.set(cache);
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

        let replace_queue_btn = self.imp().replace_queue.get();
        replace_queue_btn.connect_clicked(
            clone!(
                #[strong(rename_to = this)]
                self,
                #[weak]
                library,
                move |_| {
                    if let Some(album) = this.imp().album.borrow().as_ref() {
                        library.queue_album(album.clone(), true, true);
                    }
                }
            )
        );
        let append_queue_btn = self.imp().append_queue.get();
        append_queue_btn.connect_clicked(
            clone!(
                #[strong(rename_to = this)]
                self,
                #[weak]
                library,
                move |_| {
                    if let Some(album) = this.imp().album.borrow().as_ref() {
                        library.queue_album(album.clone(), false, false);
                    }
                }
            )
        );

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `AlbumSongRow` during setup
        factory.connect_setup(move |_, list_item| {
            let song_row = AlbumSongRow::new();
            song_row.setup(library.clone());
            list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .set_child(Some(&song_row));
        });
        // Tell factory how to bind `AlbumSongRow` to one of our Album GObjects
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: AlbumSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumSongRow>()
                .expect("The child has to be an `AlbumSongRow`.");

            // Within this binding fn is where the cached album art texture gets used.
            child.bind(&item);
        });


        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `AlbumSongRow` from `ListItem` (the UI widget)
            let child: AlbumSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<AlbumSongRow>()
                .expect("The child has to be an `AlbumSongRow`.");
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().content.set_factory(Some(&factory));
    }

    pub fn set_album(&self, album: Album, song_list: gio::ListStore) {
        self.setup_content(song_list);
        self.bind(album);
    }

    fn update_cover(&self, tex: Option<&Texture>) {
        // Use high-resolution version here
        if tex.is_some() {
            self.imp().cover.set_paintable(tex);
        }
    }

    pub fn bind(&self, album: Album) {
        println!("Binding to album: {:?}", &album);
        let title_label = self.imp().title.get();
        let artist_label = self.imp().artist.get();
        let release_date_label = self.imp().release_date.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        // TODO: actually use this
        let cache = self.imp().cache.get().unwrap().clone();
        if let Some(info) = cache.load_local_album_info(
            album.get_mb_album_id().as_deref(),
            Some(album.get_title()).as_deref(),
            album.get_artist().as_deref()
        ) {
            println!("{:?}", info.wiki);
        }

        let title_binding = album
            .bind_property("title", &title_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(title_binding);

        let artist_binding = album
            .bind_property("artist", &artist_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(artist_binding);

        let release_date_binding = album
            .bind_property("release_date", &release_date_label, "label")
            .transform_to(
                |_, boxed_date: glib::BoxedAnyObject| {
                    let format = format_description::parse("[year]-[month]-[day]").ok().unwrap();
                    if let Some(release_date) = boxed_date.borrow::<Option<Date>>().as_ref() {
                        return release_date.format(
                            &format
                        ).ok();
                    }
                    Some("-".to_owned())
                }
            )
            .sync_create()
            .build();
        // Save binding
        bindings.push(release_date_binding);

        let release_date_viz_binding = album
            .bind_property("release_date", &release_date_label, "visible")
            .transform_to(
                |_, boxed_date: glib::BoxedAnyObject| {
                    if boxed_date.borrow::<Option<Date>>().is_some() {
                        return Some(true);
                    }
                    Some(false)
                }
            )
            .sync_create()
            .build();
        // Save binding
        bindings.push(release_date_viz_binding);

        self.update_cover(album.get_cover().as_ref());
        self.imp().cover_signal_id.replace(Some(
            album.connect_notify_local(
                Some("cover"),
                clone!(
                    #[weak(rename_to = this)]
                    self,
                    #[weak]
                    album,
                    move |_, _| {
                        this.update_cover(album.get_cover().as_ref());
                    }
                )
            )
        ));

        // Save reference to album object
        self.imp().album.borrow_mut().replace(album);
    }

    pub fn unbind(&self) {
        println!("Album content page hidden. Unbinding...");
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().cover_signal_id.take() {
            if let Some(album) = self.imp().album.borrow_mut().take() {
                album.disconnect(id);
            }
        }
    }

    pub fn setup_content(&self, song_list: gio::ListStore) {
        self.imp().track_count.set_label(&song_list.n_items().to_string());
        self.imp().runtime.set_label(
            &format_secs_as_duration(
                song_list
                    .iter()
                    .map(
                        |item: Result<Song, _>| {
                            if let Ok(song) = item {
                                return song.get_duration();
                            }
                            0
                        }
                    )
                    .sum::<u64>() as f64
            )
        );
        let sel_model = gtk::NoSelection::new(Some(song_list));
        self.imp().content.set_model(Some(&sel_model));
    }

    pub fn clear_content(&self) {
        self.imp().content.set_model(Option::<&gtk::NoSelection>::None);
    }
}
