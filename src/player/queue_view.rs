use std::rc::Rc;


use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib,
    gdk,
    CompositeTemplate,
    SingleSelection,
    SignalListItemFactory,
    ListItem,
};
use gdk::Texture;
use glib::clone;

use crate::{
    cache::{
        Cache,
        CacheState,
        placeholders::ALBUMART_PLACEHOLDER
    },
    utils::strip_filename_linux,
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
    #[template(resource = "/org/euphonia/Euphonia/gtk/player/queue-view.ui")]
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
        #[template_child]
        pub clear_queue: TemplateChild<gtk::Button>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueueView {
        const NAME: &'static str = "EuphoniaQueueView";
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

    pub fn setup_listview(&self, player: Player, cache: Rc<Cache>) {
        // Enable/disable clear queue button depending on whether the queue is empty or not
        // Set selection mode
        // TODO: Allow click to jump to song
        let sel_model = SingleSelection::new(Some(player.queue()));
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

                // This song row is about to be displayed. Try to ensure that we
                // have a local copy of its album art. This might incur an API call.
                cache.ensure_local_album_art(strip_filename_linux(item.get_uri()));

                // Get `QueueRow` from `ListItem` (the UI widget)
                let child: QueueRow = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<QueueRow>()
                    .expect("The child has to be a `QueueRow`.");

                // Within this binding fn is where the cached album art texture gets used.
                child.bind(&item, cache.clone());
            })
        );

        // When row goes out of sight, unbind from item to allow reuse with another.
        // Remember to also unset the thumbnail widget's texture to potentially free it from memory.
        factory.connect_unbind(clone!(
            #[weak]
            cache,
            move |_, list_item| {
                // Get `QueueRow` from `ListItem` (the UI widget)
                let child: QueueRow = list_item
                    .downcast_ref::<ListItem>()
                    .expect("Needs to be ListItem")
                    .child()
                    .and_downcast::<QueueRow>()
                    .expect("The child has to be a `QueueRow`.");
                child.unbind(cache);
            })
        );

        // Set the factory of the list view
        self.imp().queue.set_factory(Some(&factory));

        // Setup click action
        self.imp().queue.connect_activate(move |queue, position| {
            // Get `IntegerObject` from model
            let model = queue.model().expect("The model has to exist.");
            let song = model
                .item(position)
                .and_downcast::<Song>()
                .expect("The item has to be a `common::Song`.");

            // Increase "number" of `IntegerObject`
            player.on_song_clicked(song);
        });
    }

    fn update_album_art(&self, tex: Option<&Texture>) {
        // Use high-resolution version here
        if tex.is_some() {
            self.imp().current_album_art.set_paintable(tex);
        }
        else {
            self.imp().current_album_art.set_resource(Some("/org/euphonia/Euphonia/albumart-placeholder.png"));
        }
    }

    pub fn bind_state(&self, player: Player) {
        let imp = self.imp();
        let info_box = imp.song_info_box.get();
        player
            .bind_property(
                "playback-state",
                &info_box,
                "visible"
            )
            .transform_to(|_, state: PlaybackState| {
                Some(state != PlaybackState::Stopped)
            })
            .sync_create()
            .build();

        let song_name = imp.current_song_name.get();
        player
            .bind_property(
                "title",
                &song_name,
                "label"
            )
            .sync_create()
            .build();

        let album = imp.current_album_name.get();
        player
            .bind_property(
                "album",
                &album,
                "label"
            )
            .sync_create()
            .build();

        let artist = imp.current_artist_name.get();
        player
            .bind_property(
                "artist",
                &artist,
                "label"
            )
            .sync_create()
            .build();

        self.update_album_art(player.current_song_album_art().as_ref());
        player.connect_notify_local(
            Some("album-art"),
            clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                player,
                move |_, _| {
                    this.update_album_art(player.current_song_album_art().as_ref());
                }
            )
        );

        let player_queue = player.queue();
        let clear_queue_btn = self.imp().clear_queue.get();
        player_queue
            .bind_property(
                "n-items",
                &clear_queue_btn,
                "sensitive"
            )
            .transform_to(|_, size: u32| {Some(size > 0)})
            .sync_create()
            .build();

        clear_queue_btn.connect_clicked(clone!(#[weak] player, move |_| {
            player.clear_queue();
        }));
    }

    pub fn setup(&self, player: Player, cache: Rc<Cache>) {
        self.setup_listview(player.clone(), cache);
        self.bind_state(player);
    }
}
