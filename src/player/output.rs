use std::{
    cell::{RefCell},
    f64::consts::PI
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate
};
use glib::{
    Object,
    Binding,
    signal::SignalHandlerId
};
use mpd::output::Output;

use super::Player;

fn map_icon_name(plugin_name: &str) -> &'static str {
    match plugin_name {
        "alsa" => "alsa-symbolic",
        "pulse" => "pulseaudio-symbolic",
        "pipewire" => "pipewire-symbolic",
        _ => "soundcard-symbolic"
    }
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/player/output.ui")]
    pub struct MpdOutput {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub name: TemplateChild<gtk::Label>,
         #[template_child]
        pub enable: TemplateChild<gtk::Switch>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for MpdOutput {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaMpdOutput";
        type Type = super::MpdOutput;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for MpdOutput {}

    // Trait shared by all widgets
    impl WidgetImpl for MpdOutput {}

    // Trait shared by all boxes
    impl BoxImpl for MpdOutput {}
}

glib::wrapper! {
    pub struct MpdOutput(ObjectSubclass<imp::MpdOutput>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl MpdOutput {
    pub fn from_output(output: &Output, player: Player) -> Self {
        let res: Self = Object::builder().build();
        // Get state
        let imp = res.imp();
        let name = imp.name.get();
        let icon = imp.icon.get();
        let enable = imp.enable.get();

        name.set_label(&output.name);
        icon.set_icon_name(Some(map_icon_name(&output.plugin)));
        enable.set_active(output.enabled);

        let id = output.id;
        enable.connect_activate(move |sw| {
            player.set_output(id, sw.is_active())
        });

        res
    }
}
