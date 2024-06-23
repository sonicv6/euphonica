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
    fmt::{self, Display, Formatter},
    rc::Rc,
};
use async_channel::{Sender, Receiver};

use crate::client::wrapper::{MpdWrapper, MpdMessage};
use crate::config::VERSION;
use crate::SlamprustWindow;
// use crate::player::controller::PlayerController;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct SlamprustApplication {
        // pub player: Rc<PlayerController>,
        // pub library: Rc<LibraryController>, // TODO
    	pub sender: Sender<MpdMessage>, // To send to client wrapper
    	pub client: Rc<MpdWrapper>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SlamprustApplication {
        const NAME: &'static str = "SlamprustApplication";
        type Type = super::SlamprustApplication;
        type ParentType = adw::Application;

        fn new() -> Self {
            // Set up channels for communication with client object
            // Only one message at a time to client
            let (
                sender,
                receiver
            ): (Sender<MpdMessage>, Receiver<MpdMessage>) = async_channel::bounded(1);

            // Create client instance (not connected yet)
            let client = MpdWrapper::new(
                sender.clone(),
                RefCell::new(Some(receiver))
            );

            // TODO: use gsettings for reading host & port
            let _ = sender.send_blocking(MpdMessage::Connect(
                String::from("localhost"),
                String::from("6600")
            ));

            // Create controllers (Rc pointers)
            // let player = PlayerController::new(
            //     sender_to_client.clone(),
            //     receiver_from_client.clone()
            // );

            Self {
                // player,
                client,
                sender,
                // receiver: RefCell::new(Some(receiver_from_client)),
            }
        }
    }

    impl ObjectImpl for SlamprustApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
        }
    }

    impl ApplicationImpl for SlamprustApplication {
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
                let window = SlamprustWindow::new(&*application);
                window.upcast()
            };

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for SlamprustApplication {}
    impl AdwApplicationImpl for SlamprustApplication {}
}

glib::wrapper! {
    pub struct SlamprustApplication(ObjectSubclass<imp::SlamprustApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl SlamprustApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
    }

    pub fn connect(&self) {
        // TODO: GUI & config file
        // TODO: move this to a submod for the settings dialogue instead maybe?
        self.imp().sender.send_blocking(
            MpdMessage::Connect(String::from("localhost"), String::from("6600"))
        );
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
        self.add_action_entries([quit_action, about_action]);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutWindow::builder()
            .transient_for(&window)
            .application_name("Slamprust")
            .application_icon("org.slamprust.Slamprust")
            .developer_name("Work")
            .version(VERSION)
            .developers(vec!["Work"])
            .copyright("© 2024 Work")
            .build();

        about.present();
    }
}
