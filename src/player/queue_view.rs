use std::{
    rc::Rc,
    cell::RefCell
};

use async_channel::Sender;

use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib,
    gdk,
    CompositeTemplate,
    NoSelection,
    SignalListItemFactory,
    ListItem,
};
use gdk::Texture;
use glib::{
    clone,
    signal::SignalHandlerId
};

use crate::{
    client::MpdMessage,
    client::albumart::{AlbumArtCache, strip_filename_linux},
    common::Song
};

use super::{
    QueueRow,
    Player,
    PlaybackState
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/queue-view.ui")]
    pub struct QueueView {
        #[template_child]
        pub queue: TemplateChild<gtk::ListView>,
        #[template_child]
        pub current_album_art: TemplateChild<gtk::Image>,
        #[template_child]
        pub song_info_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub current_song_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub current_artist_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub current_album_name: TemplateChild<gtk::Label>,

        pub signal_ids: RefCell<Vec<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueueView {
        const NAME: &'static str = "SlamprustQueueView";
        type Type = super::QueueView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_layout_manager_type::<gtk::BinLayout>();
            // klass.set_css_name("QueueView");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for QueueView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for QueueView {}
}

glib::wrapper! {
    pub struct QueueView(ObjectSubclass<imp::QueueView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for QueueView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl QueueView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup_listview(&self, player: Rc<Player>, albumart: Rc<AlbumArtCache>) {
        // Set selection mode
        // TODO: Allow click to jump to song
        let sel_model = NoSelection::new(Some(player.queue()));
        self.imp().queue.set_model(Some(&sel_model));

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `QueueRow` during setup
        factory.connect_setup(move |_, list_item| {
            let queue_row = QueueRow::new();
            list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .set_child(Some(&queue_row));
        });
        // Tell factory how to bind `QueueRow` to one of our Song GObjects
        factory.connect_bind(clone!(@weak albumart as cache => move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            // This song is about to be displayed. Cache its album art (if any) now.
            // Might result in a cache miss, in which case the file will be immediately loaded
            // from disk.
            // Note that this does not trigger any downloading. That's done by the Player
            // controller upon receiving queue updates.
            if item.get_thumbnail().is_none() {
                if let Some(tex) = albumart.get_for(strip_filename_linux(&item.get_uri()), true) {
                    item.set_thumbnail(Some(tex));
                }
            }

            // Get `QueueRow` from `ListItem` (the UI widget)
            let child: QueueRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<QueueRow>()
                .expect("The child has to be a `QueueRow`.");

            // Within this binding fn is where the cached album art texture gets used.
            child.bind(&item);
        }));


        // When row goes out of sight, unbind from item to allow reuse with another.
        // Remember to also unset the thumbnail widget's texture to potentially free it from memory.
        factory.connect_unbind(move |_, list_item| {
            // Get `QueueRow` from `ListItem` (the UI widget)
            let child: QueueRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<QueueRow>()
                .expect("The child has to be a `QueueRow`.");
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");
            child.unbind(&item);
        });

        // Set the factory of the list view
        self.imp().queue.set_factory(Some(&factory));
    }

    fn update_info_visibility(&self, is_playing: bool) {
        self.imp().song_info_box.set_visible(is_playing);
    }

    fn update_song_name(&self, song_name: Option<&String>) {
        if let Some(name) = song_name {
            self.imp().current_song_name.set_label(name);
        }
    }

    fn update_artist_name(&self, artist_name: Option<&String>) {
        if let Some(name) = artist_name {
            self.imp().current_artist_name.set_label(name);
        }
    }

    fn update_album_name(&self, album_name: Option<&String>) {
        if let Some(name) = album_name {
            self.imp().current_album_name.set_label(name);
        }
    }

    fn update_album_art(&self, tex: Option<&Texture>) {
        // Use high-resolution version here
        if tex.is_some() {
            self.imp().current_album_art.set_from_paintable(tex);
        }
        else {
            self.imp().current_album_art.set_from_resource(Some("/org/slamprust/Slamprust/albumart-placeholder.png"));
        }
    }

    pub fn bind_state(&self, player: Rc<Player>) {
        let mut ids = self.imp().signal_ids.borrow_mut();
        // We'll first need to sync with the state initially; afterwards the binding will do it for us.
        self.update_info_visibility(player.playback_state() != PlaybackState::Stopped);
        ids.push(
            player.connect_notify_local(
                Some("playback-state"),
                clone!(@weak self as this, @weak player as p => move |_, _| {
                    this.update_info_visibility(p.playback_state() != PlaybackState::Stopped);
                })
            )  
        );

        self.update_song_name(player.title().as_ref());
        ids.push(
            player.connect_notify_local(
                Some("title"),
                clone!(@weak self as this, @weak player as p => move |_, _| {
                    this.update_song_name(p.title().as_ref());
                })
            )
        );

        self.update_album_name(player.album().as_ref());
        ids.push(
            player.connect_notify_local(
                Some("album"),
                clone!(@weak self as this, @weak player as p => move |_, _| {
                    this.update_album_name(p.album().as_ref());
                })
            )
        );

        self.update_artist_name(player.artist().as_ref());
        ids.push(
            player.connect_notify_local(
                Some("artist"),
                clone!(@weak self as this, @weak player as p => move |_, _| {
                    this.update_artist_name(p.artist().as_ref());
                })
            )
        );

        self.update_album_art(player.album_art().as_ref());
        ids.push(
            player.connect_notify_local(
                Some("album-art"),
                clone!(@weak self as this, @weak player as p => move |_, _| {
                    this.update_album_art(p.album_art().as_ref());
                })
            )
        );
    }

    pub fn setup(&self, player: Rc<Player>, albumart: Rc<AlbumArtCache>) {
        self.setup_listview(player.clone(), albumart);
        self.bind_state(player);
    }
}
