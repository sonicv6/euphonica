/* window.rs
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

use std::cell::{RefCell};
use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib
};
use glib::{
    signal::SignalHandlerId
};
use crate::{
    utils,
    client::{ConnectionState},
    application::EuphoniaApplication,
    player::{QueueView, PlayerBar},
    library::AlbumView,
    sidebar::Sidebar
};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/window.ui")]
    pub struct EuphoniaWindow {
        // Template widgets
        // #[template_child]
        // pub view_switcher: TemplateChild<adw::ViewSwitcher>,
        // #[template_child]
        // pub header_bar: TemplateChild<adw::HeaderBar>,


        // Main views
        #[template_child]
        pub album_view: TemplateChild<AlbumView>,
        #[template_child]
        pub queue_view: TemplateChild<QueueView>,

        // Content view stack
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,

        // Sidebar
        #[template_child]
        pub title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub sidebar: TemplateChild<Sidebar>,

        // Bottom bar
        #[template_child]
        pub player_bar: TemplateChild<PlayerBar>,

        // RefCells to notify IDs so we can unbind later
        pub notify_position_id: RefCell<Option<SignalHandlerId>>,
        pub notify_playback_state_id: RefCell<Option<SignalHandlerId>>,
        pub notify_duration_id: RefCell<Option<SignalHandlerId>>,


    }

    #[glib::object_subclass]
    impl ObjectSubclass for EuphoniaWindow {
        const NAME: &'static str = "EuphoniaWindow";
        type Type = super::EuphoniaWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self {
                album_view: TemplateChild::default(),
                queue_view: TemplateChild::default(),
                stack: TemplateChild::default(),
                title: TemplateChild::default(),
                sidebar: TemplateChild::default(),
                player_bar: TemplateChild::default(),
                notify_position_id: RefCell::new(None),
                notify_duration_id: RefCell::new(None),
                notify_playback_state_id: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for EuphoniaWindow {}
    impl WidgetImpl for EuphoniaWindow {}
    impl WindowImpl for EuphoniaWindow {}
    impl ApplicationWindowImpl for EuphoniaWindow {}
    impl AdwApplicationWindowImpl for EuphoniaWindow {}
}

glib::wrapper! {
    pub struct EuphoniaWindow(ObjectSubclass<imp::EuphoniaWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl EuphoniaWindow {
    pub fn new<P: glib::object::IsA<gtk::Application>>(application: &P) -> Self {
        let win: Self =  glib::Object::builder()
            .property("application", application)
            .build();

        let app = win.downcast_application();

        win.restore_window_state();
        win.imp().queue_view.setup(
            app.get_player(),
            app.get_album_art_cache()
        );
        win.imp().album_view.setup(
            app.get_library(),
            app.get_album_art_cache()
        );
        win.imp().sidebar.setup(
            win.imp().stack.clone()
        );
        win.imp().player_bar.setup(
            app.get_player(),
            app.get_sender()
        );
		win.bind_state();
        win.setup_signals();
        win
    }

    fn restore_window_state(&self) {
        let settings = utils::settings_manager();
        let state = settings.child("state");
        let width = state.int("last-window-width");
        let height = state.int("last-window-height");
        self.set_default_size(width, height);
    }

    fn downcast_application(&self) -> EuphoniaApplication {
        self.application()
            .unwrap()
            .downcast::<crate::application::EuphoniaApplication>()
            .unwrap()
    }

    fn bind_state(&self) {
        // Bind client state to app name widget
        let client = self.downcast_application().get_client();
        let state = client.get_client_state();
        let title = self.imp().title.get();
        state
            .bind_property(
                "connection-state",
                &title,
                "subtitle"
            )
            .transform_to(|_, state: ConnectionState| {
                match state {
                    ConnectionState::NotConnected => Some("Not connected"),
                    ConnectionState::Connecting => Some("Connecting"),
                    ConnectionState::Unauthenticated => Some("Unauthenticated"),
                    ConnectionState::Connected => Some("Connected")
                }
            })
            .sync_create()
            .build();
	}

	fn setup_signals(&self) {
	    self.connect_close_request(move |window| {
            let size = window.default_size();
	        let width = size.0;
            let height = size.1;
            let settings = utils::settings_manager();
            let state = settings.child("state");
            state
                .set_int("last-window-width", width)
                .expect("Unable to store last-window-width");
            state
                .set_int("last-window-height", height)
                .expect("Unable to stop last-window-height");

	        // TODO: persist other settings at closing?
            glib::Propagation::Proceed
        });
	}
}
