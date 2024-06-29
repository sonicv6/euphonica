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
    cell::{Cell, RefCell},
    rc::Rc
};
use adw::subclass::prelude::*;
use gtk::{
    prelude::*,
    gio,
    glib,
    NoSelection,
    SignalListItemFactory,
    ListItem
};
use glib::signal::SignalHandlerId;
use glib::clone;
use crate::{
    client::MpdMessage,
    application::SlamprustApplication,
    player::{QueueRow, PlaybackState},
    common::Song
};
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
        #[template_child]
        pub queue: TemplateChild<gtk::ListView>,

        // RefCells to notify IDs so we can unbind later
        pub notify_position_id: RefCell<Option<SignalHandlerId>>,
        pub notify_playback_state_id: RefCell<Option<SignalHandlerId>>,
        pub notify_duration_id: RefCell<Option<SignalHandlerId>>,

        // Handle to seekbar polling task
        pub seekbar_poller_handle: RefCell<Option<glib::JoinHandle<()>>>,
        // Temporary place for seekbar position before sending seekcur
        // TODO: move both of these into a custom seekbar widget.
        pub new_position: Cell<f64>,
        pub seekbar_clicked: Cell<bool>
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
                queue: TemplateChild::default(),
                notify_position_id: RefCell::new(None),
                notify_duration_id: RefCell::new(None),
                notify_playback_state_id: RefCell::new(None),
                seekbar_poller_handle: RefCell::new(None),
                new_position: Cell::new(0.0),
                seekbar_clicked: Cell::new(false)
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

        win.setup_queue();
        win.setup_seekbar();
		win.bind_state();
        win.setup_signals();
        win
    }

    fn downcast_application(&self) -> SlamprustApplication {
        self.application()
            .unwrap()
            .downcast::<crate::application::SlamprustApplication>()
            .unwrap()
    }

    fn get_queue(&self) -> gio::ListStore {
        self.downcast_application().get_player().queue()
    }

    fn setup_queue(&self) {
        // Set selection mode
        let sel_model = NoSelection::new(Some(self.get_queue()));
        self.imp().queue.set_model(Some(&sel_model));

        // Set up factory
        let factory = SignalListItemFactory::new();

        // Create an empty `QueueRow` during setup
        factory.connect_setup(move |_, list_item| {
            let queue_row = QueueRow::new();
            list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .set_child(Some(&queue_row));
        });

        // Tell factory how to bind `QueueRow` to one of our Song GObjects
        factory.connect_bind(move |_, list_item| {
            // Get `Song` from `ListItem` (that is, the data side)
            let item: Song = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<Song>()
                .expect("The item has to be a common::Song.");

            // Get `QueueRow` from `ListItem` (the UI widget)
            let child: QueueRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<QueueRow>()
                .expect("The child has to be a `QueueRow`.");

            child.bind(&item);
        });

        // When row goes out of sight, unbind from item to allow reuse with another
        factory.connect_unbind(move |_, list_item| {
            // Get `QueueRow` from `ListItem` (the UI widget)
            let child: QueueRow = list_item
                .downcast_ref::<ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<QueueRow>()
                .expect("The child has to be a `QueueRow`.");
            // No need to get item again - child already has bindings to it.
            child.unbind();
        });

        // Set the factory of the list view
        self.imp().queue.set_factory(Some(&factory));
    }

    fn setup_seekbar(&self) {
        // Capture mouse button release action for seekbar
        // Funny story: gtk::Scale has its own GestureClick controller which will eat up mouse button events.
        // Workaround: capture mouse button release event at window level in capture phase, using a bool
        // set by the seekbar's change-value signal to determine whether it is related to the seekbar or not.
        let sender = self.downcast_application().get_sender();

        let seekbar_gesture = gtk::GestureClick::new();
        seekbar_gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        seekbar_gesture.connect_released(clone!(@weak self as this => move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::None); // allow propagating to seekbar
            if this.imp().seekbar_clicked.get() {
                println!("Released seekbar");
                let _ = sender.send_blocking(MpdMessage::SeekCur(this.imp().new_position.get()));
                this.imp().seekbar_clicked.replace(false);
                this.maybe_start_polling();
            }
        }));

        self.add_controller(seekbar_gesture);

        // Funny story: there is no gtk::Scale::get_value.
        // We need to connect to the change-value signal to do that.
        // Here is where we (might) stop the polling loop and set the
        // seekbar_clicked flag to true.
        self.imp().seekbar.connect_change_value(clone!(@strong self as this => move |_, _, new_position| {
            this.imp().new_position.replace(new_position);
            if let Some(handle) = this.imp().seekbar_poller_handle.take() {
                println!("Dragging seekbar");
                handle.abort();
            }
            this.imp().seekbar_clicked.set(true);
            glib::signal::Propagation::Proceed
        }));
    }

	fn update_label(&self) {
	    let player = self.downcast_application().get_player();
	    match player.playback_state() {
	        PlaybackState::Playing => {self.imp().label.set_label("Playing")},
	        PlaybackState::Paused => {self.imp().label.set_label("Paused")},
	        PlaybackState::Stopped => {self.imp().label.set_label("Stopped")}
	    }
	}

	fn update_seekbar(&self, update_duration: bool) {
	    let player = self.downcast_application().get_player();
	    let pos = player.position();
        let pos_upper = pos.ceil() as u64;
        let duration = player.duration();
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
        // Won't start a new loop if there is already one
        let sender = self.downcast_application().get_sender();
        let player = self.downcast_application().get_player();
        let poller_handle = glib::MainContext::default().spawn_local(async move {
            loop {
                // Don't poll if not playing
                if player.playback_state() != PlaybackState::Playing {
                    break;
                }
                // Skip poll if channel is full
                if !sender.is_full() {
                    let _ = sender.send_blocking(MpdMessage::Status);
                }
                glib::timeout_future_seconds(1).await;
            }
        });
        self.imp().seekbar_poller_handle.replace(Some(poller_handle));
    }

	fn bind_state(&self) {
		let player = self.downcast_application().get_player();

        // We'll first need to sync with the state initially; afterwards the binding will do it for us.
        self.update_label();
        let notify_playback_state_id = player.connect_notify_local(
            Some("playback-state"),
            clone!(@weak self as win => move |_, _| {
                win.update_label();
                win.maybe_start_polling();
            }),
        );

        self.update_seekbar(true);
        let notify_position_id = player.connect_notify_local(
            Some("position"),
            clone!(@weak self as win => move |_, _| {
                win.update_seekbar(false);
            }),
        );
        let notify_duration_id = player.connect_notify_local(
            Some("duration"),
            clone!(@weak self as win => move |_, _| {
                win.update_seekbar(true);
            }),
        );
        self.imp().notify_playback_state_id.replace(Some(notify_playback_state_id));
        self.imp().notify_position_id.replace(Some(notify_position_id));
        self.imp().notify_duration_id.replace(Some(notify_duration_id));
	}

	fn unbind_state(&self) {
	    let player = self.downcast_application().get_player();

		// Just take directly since we're unbinding anyway
		// TODO: turn this into a loop perhaps?
        if let Some(id) = self.imp().notify_playback_state_id.take() {
            player.disconnect(id);
        }
        if let Some(id) = self.imp().notify_position_id.take() {
            player.disconnect(id);
        }
        if let Some(id) = self.imp().notify_duration_id.take() {
            player.disconnect(id);
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
