use std::rc::Rc;
use async_channel::Sender;

use adw::subclass::prelude::*;
use adw::prelude::*;
use gtk::{
    glib,
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
    #[template(resource = "/org/euphonia/Euphonia/gtk/preferences.ui")]
    pub struct Preferences {
        #[template_child]
        pub mpd_host: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub mpd_port: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub reconnect: TemplateChild<gtk::Button>,

        // pub signal_ids: RefCell<Vec<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Preferences {
        const NAME: &'static str = "EuphoniaPreferences";
        type Type = super::Preferences;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_layout_manager_type::<gtk::BinLayout>();
            // klass.set_css_name("Preferences");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
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
    pub fn new(sender: Sender<MpdMessage>, client_state: Rc<ClientState>) -> Self {
        let res = Self::default();
        let imp = res.imp();
        // Populate with current gsettings values
        let settings = utils::settings_manager();
        // These should only be saved when the Apply button is clicked.
        // As such we won't bind the widgets directly to the settings.
        let conn_settings = settings.child("client");
        imp.mpd_host.set_text(&conn_settings.string("mpd-host"));
        imp.mpd_port.set_text(&conn_settings.int("mpd-port").to_string());

        // TODO: more input validation
        // Prevent entering anything other than digits into the port entry row
        // This is needed since using a spinbutton row for port entry feels a bit weird
        imp.mpd_port.connect_changed(clone!(
            #[strong(rename_to = this)]
            res,
            move |entry| {
                if entry.text().parse::<u32>().is_err() {
                    if !entry.has_css_class("error") {
                        entry.add_css_class("error");
                        this.imp().reconnect.set_sensitive(false);
                    }
                }
                else if entry.has_css_class("error") {
                    entry.remove_css_class("error");
                    this.imp().reconnect.set_sensitive(true);
                }
            }
        ));

        // Use toasts to announce connection statuses within the preferences dialog
        client_state.connect_notify_local(
            Some("connection-state"),
            clone!(
                #[strong(rename_to = this)]
                res,
                move |cs, _| {
                    match cs.get_connection_state() {
                        ConnectionState::NotConnected => {
                            this.add_toast(
                                adw::Toast::new("Failed to connect")
                            );
                            if !this.imp().mpd_port.has_css_class("error") {
                                this.imp().reconnect.set_sensitive(true);
                            }
                        },
                        ConnectionState::Connecting => {
                            // No toast for this one, as it will prevent the
                            // "result" toasts from being displayed.
                            // Instead we'll simply dim the Apply button.
                            this.imp().reconnect.set_sensitive(false);
                        },
                        ConnectionState::Unauthenticated => {
                            this.add_toast(
                                adw::Toast::new("Authentication failed")
                            );
                            if !this.imp().mpd_port.has_css_class("error") {
                                this.imp().reconnect.set_sensitive(true);
                            }
                        },
                        ConnectionState::Connected => {
                            this.add_toast(
                                adw::Toast::new("Connected!")
                            );
                            if !this.imp().mpd_port.has_css_class("error") {
                                this.imp().reconnect.set_sensitive(true);
                            }
                        }
                    }
                }
            )
        );

        imp.reconnect.connect_clicked(clone!(
            #[strong(rename_to = this)]
            res,
            #[strong]
            conn_settings,
            #[strong]
            sender,
            move |_| {
                let _ = conn_settings.set_string("mpd-host", &this.imp().mpd_host.text());
                let _ = conn_settings.set_uint("mpd-port", this.imp().mpd_port.text().parse::<u32>().unwrap());
                let _ = sender.send_blocking(MpdMessage::Connect);
            }
        ));

        res
    }
}
