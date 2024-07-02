use std::{
    cell::{Cell, RefCell},
    rc::Rc
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    Label,
    Image
};
use glib::{
    Object,
    Binding,
    clone,
    signal::SignalHandlerId
};

use crate::common::Song;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/player-bar.ui")]
    pub struct PlayerBar {
        #[template_child]
        pub thumbnail: TemplateChild<Image>,
        #[template_child]
        pub song_name: TemplateChild<Label>,
        #[template_child]
        pub playing_indicator: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Song GObject
        pub bindings: RefCell<Vec<Binding>>,
        pub thumbnail_signal_id: RefCell<Option<SignalHandlerId>>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for PlayerBar {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "SlamprustPlayerBar";
        type Type = super::PlayerBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for PlayerBar {}

    // Trait shared by all widgets
    impl WidgetImpl for PlayerBar {}

    // Trait shared by all boxes
    impl BoxImpl for PlayerBar {}
}