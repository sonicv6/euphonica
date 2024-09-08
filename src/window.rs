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

use std::cell::RefCell;
use adw::{
    prelude::*,
    subclass::prelude::*
};
use gtk::{
    gdk, gio, glib::{self, clone},
};
use glib::signal::SignalHandlerId;
use crate::{
    application::EuphoniaApplication,
    client::ConnectionState,
    library::{AlbumView, ArtistView},
    player::{PlayerBar, QueueView},
    sidebar::Sidebar,
    utils
};

mod imp {
    use std::cell::{Cell, OnceCell};

    use glib::Properties;
    use gtk::graphene;
    use utils::settings_manager;

    use crate::{common::paintables::FadePaintable, player::Player};

    use super::*;

    #[derive(Debug, Default, Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::EuphoniaWindow)]
    #[template(resource = "/org/euphonia/Euphonia/window.ui")]
    pub struct EuphoniaWindow {
        // Top level widget (for toggling root class)
        #[template_child]
        pub content: TemplateChild<gtk::Box>,
        // Main views
        #[template_child]
        pub album_view: TemplateChild<AlbumView>,
        #[template_child]
        pub artist_view: TemplateChild<ArtistView>,
        #[template_child]
        pub queue_view: TemplateChild<QueueView>,

        // Content view stack
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,

        // Sidebar
        // TODO: Replace with Libadwaita spinner when v1.6 hits stable
        #[template_child]
        pub busy_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub sidebar: TemplateChild<Sidebar>,

        // Bottom bar
        #[template_child]
        pub player_bar_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub player_bar: TemplateChild<PlayerBar>,

