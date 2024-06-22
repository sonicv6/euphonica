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

use std::{
    rc::Rc,
    cell::RefCell
};
use gtk::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use glib::{clone, closure_local};
use crate::client::wrapper::MpdWrapper;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/window.ui")]
    pub struct SlamprustWindow {
        // Template widgets
        #[template_child]
        pub header_bar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,

        // RefCells to notify IDs so we can unbind later
        pub notify_playing_id: RefCell<Option<glib::signal::SignalHandlerId>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SlamprustWindow {
        const NAME: &'static str = "SlamprustWindow";
        type Type = super::SlamprustWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self {
                header_bar: TemplateChild::default(),
                label: TemplateChild::default(),

                notify_playing_id: RefCell::new(None)
            }
        }
    }

    impl ObjectImpl for SlamprustWindow {}
    impl WidgetImpl for SlamprustWindow {}
    impl WindowImpl for SlamprustWindow {}
    impl ApplicationWindowImpl for SlamprustWindow {}
    impl AdwApplicationWindowImpl for SlamprustWindow {}
}

glib::wrapper! {
    pub struct SlamprustWindow(ObjectSubclass<imp::SlamprustWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl SlamprustWindow {
    pub fn new<P: glib::IsA<gtk::Application>>(application: &P) -> Self {
        let win: Self =  glib::Object::builder()
            .property("application", application)
            .build();

		win.bind_state();
        win.setup_signals();
        win
    }

    fn client(&self) -> Option<Rc<MpdWrapper>> {
        println!("Checking if client exists");
        if let Some(app) = self.application() {
            println!("Has client!");
            return Some(app
                .downcast::<crate::application::SlamprustApplication>()
                .unwrap()
                .get_client()
            );
        }
        None
    }

	fn update_label(&self) {
	    let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
	    let player_state = client.get_player_state();
	    if player_state.is_playing() {
	        self.imp().label.set_label("Playing");
	    }
	    else {
	        self.imp().label.set_label("Paused");
	    }
	}

	fn bind_state(&self) {
	    println!("bind_state: getting client...");
		let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
		let player_state = client.get_player_state();
        // Test: use the PlayerState::playing property
        // We'll first need to sync with the state initially; afterwards the binding will do it for us.
        self.update_label();
        let notify_playing_id = player_state.connect_notify_local(
            Some("playing"),
            clone!(@weak self as win => move |_, _| {
                win.update_label();
            }),
        );
        self.imp().notify_playing_id.replace(Some(notify_playing_id));
	}

	fn unbind_state(&self) {
	    let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
		let player_state = client.get_player_state();

		// Just take directly since we're unbinding anyway
        if let Some(id) = self.imp().notify_playing_id.take() {
            player_state.disconnect(id);
        }
	}

	fn setup_signals(&self) {
	    self.connect_close_request(move |window| {
	        // TODO: save window size?
	        // TODO: persist other settings at closing?
            window.unbind_state();
            glib::Propagation::Proceed
        });
	}
}
