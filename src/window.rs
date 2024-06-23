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
use glib::signal::SignalHandlerId;
use glib::{clone, closure_local};
use crate::client::wrapper::{MpdWrapper, MpdMessage};

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
        #[template_child]
        pub seekbar: TemplateChild<gtk::Scale>,

        // RefCells to notify IDs so we can unbind later
        pub notify_position_id: RefCell<Option<SignalHandlerId>>,
        pub notify_playing_id: RefCell<Option<SignalHandlerId>>,
        pub notify_duration_id: RefCell<Option<SignalHandlerId>>,

        // Handle to seekbar polling task
        pub seekbar_poller_handle: RefCell<Option<glib::JoinHandle<()>>>
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
                seekbar: TemplateChild::default(),
                notify_position_id: RefCell::new(None),
                notify_duration_id: RefCell::new(None),
                notify_playing_id: RefCell::new(None),
                seekbar_poller_handle: RefCell::new(None)
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

        win.setup_seekbar();
		win.bind_state();
        win.setup_signals();
        win
    }

    fn setup_seekbar(&self) {
        // Capture mouse button release action for seekbar
        let seekbar_gesture = gtk::GestureClick::new();
        // Assign your handler to an event of the gesture (e.g. the `pressed` event)
        seekbar_gesture.connect_pressed(clone!(@weak self as this => move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            println!("Dragging seekbar");
            if let Some(handle) = this.imp().seekbar_poller_handle.take() {
                handle.abort();
            }
        }));
        // Funny story: there is no gtk::Scale::get_value.
        // We need to connect to the change-value signal to do that.
        let release_sender = self.application()
            .unwrap()
            .downcast::<crate::application::SlamprustApplication>()
            .unwrap()
            .get_sender();
        let prepare_sender = release_sender.clone();
        self.imp().seekbar.connect_change_value(move |_, scroll_type, new_position| {
            let _ = prepare_sender.send_blocking(MpdMessage::PrepareSeekCur(new_position));
            glib::signal::Propagation::Proceed
        });
        seekbar_gesture.connect_unpaired_release(clone!(@weak self as this => move |gesture, _, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            println!("Released seekbar");
            let _ = release_sender.send_blocking(MpdMessage::SeekCur);
        }));
        self.imp().seekbar.add_controller(seekbar_gesture);
    }

    fn client(&self) -> Option<Rc<MpdWrapper>> {
        println!("Checking if client wrapper exists");
        if let Some(app) = self.application() {
            println!("Has client wrapper!");
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

	fn update_seekbar(&self, update_duration: bool) {
	    let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
	    let player_state = client.get_player_state();
	    let pos = player_state.position();
        let pos_upper = pos.ceil() as u64;
        let duration = player_state.duration();
	    if update_duration {
	        // Set value to 0 first since new duration might be 0 (empty playlist).
	        self.imp().seekbar.set_value(0.0);
	        self.imp().seekbar.set_range(0.0, duration as f64);
       }
        // Now we can restore position. Avoid setting pos higher than max.
        // We can safely approximate this by comparing the rounded-up version of
        // position to max.

        if pos_upper > duration {
            self.imp().seekbar.set_value(duration as f64);
        }
        else {
            self.imp().seekbar.set_value(pos);
        }
	    // TODO: sensitivity status (disable when stopped or playlist is empty)
	}

    fn maybe_start_polling(&self) {
        // Periodically poll for player progress to update seekbar
        if let Some(app) = self.application() {
            let downcast_app = app
                .downcast::<crate::application::SlamprustApplication>()
                .unwrap();
            let sender = downcast_app.get_sender();
            let client = downcast_app.get_client().clone();
            let poller_handle = glib::MainContext::default().spawn_local(async move {
                loop {
                    let state = client.get_player_state();
                    // Don't poll if not playing
                    if !state.is_playing() {
                        break;
                    }
                    // Skip poll if channel is full
                    if !sender.is_full() {
                        let _ = sender.send_blocking(MpdMessage::Status(false));
                    }
                    glib::timeout_future_seconds(1).await;
                }
            });
            self.imp().seekbar_poller_handle.replace(Some(poller_handle));
        }
    }

	fn bind_state(&self) {
		let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
		let player_state = client.get_player_state();

        // We'll first need to sync with the state initially; afterwards the binding will do it for us.
        self.update_label();
        let notify_playing_id = player_state.connect_notify_local(
            Some("playing"),
            clone!(@weak self as win => move |_, _| {
                win.update_label();
                win.maybe_start_polling();
            }),
        );

        self.update_seekbar(true);
        let notify_position_id = player_state.connect_notify_local(
            Some("position"),
            clone!(@weak self as win => move |_, _| {
                win.update_seekbar(false);
            }),
        );
        let notify_duration_id = player_state.connect_notify_local(
            Some("duration"),
            clone!(@weak self as win => move |_, _| {
                win.update_seekbar(true);
            }),
        );
        self.imp().notify_playing_id.replace(Some(notify_playing_id));
        self.imp().notify_position_id.replace(Some(notify_position_id));
        self.imp().notify_duration_id.replace(Some(notify_duration_id));
	}

	fn unbind_state(&self) {
	    let client = self.client().unwrap();  // Panic otherwise since we can't proceed without state
		let player_state = client.get_player_state();

		// Just take directly since we're unbinding anyway
		// TODO: turn this into a loop perhaps?
        if let Some(id) = self.imp().notify_playing_id.take() {
            player_state.disconnect(id);
        }
        if let Some(id) = self.imp().notify_position_id.take() {
            player_state.disconnect(id);
        }
        if let Some(id) = self.imp().notify_duration_id.take() {
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