        // RefCells to notify IDs so we can unbind later
        pub notify_position_id: RefCell<Option<SignalHandlerId>>,
        pub notify_playback_state_id: RefCell<Option<SignalHandlerId>>,
        pub notify_duration_id: RefCell<Option<SignalHandlerId>>,

        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set)]
        pub use_album_art_bg: Cell<bool>,
        #[property(get, set)]
        pub blur_radius: Cell<u32>,
        #[property(get, set)]
        pub opacity: Cell<f64>,
        #[property(get, set)]
        pub transition_duration: Cell<f64>,
        pub bg_paintable: FadePaintable,
        pub player: OnceCell<Player>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EuphoniaWindow {
        const NAME: &'static str = "EuphoniaWindow";
        type Type = super::EuphoniaWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            // klass.set_layout_manager_type::<gtk::BoxLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for EuphoniaWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let settings = settings_manager().child("player");
            let obj_borrow = self.obj();
            let obj = obj_borrow.as_ref();
            settings
                .bind(
                    "use-album-art-as-bg",
                    obj,
                    "use-album-art-bg"
                )
                .build();

            // If using album art as background we must disable the default coloured
            // backgrounds that navigation views use for their sidebars.
            // We do this by toggling the "no-shading" CSS class for the top-level
            // content widget, which in turn toggles the CSS selectors selecting those
            // views.
            obj.connect_notify_local(
                Some("use-album-art-bg"),
                |this, _| {
                    this.update_background();
                }
            );

            settings
                .bind(
                    "bg-blur-radius",
                    obj,
                    "blur-radius"
                )
                .build();

            settings
                .bind(
                    "bg-opacity",
                    obj,
                    "opacity"
                )
                .build();

            settings
                .bind(
                    "bg-transition-duration-s",
                    obj,
                    "transition-duration"
                )
                .build();

            self.sidebar.connect_notify_local(
                Some("showing-queue-view"),
                clone!(
                    #[weak(rename_to = this)]
                    obj,
                    move |_, _| {
                        this.update_player_bar_visibility();
                    }
                )
            );

            self.queue_view.connect_notify_local(
                Some("show-content"),
                clone!(
                    #[weak(rename_to = this)]
                    obj,
                    move |_, _| {
                        this.update_player_bar_visibility();
                    }
                )
            );

            self.queue_view.connect_notify_local(
                Some("collapsed"),
                clone!(
                    #[weak(rename_to = this)]
                    obj,
                    move |_, _| {
                        this.update_player_bar_visibility();
                    }
                )
            );
        }
    }
    impl WidgetImpl for EuphoniaWindow {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;

            // Bluuuuuur
            // Adopted from Nanling Zheng's implementation for Gapless.
            if self.use_album_art_bg.get() {
                let bg_width = self.bg_paintable.intrinsic_width() as f32;
                let bg_height = self.bg_paintable.intrinsic_height() as f32;
                // Also zoom in enough to avoid the "transparent edges" caused by blurring edge pixels
                let blur_radius = self.blur_radius.get();
                let scale_x = width / (bg_width - blur_radius as f32) as f32;
                let scale_y = height / (bg_height - blur_radius as f32) as f32;
                let scale_max = scale_x.max(scale_y);
                let view_width = bg_width * scale_max;
                let view_height = bg_height * scale_max;
                let delta_x = (width - view_width) * 0.5;
                let delta_y = (height - view_height) * 0.5;

                snapshot.push_clip(&graphene::Rect::new(
                    0.0, 0.0, width, height
                ));
                snapshot.translate(&graphene::Point::new(
                    delta_x, delta_y
                ));
                // Blur & opacity nodes

                if blur_radius > 0 {
                    snapshot.push_blur(blur_radius as f64);
                }
                let opacity = self.opacity.get();
                if opacity < 1.0 {
                    snapshot.push_opacity(opacity);
                }
                self.bg_paintable.snapshot(snapshot, view_width as f64, view_height as f64);
                snapshot.translate(&graphene::Point::new(
                    -delta_x, -delta_y
                ));
                snapshot.pop();
                if opacity < 1.0 {
                    snapshot.pop();
                }
                if blur_radius > 0 {
                    snapshot.pop();
                }
            }

            // Call the parent class's snapshot() method to render child widgets
            self.parent_snapshot(snapshot);
        }
    }
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
        let player = app.get_player();
        win.update_background();
        player.connect_notify_local(
            Some("album-art"),
            clone!(
                #[weak(rename_to = this)]
                win,
                move |_, _| {
                    this.update_background();
                }
            )
        );
        let _ = win.imp().player.set(player);

        win.restore_window_state();
        win.imp().queue_view.setup(
            app.get_player(),
            app.get_cache()
        );
        win.imp().album_view.setup(
            app.get_library(),
            app.get_cache(),
            app.get_client().get_client_state()
        );
        win.imp().artist_view.setup(
            app.get_library(),
            app.get_cache(),
            app.get_client().get_client_state()
        );
        win.imp().sidebar.setup(
            win.imp().stack.get(),
            app.get_player()
        );
        win.imp().player_bar.setup(
            app.get_player()
        );
		win.bind_state();
        win.setup_signals();
        win
    }

    fn update_player_bar_visibility(&self) {
        let revealer = self.imp().player_bar_revealer.get();
        if self.imp().sidebar.showing_queue_view() {
            let queue_view = self.imp().queue_view.get();
            if (queue_view.collapsed() && queue_view.show_content()) || !queue_view.collapsed() {
                revealer.set_reveal_child(false);
            }
            else {
                revealer.set_reveal_child(true);
            }
        }
        else {
            revealer.set_reveal_child(true);
        }
    }

    /// Update blurred background, if enabled
    fn update_background(&self) {
        if let Some(player) = self.imp().player.get() {
            let tex: Option<gdk::Texture> = player.current_song_album_art();
            let imp = self.imp();
            let bg_paintable = imp.bg_paintable.clone();
            if imp.use_album_art_bg.get() && tex.is_some() {
                if !imp.content.has_css_class("no-shading") {
                    imp.content.add_css_class("no-shading");
                }
                bg_paintable.set_new_paintable(Some(tex.unwrap().upcast::<gdk::Paintable>()));
            }
            else {
                if imp.content.has_css_class("no-shading") {
                    imp.content.remove_css_class("no-shading");
                }
                bg_paintable.set_new_paintable(None);
            }

            // Run fade transition
            // Remember to queue draw too
            let anim_target = adw::CallbackAnimationTarget::new(
                clone!(
                    #[weak(rename_to = this)]
                    self,
                    #[weak]
                    bg_paintable,
                    move |progress: f64| {
                        bg_paintable.set_fade(progress);
                        this.queue_draw();
                    }
                )
            );
            let anim = adw::TimedAnimation::new(
                self,
                0.0, 1.0,
                (self.imp().transition_duration.get() * 1000.0).round() as u32,
                anim_target
            );
            anim.play();
        }
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
        let spinner = self.imp().busy_spinner.get();
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

        state
            .bind_property(
                "busy",
                &spinner,
                "visible"
            )
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
