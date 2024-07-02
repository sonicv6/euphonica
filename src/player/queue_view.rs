use std::{
    rc::Rc
};

use async_channel::Sender;

use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib,
    CompositeTemplate,
    NoSelection,
    SignalListItemFactory,
    ListItem
};

use crate::{
    player::Player,
    client::MpdMessage,
    common::Song
};

use super::QueueRow;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/queue-view.ui")]
    pub struct QueueView {
        #[template_child]
        pub queue: TemplateChild<gtk::ListView>
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

    pub fn setup(&self, player: Rc<Player>, sender: Sender<MpdMessage>) {
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
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            if !item.has_cover() {
                // Request album art. Will be updated later when ready.
                let _ = sender.send_blocking(MpdMessage::AlbumArt(item.get_uri()));
            }

            // Get `QueueRow` from `ListItem` (the UI widget)
            let child: QueueRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<QueueRow>()
                .expect("The child has to be a `QueueRow`.");

            child.bind(&item);
        });

        // When row goes out of sight, unbind from item to allow reuse with another
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
}
