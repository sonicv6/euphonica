use std::rc::Rc;

use async_channel::Sender;


use adw::subclass::prelude::*;
use gtk::{
    glib,
    CompositeTemplate
};

use crate::{cache::Cache, client::{ClientState, MpdMessage}};

use super::{
    ClientPreferences,
    IntegrationsPreferences,
    LibraryPreferences,
    PlayerPreferences
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/preferences/dialog.ui")]
    pub struct Preferences {
        #[template_child]
        pub client_tab: TemplateChild<ClientPreferences>,

        #[template_child]
        pub integrations_tab: TemplateChild<IntegrationsPreferences>,

        #[template_child]
        pub library_tab: TemplateChild<LibraryPreferences>,

        #[template_child]
        pub player_tab: TemplateChild<PlayerPreferences>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Preferences {
        const NAME: &'static str = "EuphoniaPreferences";
        type Type = super::Preferences;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Preferences {}
    impl WidgetImpl for Preferences {}
    impl WindowImpl for Preferences {}
    impl DialogImpl for Preferences {}
    impl AdwDialogImpl for Preferences {}
    impl PreferencesDialogImpl for Preferences {}
}

glib::wrapper! {
    pub struct Preferences(ObjectSubclass<imp::Preferences>)
        @extends adw::PreferencesDialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, adw::Dialog, gtk::Widget;
}

impl Default for Preferences {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Preferences {
    pub fn new(sender: Sender<MpdMessage>, client_state: ClientState, cache: Rc<Cache>) -> Self {
        let res = Self::default();

        res.imp().client_tab.get().setup(sender, client_state);
        res.imp().library_tab.get().setup();
        res.imp().player_tab.get().setup();
        res.imp().integrations_tab.get().setup(cache);

        res
    }
}
