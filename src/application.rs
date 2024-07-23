/* application.rs
 *
 * Copyright 2024 Work
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
    cell::RefCell,
    rc::Rc,
    fs::create_dir_all,
    path::PathBuf
};
use async_channel::{Sender, Receiver};

use crate::{
    library::Library,
    player::Player,
    client::{MpdWrapper, MpdMessage, AlbumArtCache},
    config::VERSION,
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
        pub albumart: Rc<AlbumArtCache>,
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

            // Set up channels for communication with client object
            // Only one message at a time to client
            let (
                sender,
                receiver
            ): (Sender<MpdMessage>, Receiver<MpdMessage>) = async_channel::unbounded();

            // Create controllers
            // These two are GObjects (already refcounted by GLib)
            let player = Player::default();
            let library = Library::default();
            let albumart = Rc::new(AlbumArtCache::new(&cache_path));
            player.setup(sender.clone(), albumart.clone());
            library.setup(sender.clone(), albumart.clone());

            // Create client instance (not connected yet)
            let client = MpdWrapper::new(
                player.clone(),
                library.clone(),
                sender.clone(),
                RefCell::new(Some(receiver)),
                albumart.clone()
            );

            Self {
                player,
                library,
                client,
                albumart,
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

    pub fn get_album_art_cache(&self) -> Rc<AlbumArtCache> {
        self.imp().albumart.clone()
    }

    pub fn get_client(&self) -> Rc<MpdWrapper> {
        self.imp().client.clone()
    }

    pub fn get_sender(&self) -> Sender<MpdMessage> {
        self.imp().sender.clone()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| app.show_preferences())
            .build();
        self.add_action_entries([quit_action, about_action, preferences_action]);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Euphonia")
            .application_icon("org.euphonia.Euphonia")
            .developer_name("Work")
            .version(VERSION)
            .developers(vec!["Work"])
            .copyright("© 2024 Work")
            .build();

        about.present(Some(&window));
    }

    fn show_preferences(&self) {
        let window = self.active_window().unwrap();
        let prefs = Preferences::new(
            self.imp().sender.clone(),
            self.imp().client.clone().get_client_state()
        );
        prefs.present(Some(&window));
    }
}
