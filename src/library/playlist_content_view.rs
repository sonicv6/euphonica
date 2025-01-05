use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};
use adw::subclass::prelude::*;
use gtk::{
    gio, glib, prelude::*, BitsetIter, CompositeTemplate, ListItem, SignalListItemFactory
};
use glib::{
    clone,
    closure_local,
    Binding,
    signal::SignalHandlerId
};

use mpd::error::{Error as MpdError, ErrorCode as MpdErrorCode, ServerError};

use super::{
    Library,
    PlaylistSongRow
};
use crate::{
    cache::{
        placeholders::ALBUMART_PLACEHOLDER, Cache,
    }, client::ClientState, common::{INode, Song}, utils::format_secs_as_duration, window::EuphonicaWindow
};

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/org/euphonica/Euphonica/gtk/library/playlist-content-view.ui")]
    pub struct PlaylistContentView {
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
        pub last_mod: TemplateChild<gtk::Label>,
        #[template_child]
        pub track_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub runtime: TemplateChild<gtk::Label>,

        #[template_child]
        pub replace_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub replace_queue_text: TemplateChild<gtk::Label>,
        #[template_child]
        pub append_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub append_queue_text: TemplateChild<gtk::Label>,
        #[template_child]
        pub sel_all: TemplateChild<gtk::Button>,
        #[template_child]
        pub sel_none: TemplateChild<gtk::Button>,

        #[template_child]
        pub rename: TemplateChild<gtk::Button>,
        #[template_child]
        pub new_name: TemplateChild<gtk::Entry>,
        #[template_child]
        pub delete: TemplateChild<gtk::Button>,

        pub song_list: gio::ListStore,
        pub sel_model: gtk::MultiSelection,

        pub playlist: RefCell<Option<INode>>,
        pub bindings: RefCell<Vec<Binding>>,
        pub cover_signal_id: RefCell<Option<SignalHandlerId>>,
        pub cache: OnceCell<Rc<Cache>>,
        pub selecting_all: Cell<bool>,  // Enables queuing the entire album efficiently
        pub window: OnceCell<EuphonicaWindow>
    }

    impl Default for PlaylistContentView {
        fn default() -> Self {
            Self {
                cover: TemplateChild::default(),
                title: TemplateChild::default(),
                last_mod: TemplateChild::default(),
                track_count: TemplateChild::default(),
                infobox_revealer: TemplateChild::default(),
                collapse_infobox: TemplateChild::default(),
                runtime: TemplateChild::default(),
                content: TemplateChild::default(),
                song_list: gio::ListStore::new::<Song>(),
                sel_model: gtk::MultiSelection::new(Option::<gio::ListStore>::None),
                replace_queue: TemplateChild::default(),
                append_queue: TemplateChild::default(),
                replace_queue_text: TemplateChild::default(),
                append_queue_text: TemplateChild::default(),
                sel_all: TemplateChild::default(),
                sel_none: TemplateChild::default(),
                rename: TemplateChild::default(),
                new_name: TemplateChild::default(),
                delete: TemplateChild::default(),
                bindings: RefCell::new(Vec::new()),
                cover_signal_id: RefCell::new(None),
                cache: OnceCell::new(),
                playlist: RefCell::new(None),
                selecting_all: Cell::new(true),  // When nothing is selected, default to select-all
                window: OnceCell::new()
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaylistContentView {
        const NAME: &'static str = "EuphonicaPlaylistContentView";
        type Type = super::PlaylistContentView;
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

    impl ObjectImpl for PlaylistContentView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.sel_model.set_model(Some(&self.song_list.clone()));
            self.content.set_model(Some(&self.sel_model));

            // Change button labels depending on selection state
            self.sel_model.connect_selection_changed(clone!(
                #[weak(rename_to = this)]
                self,
                move |sel_model, _, _| {
                    // TODO: this can be slow, might consider redesigning
                    let n_sel = sel_model.selection().size();
                    if n_sel == 0 || (n_sel as u32) == sel_model.model().unwrap().n_items() {
                        this.selecting_all.replace(true);
                        this.replace_queue_text.set_label("Play all");
                        this.append_queue_text.set_label("Queue all");
                    }
                    else {
                        // TODO: l10n
                        this.selecting_all.replace(false);
                        this.replace_queue_text.set_label(format!("Play {}", n_sel).as_str());
                        this.append_queue_text.set_label(format!("Queue {}", n_sel).as_str());
                    }
                }
            ));

            let sel_model = self.sel_model.clone();
            self.sel_all.connect_clicked(clone!(
                #[weak]
                sel_model,
                move |_| {
                    sel_model.select_all();
                }
            ));
            self.sel_none.connect_clicked(clone!(
                #[weak]
                sel_model,
                move |_| {
                    sel_model.unselect_all();
                }
            ));
        }
    }

    impl WidgetImpl for PlaylistContentView {}
}

glib::wrapper! {
    pub struct PlaylistContentView(ObjectSubclass<imp::PlaylistContentView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for PlaylistContentView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PlaylistContentView {
    pub fn setup(&self, library: Library, client_state: ClientState, cache: Rc<Cache>, window: EuphonicaWindow) {
        // cache.get_cache_state().connect_closure(
        //     "album-art-downloaded",
        //     false,
        //     closure_local!(
        //         #[weak(rename_to = this)]
        //         self,
        //         move |_: CacheState, folder_uri: String| {
        //             if let Some(album) = this.imp().album.borrow().as_ref() {
        //                 if folder_uri == album.get_uri() {
        //                     this.update_cover(album.get_info());
        //                 }
        //             }
        //         }
        //     )
        // );
        self.imp().window.set(window).expect("PlaylistContentView: Cannot set reference to window");
        client_state.connect_closure(
            "playlist-songs-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                move |_: ClientState, name: String, songs: glib::BoxedAnyObject| {
                    if let Some(playlist) = this.imp().playlist.borrow().as_ref() {
                        if playlist.get_name() == Some(&name) {
                            this.add_songs(songs.borrow::<Vec<Song>>().as_ref());
                        }
                    }
                }
            )
        );

        let _ = self.imp().cache.set(cache.clone());
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
                    if let Some(playlist) = this.imp().playlist.borrow().as_ref() {
                        if this.imp().selecting_all.get() {
                            library.queue_playlist(playlist.get_name().unwrap(), true, true);
                        }
                        else {
                            let store = &this.imp().song_list;
                            // Get list of selected songs
                            let sel = &this.imp().sel_model.selection();
                            let mut songs: Vec<Song> = Vec::with_capacity(sel.size() as usize);
                            let (iter, first_idx) = BitsetIter::init_first(sel).unwrap();
                            songs.push(store.item(first_idx).and_downcast::<Song>().unwrap());
                            iter
                                .for_each(
                                    |idx| songs.push(store.item(idx).and_downcast::<Song>().unwrap())
                                );
                            library.queue_songs(&songs, true, true);
                        }
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
                    if let Some(playlist) = this.imp().playlist.borrow().as_ref() {
                        if this.imp().selecting_all.get() {
                            library.queue_playlist(playlist.get_name().unwrap(), false, false);
                        }
                        else {
                            let store = &this.imp().song_list;
                            // Get list of selected songs
                            let sel = &this.imp().sel_model.selection();
                            let mut songs: Vec<Song> = Vec::with_capacity(sel.size() as usize);
                            let (iter, first_idx) = BitsetIter::init_first(sel).unwrap();
                            songs.push(store.item(first_idx).and_downcast::<Song>().unwrap());
                            iter
                                .for_each(
                                    |idx| songs.push(store.item(idx).and_downcast::<Song>().unwrap())
                                );
                            library.queue_songs(&songs, false, false);
                        }
                    }
                }
            )
        );

        let rename_btn = self.imp().rename.get();
        let new_name = self.imp().new_name.get();

        new_name
            .connect_closure(
                "changed",
                false,
                closure_local!(
                    #[weak(rename_to = this)]
                    self,
                    #[weak]
                    rename_btn,
                    move |entry: gtk::Entry| {
                        if let Some(playlist) = this.imp().playlist.borrow().as_ref() {
                            rename_btn.set_sensitive(
                                entry.text_length() > 0 &&
                                    entry.buffer().text() != playlist.get_name().unwrap()
                            );
                        }
                        else {
                            rename_btn.set_sensitive(false);
                        }
                    }
                )
            );

        rename_btn.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            library,
            #[weak]
            new_name,
            move |_| {
                let rename_from: Option<String>;
                {
                    if let Some(playlist) = this.imp().playlist.borrow().as_ref() {
                        rename_from = playlist.get_name().map(&str::to_owned);
                    }
                    else {
                        rename_from = None;
                    }
                }
                if let Some(rename_from) = rename_from {
                    let rename_to = new_name.buffer().text().as_str().to_owned();

                    // Temporarily rebind to a modified INode object. The actual updated INode,
                    // with correct last-modified time, will be given after the idle trigger.
                    let tmp_inode = this.imp().playlist.borrow().as_ref().unwrap().with_new_name(&rename_to);
                    this.unbind(false);
                    this.bind(tmp_inode);

                    match library.rename_playlist(&rename_from, &rename_to) {
                        Ok(()) => {} // Wait for idle to trigger a playlist refresh
                        Err(e) => match e {
                            Some(MpdError::Server(ServerError {code, pos: _, command: _, detail: _})) => {
                                this.imp().window.get().unwrap().show_dialog(
                                    "Rename Failed",
                                    &(match code {
                                        MpdErrorCode::Exist => format!("There is already another playlist named \"{}\". Please pick another name.", &rename_to),
                                        MpdErrorCode::NoExist => "Internal error: tried to rename a nonexistent playlist".to_owned(),
                                        _ => format!("Internal error ({:?})", code)
                                    })
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        ));

        append_queue_btn.connect_clicked(
            clone!(
                #[strong(rename_to = this)]
                self,
                #[weak]
                library,
                move |_| {
                    if let Some(playlist) = this.imp().playlist.borrow().as_ref() {

                        if this.imp().selecting_all.get() {
                            library.queue_playlist(playlist.get_name().unwrap(), false, false);
                        }
                        else {
                            let store = &this.imp().song_list;
                            // Get list of selected songs
                            let sel = &this.imp().sel_model.selection();
                            let mut songs: Vec<Song> = Vec::with_capacity(sel.size() as usize);
                            let (iter, first_idx) = BitsetIter::init_first(sel).unwrap();
                            songs.push(store.item(first_idx).and_downcast::<Song>().unwrap());
                            iter
                                .for_each(
                                    |idx| songs.push(store.item(idx).and_downcast::<Song>().unwrap())
                                );
                            library.queue_songs(&songs, false, false);
                        }
                    }
                }
            )
        );

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `PlaylistSongRow` during setup
        factory.connect_setup(move |_, list_item| {
            let item = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem");
            let row = PlaylistSongRow::new(library.clone(), &item);
            item.set_child(Some(&row));
        });
        // Tell factory how to bind `PlaylistSongRow` to one of our Playlist GObjects
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

                // Get `PlaylistSongRow` from `ListItem` (the UI widget)
                let child: PlaylistSongRow = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<PlaylistSongRow>()
                    .expect("The child has to be an `PlaylistSongRow`.");

                // Within this binding fn is where the cached album art texture gets used.
                child.bind(&item, cache.clone());
            }
        ));


        // When row goes out of sight, unbind from item to allow reuse with another.
        factory.connect_unbind(move |_, list_item| {
            // Get `PlaylistSongRow` from `ListItem` (the UI widget)
            let child: PlaylistSongRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<PlaylistSongRow>()
                .expect("The child has to be an `PlaylistSongRow`.");
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().content.set_factory(Some(&factory));
    }

    /// Returns true if an album art was successfully retrieved.
    /// On false, we will want to call cache.ensure_local_album_art()
    // fn update_cover(&self, info: &PlaylistInfo) -> bool {
    //     if let Some(cache) = self.imp().cache.get() {
    //         if let Some(tex) = cache.load_cached_album_art(info, false, true) {
    //             self.imp().cover.set_paintable(Some(&tex));
    //             return true;
    //         }
    //         else {
    //             self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
    //             return false;
    //         }
    //     }
    //     false
    // }

    pub fn bind(&self, playlist: INode) {
        let title_label = self.imp().title.get();
        let last_mod_label = self.imp().last_mod.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        let title_binding = playlist
            .bind_property("uri", &title_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(title_binding);

        let last_mod_binding = playlist
            .bind_property("last-modified", &last_mod_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(last_mod_binding);

        // self.update_cover();
        self.imp().playlist.borrow_mut().replace(playlist);
    }

    pub fn unbind(&self, clear_contents: bool) {
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().cover_signal_id.take() {
            if let Some(cache) = self.imp().cache.get() {
                cache.get_cache_state().disconnect(id);
            }
        }
        if clear_contents {
            self.imp().song_list.remove_all();
        }

    }

    fn add_songs(&self, songs: &[Song]) {
        self.imp().song_list.extend_from_slice(songs);
        self.imp().track_count.set_label(&self.imp().song_list.n_items().to_string());
        self.imp().runtime.set_label(
            &format_secs_as_duration(
                self.imp().song_list
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
    }

    pub fn current_playlist(&self) -> Option<INode> {
        self.imp().playlist.borrow().clone()
    }
}
