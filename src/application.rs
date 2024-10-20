/* application.rs
 *
 * Copyright 2024 htkhiem2000
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */
use gtk::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use std::{
    rc::Rc,
    fs::create_dir_all,
    path::PathBuf
};
use async_channel::Sender;

use crate::{
    library::Library,
    player::Player,
    client::{MpdWrapper, MpdMessage},
    cache::Cache,
    config::{VERSION, APPLICATION_USER_AGENT},
    preferences::Preferences,
    EuphoniaWindow
};

use adw::prelude::*;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct EuphoniaApplication {
        pub player: Player,
        pub library: Library,
        pub cache: Rc<Cache>,
        // pub library: Rc<LibraryController>, // TODO
    	pub sender: Sender<MpdMessage>, // To send to client wrapper
    	pub client: Rc<MpdWrapper>,
    	pub cache_path: PathBuf // Just clone this to construct more detailed paths
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EuphoniaApplication {
        const NAME: &'static str = "EuphoniaApplication";
        type Type = super::EuphoniaApplication;
        type ParentType = adw::Application;

        fn new() -> Self {
            // Create cache folder. This is where the cached album arts go.
            let mut cache_path: PathBuf = glib::user_cache_dir();
            cache_path.push("euphonia");
            println!("Cache path: {}", cache_path.to_str().unwrap());
            create_dir_all(&cache_path).expect("Could not create temporary directories!");

            // Create cache controller
            let cache = Cache::new(&cache_path);
            let meta_sender = cache.get_sender();

            // Create client instance (not connected yet)
            let client = MpdWrapper::new(meta_sender.clone());
            let sender = client.clone().get_sender();

            // Create controllers
            // These two are GObjects (already refcounted by GLib)
            let player = Player::default();
            let library = Library::default();
            cache.set_mpd_sender(sender.clone());

            Self {
                player,
                library,
                client,
                cache,
                sender,
                cache_path
            }
        }

    }

    impl ObjectImpl for EuphoniaApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
            obj.set_accels_for_action("app.fullscreen", &["F11"]);

            self.library.setup(self.sender.clone(), self.cache.clone());
            self.player.setup(
                self.obj().clone(),
                self.sender.clone(),
                self.client.clone().get_client_state(),
                self.cache.clone()
            );
        }
    }

    impl ApplicationImpl for EuphoniaApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = if let Some(window) = application.active_window() {
                window
            } else {
                let window = EuphoniaWindow::new(&*application);
                window.upcast()
            };

            // Ask the window manager/compositor to present the window
            window.present();

            // Start attempting to connect to the daemon once the window has been displayed.
            // This avoids delaying the presentation until the connection process concludes.
            let _ = application.imp().sender.send_blocking(MpdMessage::Connect);
        }
    }

    impl GtkApplicationImpl for EuphoniaApplication {}
    impl AdwApplicationImpl for EuphoniaApplication {}
}

glib::wrapper! {
    pub struct EuphoniaApplication(ObjectSubclass<imp::EuphoniaApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl EuphoniaApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        // TODO: Find a better place to put these
        musicbrainz_rs::config::set_user_agent(APPLICATION_USER_AGENT);
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
    }

    pub fn get_player(&self) -> Player {
        self.imp().player.clone()
    }

    pub fn get_library(&self) -> Library {
        self.imp().library.clone()
    }

    pub fn get_cache(&self) -> Rc<Cache> {
        self.imp().cache.clone()
    }

    pub fn get_client(&self) -> Rc<MpdWrapper> {
        self.imp().client.clone()
    }

    pub fn get_sender(&self) -> Sender<MpdMessage> {
        self.imp().sender.clone()
    }

    fn setup_gactions(&self) {
        let toggle_fullscreen_action = gio::ActionEntry::builder("fullscreen")
            .activate(move |app: &Self, _, _| app.toggle_fullscreen())
            .build();
        let update_db_action = gio::ActionEntry::builder("update-db")
            .activate(move |app: &Self, _, _| app.update_db())
            .build();
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| app.show_preferences())
            .build();
        self.add_action_entries([
            toggle_fullscreen_action,
            update_db_action,
            quit_action,
            about_action,
            preferences_action
        ]);
    }

    fn toggle_fullscreen(&self) {
        let window = self.active_window().unwrap();
        self.set_fullscreen(!window.is_fullscreen());
    }

    pub fn is_fullscreen(&self) -> bool {
        self.active_window().unwrap().is_fullscreen()
    }

    pub fn set_fullscreen(&self, state: bool) {
        let window = self.active_window().unwrap();
        if state {
            window.fullscreen();
        }
        else {
            window.unfullscreen();
        }
    }

    pub fn raise_window(&self) {
        let window = self.active_window().unwrap();
        window.present();
    }

    fn update_db(&self) {
        let sender = &self.imp().sender;
        let _ = sender.send_blocking(MpdMessage::Update);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Euphonia")
            .application_icon("org.euphonia.Euphonia")
            .developer_name("htkhiem2000")
            .version(VERSION)
            .developers(vec!["htkhiem2000"])
            .license_type(gtk::License::Gpl30)
            .copyright("© 2024 htkhiem2000")
            .build();

        about.add_credit_section(Some("Special Thanks"), &[
            "Nanling Zheng (reference background blur implementation) <neithern@outlook.com>",
            "Emmanuele Bassi (GTK, LibAdwaita, the Amberol project) https://www.bassi.io/"
        ]);
        about.present(Some(&window));
    }

    fn show_preferences(&self) {
        let window = self.active_window().unwrap();
        let prefs = Preferences::new(
            self.imp().sender.clone(),
            self.imp().client.clone().get_client_state(),
            self.imp().cache.clone()
        );
        prefs.present(Some(&window));
    }
}
