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
    #[template(resource = "/org/euphonia/Euphonia/gtk/preferences/client.ui")]
    pub struct ClientPreferences {
        #[template_child]
        pub mpd_host: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub mpd_port: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub mpd_status: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub reconnect: TemplateChild<gtk::Button>,
        #[template_child]
        pub mpd_download_album_art: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub use_lastfm: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub lastfm_key: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub lastfm_user_agent: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub lastfm_download_album_art: TemplateChild<adw::SwitchRow>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClientPreferences {
        const NAME: &'static str = "EuphoniaClientPreferences";
        type Type = super::ClientPreferences;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ClientPreferences {}
    impl WidgetImpl for ClientPreferences {}
    impl PreferencesPageImpl for ClientPreferences {}
}

glib::wrapper! {
    pub struct ClientPreferences(ObjectSubclass<imp::ClientPreferences>)
        @extends adw::PreferencesPage,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Widget;
}

impl Default for ClientPreferences {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ClientPreferences {
    fn on_connection_state_changed(&self, cs: &ClientState) {
        match cs.get_connection_state() {
            ConnectionState::NotConnected => {
                self.imp().mpd_status.set_subtitle("Failed to connect");
                if !self.imp().mpd_port.has_css_class("error") {
                    self.imp().reconnect.set_sensitive(true);
                }
            },
            ConnectionState::Connecting => {
                self.imp().mpd_status.set_subtitle("Connecting...");
                self.imp().reconnect.set_sensitive(false);
            },
            ConnectionState::Unauthenticated => {
                self.imp().mpd_status.set_subtitle("Authentication failed");
                if !self.imp().mpd_port.has_css_class("error") {
                    self.imp().reconnect.set_sensitive(true);
                }
            },
            ConnectionState::Connected => {
                self.imp().mpd_status.set_subtitle("Connected");
                if !self.imp().mpd_port.has_css_class("error") {
                    self.imp().reconnect.set_sensitive(true);
                }
            }
        }
    }

    pub fn setup(&self, sender: Sender<MpdMessage>, client_state: ClientState) {
        let imp = self.imp();
        // Populate with current gsettings values
        let settings = utils::settings_manager();
        // These should only be saved when the Apply button is clicked.
        // As such we won't bind the widgets directly to the settings.
        let conn_settings = settings.child("client");
        imp.mpd_host.set_text(&conn_settings.string("mpd-host"));
        imp.mpd_port.set_text(&conn_settings.uint("mpd-port").to_string());

        // TODO: more input validation
        // Prevent entering anything other than digits into the port entry row
        // This is needed since using a spinbutton row for port entry feels a bit weird
        imp.mpd_port.connect_changed(clone!(
            #[strong(rename_to = this)]
            self,
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

        // Display connection status
        self.on_connection_state_changed(&client_state);
        client_state.connect_notify_local(
            Some("connection-state"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |cs, _| {
                    this.on_connection_state_changed(cs);
                }
            )
        );

        imp.reconnect.connect_clicked(clone!(
            #[strong(rename_to = this)]
            self,
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
        let mpd_download_album_art = imp.mpd_download_album_art.get();
        conn_settings
            .bind(
                "mpd-download-album-art",
                &mpd_download_album_art,
                "active"
            )
            .build();

        // Set up Last.fm settings
        let use_lastfm = imp.use_lastfm.get();
        conn_settings
            .bind(
                "use-lastfm",
                &use_lastfm,
                "active"
            )
            .build();
        let lastfm_key = imp.lastfm_key.get();
        let lastfm_user_agent = imp.lastfm_user_agent.get();
        let lastfm_download_album_art = imp.lastfm_download_album_art.get();
        // let lastfm_username = imp.lastfm_username.get();
        for widget in [
            &lastfm_key.clone().upcast::<gtk::Widget>(),
            &lastfm_user_agent.clone().upcast::<gtk::Widget>(),
            &lastfm_download_album_art.clone().upcast::<gtk::Widget>()
        ] {
            use_lastfm
                .bind_property(
                    "active",
                    widget,
                    "sensitive"
                )
                .sync_create()
                .build();
        }

        conn_settings
            .bind(
                "lastfm-api-key",
                &lastfm_key,
                "text"
            )
            .build();

        conn_settings
            .bind(
                "lastfm-user-agent",
                &lastfm_user_agent,
                "text"
            )
            .build();

        conn_settings
            .bind(
                "lastfm-download-album-art",
                &lastfm_download_album_art,
                "active"
            )
            .build();
    }
}
