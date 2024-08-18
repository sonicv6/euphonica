use adw::subclass::prelude::*;
use adw::prelude::*;
use gtk::{
    glib::{self, Value, Variant},
    CompositeTemplate
};

use glib::clone;

use crate::{
    client::{MpdMessage, ClientState, ConnectionState},
    utils
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/preferences/library.ui")]
    pub struct LibraryPreferences {
        #[template_child]
        pub sort_nulls_first: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub sort_case_sensitive: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub search_case_sensitive: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LibraryPreferences {
        const NAME: &'static str = "EuphoniaLibraryPreferences";
        type Type = super::LibraryPreferences;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LibraryPreferences {}
    impl WidgetImpl for LibraryPreferences {}
    impl PreferencesPageImpl for LibraryPreferences {}
}

glib::wrapper! {
    pub struct LibraryPreferences(ObjectSubclass<imp::LibraryPreferences>)
        @extends adw::PreferencesPage,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Widget;
}

impl Default for LibraryPreferences {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl LibraryPreferences {
    pub fn setup(&self) {
        let imp = self.imp();
        // Populate with current gsettings values
        let settings = utils::settings_manager();
        // Set up library settings
        let library_settings = settings.child("library");
        let sort_nulls_first = imp.sort_nulls_first.get();
        library_settings
            .bind(
                "sort-nulls-first",
                &sort_nulls_first,
                "active"
            )
            .build();
        let sort_case_sensitive = imp.sort_case_sensitive.get();
        library_settings
            .bind(
                "sort-case-sensitive",
                &sort_case_sensitive,
                "active"
            )
            .build();
        let search_case_sensitive = imp.search_case_sensitive.get();
        library_settings
            .bind(
                "search-case-sensitive",
                &search_case_sensitive,
                "active"
            )
            .build();
    }
}
