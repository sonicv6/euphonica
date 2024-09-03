use std::cell::{Cell, RefCell};
use gtk::{
    gdk, glib::{self, Variant}, graphene, prelude::*, subclass::prelude::*, CompositeTemplate
};
use adw::prelude::*;
use glib::{
    clone,
    closure_local,
    BoxedAnyObject
};
use mpd::output::Output;

use crate::{
    utils::settings_manager,
    common::{QualityGrade, paintables::FadePaintable}
};

use super::{
    Player, Seekbar,
    PlaybackState, VolumeKnob, MpdOutput
};

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecBoolean,
        ParamSpecUInt,
        ParamSpecDouble
    };
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/player/player-bar.ui")]
    pub struct PlayerBar {
        // Left side: current song info
        #[template_child]
        pub info_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub albumart: TemplateChild<gtk::Image>,
        #[template_child]
        pub song_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub artist: TemplateChild<gtk::Label>,
        #[template_child]
        pub album: TemplateChild<gtk::Label>,
        #[template_child]
        pub quality_grade: TemplateChild<gtk::Image>,
        #[template_child]
        pub format_desc: TemplateChild<gtk::Label>,

        // Centre: playback controls
        #[template_child]
        pub play_pause_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub play_pause_symbol: TemplateChild<gtk::Stack>,  // inside the play/pause button
        #[template_child]
        pub prev_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub seekbar: TemplateChild<Seekbar>,

        // Right side: output info & volume control
        #[template_child]
        pub output_section: TemplateChild<gtk::Box>,
        #[template_child]
        pub output_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub prev_output: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_output: TemplateChild<gtk::Button>,
        #[template_child]
        pub vol_knob: TemplateChild<VolumeKnob>,

        // Kept here so we can access it in snapshot()
        pub bg_paintable: FadePaintable,
        pub output_widgets: RefCell<Vec<MpdOutput>>,
        // Kept here so snapshot() does not have to query GSettings on every frame
        pub use_album_art_bg: Cell<bool>,
        pub blur_radius: Cell<u32>,
        pub opacity: Cell<f64>,
        pub transition_duration: Cell<f64>,
        // Index of visible child in output_widgets
        pub current_output: Cell<usize>,
        pub output_count: Cell<usize>,

    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for PlayerBar {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaPlayerBar";
        type Type = super::PlayerBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_layout_manager_type::<gtk::BoxLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for PlayerBar {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::builder("use-album-art-bg").build(),
                    ParamSpecDouble::builder("transition-duration").build(),
                    ParamSpecDouble::builder("opacity").build(),
                    ParamSpecUInt::builder("blur-radius").build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            match pspec.name() {
                "use-album-art-bg" => self.use_album_art_bg.get().to_value(),
                "transition-duration" => self.transition_duration.get().to_value(),
                "opacity" => self.opacity.get().to_value(),
                "blur-radius" => self.blur_radius.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "use-album-art-bg" => {
                    // Always set to title tag
                    if let Ok(b) = value.get::<bool>() {
                        let old = self.use_album_art_bg.replace(b);
                        if old != b {
                            obj.notify("use-album-art-bg");
                            obj.queue_draw();
                        }
                    }
                }
                "transition-duration" => {
                    if let Ok(val) = value.get::<f64>() {
                        let old = self.transition_duration.replace(val);
                        if old != val {
                            obj.notify("transition-duration");
                            // No need to queue draw for this (no immediate change)
                        }
                    }
                }
                "opacity" => {
                    if let Ok(val) = value.get::<f64>() {
                        let old = self.opacity.replace(val);
                        if old != val {
                            obj.notify("opacity");
                            obj.queue_draw();
                        }
                    }
                }
                "blur-radius" => {
                    if let Ok(val) = value.get::<u32>() {
                        let old = self.blur_radius.replace(val);
                        if old != val {
                            obj.notify("blur-radius");
                            obj.queue_draw();
                        }
                    }
                }
                _ => unimplemented!(),
            }
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for PlayerBar {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;

            // Bluuuuuur
            // Adopted from Nanling Zheng's implementation for Gapless.
            if self.use_album_art_bg.get() {
                let bg_width = self.bg_paintable.intrinsic_width() as f32;
                let bg_height = self.bg_paintable.intrinsic_height() as f32;
                let scale_x = width / bg_width as f32;
                let scale_y = height / bg_height as f32;
                let scale_max = scale_x.max(scale_y);
                let view_width = bg_width * scale_max;
                let view_height = bg_height * scale_max;
                let delta_x = (width - view_width) * 0.5;
                let delta_y = (height - view_height) * 0.5;
                // Crop background to only the bottom bar
                snapshot.push_clip(&graphene::Rect::new(
                    0.0, 0.0, width, height
                ));
                snapshot.translate(&graphene::Point::new(
                    delta_x, delta_y
                ));
                // Blur & opacity nodes
                let blur_radius = self.blur_radius.get();
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

    // Trait shared by all boxes
    impl BoxImpl for PlayerBar {}
}


glib::wrapper! {
    pub struct PlayerBar(ObjectSubclass<imp::PlayerBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PlayerBar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PlayerBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup(&self, player: Player) {
        self.bind_settings();
        self.setup_volume_knob(player.clone());
        self.bind_state(player.clone());
        self.setup_seekbar(player);
    }

    fn bind_settings(&self) {
        let settings = settings_manager().child("player");
        settings
            .bind(
                "use-album-art-as-bg",
                self,
                "use-album-art-bg"
            )
            .build();

        settings
            .bind(
                "bg-blur-radius",
                self,
                "blur-radius"
            )
            .build();

        settings
            .bind(
                "bg-opacity",
                self,
                "opacity"
            )
            .build();

        settings
            .bind(
                "bg-transition-duration-s",
                self,
                "transition-duration"
            )
            .build();
    }

    fn setup_volume_knob(&self, player: Player) {
        let settings = settings_manager().child("player");
        let knob = self.imp().vol_knob.get();
        knob.setup();

        settings
            .bind(
                "vol-knob-unit",
                &knob,
                "use-dbfs"
            )
            .get_only()
            .mapping(|v: &Variant, _| {
                Some((v.get::<String>().unwrap().as_str() == "decibels").to_value())
            })
            .build();

        settings
            .bind(
                "vol-knob-sensitivity",
                &knob,
                "sensitivity"
            )
            .mapping(|v: &Variant, _| { Some(v.get::<f64>().unwrap().to_value())})
            .build();

        knob.connect_notify_local(
            Some("value"),
            clone!(
                #[weak]
                player,
                move |knob: &VolumeKnob, _| {
                    player.set_volume(knob.value().round() as i8);
                }
            )
        );

        knob.connect_notify_local(
            Some("is-muted"),
            clone!(
                #[weak]
                player,
                move |knob: &VolumeKnob, _| {
                    if knob.is_muted() {
                        player.set_volume(0);
                    }
                    else {
                        // Restore previous volume
                        player.set_volume(knob.value().round() as i8);
                    }
                }
            )
        );

        // Only fired for EXTERNAL changes.
        player.connect_closure(
            "volume-changed",
            false,
            closure_local!(
                |_: Player, val: i8| {
                    knob.sync_value(val);
                }
            )
        );
    }

    fn bind_state(&self, player: Player) {
        let imp = self.imp();
        let info_box = imp.info_box.get();
        player
            .bind_property(
                "playback-state",
                &info_box,
                "visible"
            )
            .transform_to(|_, state: PlaybackState| {
                Some(state != PlaybackState::Stopped)
            })
            .sync_create()
            .build();

        let song_name = imp.song_name.get();
        player
            .bind_property(
                "title",
                &song_name,
                "label"
            )
            .sync_create()
            .build();

        let album = imp.album.get();
        player
            .bind_property(
                "album",
                &album,
                "label"
            )
            .sync_create()
            .build();

        let artist = imp.artist.get();
        player
            .bind_property(
                "artist",
                &artist,
                "label"
            )
            .sync_create()
            .build();

        let quality_grade = imp.quality_grade.get();
        player
            .bind_property(
                "quality-grade",
                &quality_grade,
                "icon-name"
            )
            .transform_to(|_, grade: QualityGrade| {
                Some(grade.to_icon_name())}
            )
            .sync_create()
            .build();

        player
            .bind_property(
                "quality-grade",
                &quality_grade,
                "visible"
            )
            .transform_to(|_, grade: QualityGrade| {
                Some(grade != QualityGrade::Lossy)
            })
            .sync_create()
            .build();

        let format_desc = imp.format_desc.get();
        player
            .bind_property(
                "format-desc",
                &format_desc,
                "label"
            )
            .sync_create()
            .build();

        let play_pause_symbol = imp.play_pause_symbol.get();
        player
            .bind_property(
                "playback-state",
                &play_pause_symbol,
                "visible-child-name"
            )
            .transform_to(
                |_, state: PlaybackState| {
                    match state {
	                    PlaybackState::Playing => {
	                        Some("play")
                        },
	                    PlaybackState::Paused | PlaybackState::Stopped => {
	                        Some("pause")
	                    },
	                }
                }
            )
            .sync_create()
            .build();

        // Only make player update seekbar upon change.
        // Do not bind directly as that will prevent us from sliding the
        // seekbar to begin with.
        player.connect_notify_local(
            Some("playback-state"),
            |player, _| {
                if player.playback_state() == PlaybackState::Playing {
                    player.maybe_start_polling();
                }
            }
        );

        player.connect_closure(
            "outputs-changed",
            false,
            closure_local!(
                #[strong(rename_to = this)]
                self,
                move |player: Player, outputs: BoxedAnyObject| {
                    this.update_outputs(player, outputs.borrow::<Vec<Output>>().as_ref());
                }
            )
        );

        self.update_album_art(player.current_song_album_art());
        player.connect_notify_local(
            Some("album-art"),
            clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                player,
                move |_, _| {
                    this.update_album_art(player.current_song_album_art());
                }
            )
        );

        self.imp().prev_btn.connect_clicked(
            clone!(
                #[strong]
                player,
                move |_| {
                    player.prev_song();
                }
            )
        );
        self.imp().play_pause_btn.connect_clicked(
            clone!(
                #[weak]
                player,
                move |_| {
                    player.toggle_playback()
                }
            )
        );
        self.imp().next_btn.connect_clicked(
            clone!(
                #[strong]
                player,
                move |_| {
                    player.next_song();
                }
            )
        );

        self.imp().prev_output.connect_clicked(
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.prev_output();
                }
            )
        );

        self.imp().next_output.connect_clicked(
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.next_output();
                }
            )
        );
    }

    fn update_album_art(&self, tex: Option<gdk::Texture>) {
        // Use high-resolution version here
        // Update cover paintable
        if tex.is_some() {
            self.imp().albumart.set_paintable(tex.as_ref());
        }
        else {
            self.imp().albumart.set_resource(
                Some("/org/euphonia/Euphonia/albumart-placeholder.png")
            );
        }
        // Update blurred background
        let bg_paintable = self.imp().bg_paintable.clone();
        if let Some(tex) = tex {
            bg_paintable.set_new_paintable(Some(tex.upcast::<gdk::Paintable>()));
        }
        else {
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

    fn update_outputs(&self, player: Player, outputs: &[Output]) {
        let section = self.imp().output_section.get();
        let stack = self.imp().output_stack.get();
        let old_widgets = self.imp().output_widgets.replace(Vec::with_capacity(0));
        // Clear stack
        for widget in old_widgets {
            stack.remove(&widget);
        }
        if outputs.is_empty() {
            section.set_visible(false);
            let _ = self.imp().output_count.replace(0);
        }
        else {
            section.set_visible(true);
            // Handle buttons
            if outputs.len() > 1 {
                self.imp().prev_output.set_visible(true);
                self.imp().next_output.set_visible(true);
            }
            else {
                self.imp().prev_output.set_visible(false);
                self.imp().next_output.set_visible(false);
            }
            let new_widgets: Vec<MpdOutput> = outputs.iter().map(|v| {
                MpdOutput::from_output(v, player.clone())
            }).collect();
            // Add new ones
            for widget in &new_widgets {
                stack.add_child(widget);
            }
            let _ = self.imp().output_count.replace(new_widgets.len());
            let _ = self.imp().output_widgets.replace(new_widgets);
            // Call this once to update button sensitivities & visible child
            self.set_visible_output(self.imp().current_output.get() as i32);
        }
    }

    fn set_visible_output(&self, new_idx: i32) {
        if self.imp().output_count.get() > 0 {
            let max = self.imp().output_count.get() - 1;
            if new_idx as usize >= max {
                let _ = self.imp().current_output.replace(max);
                self.imp().next_output.set_sensitive(false);
                self.imp().prev_output.set_sensitive(true);
            }
            else if new_idx <= 0 {
                let _ = self.imp().current_output.replace(0);
                self.imp().next_output.set_sensitive(true);
                self.imp().prev_output.set_sensitive(false);
            }
            else {
                let _ = self.imp().current_output.replace(new_idx as usize);
                self.imp().next_output.set_sensitive(true);
                self.imp().prev_output.set_sensitive(true);
            }

            // Update stack
            self.imp().output_stack.set_visible_child(
                &self.imp().output_widgets.borrow()[
                    self.imp().current_output.get()
                ]
            );
        }
    }

    fn next_output(&self) {
        self.set_visible_output(self.imp().current_output.get() as i32 + 1);
    }

    fn prev_output(&self) {
        self.set_visible_output(self.imp().current_output.get() as i32 - 1);
    }

    fn setup_seekbar(&self, player: Player) {
        let seekbar = self.imp().seekbar.get();
        seekbar.connect_closure(
            "pressed",
            false,
            closure_local!(
                #[weak]
                player,
                move |_: Seekbar| {
                    player.block_polling();
                    player.stop_polling();
                }
            )
        );

        seekbar.connect_closure(
            "released",
            false,
            closure_local!(
                #[weak]
                player,
                move |seekbar: Seekbar| {
                    player.unblock_polling();
                    player.seek(seekbar.value());
                    // Player will start polling again on next status update,
                    // which should be triggered by us seeking.
                }
            )
        );

        player.connect_notify_local(
            Some("position"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |player, _| {
                    this.imp().seekbar.set_value(player.position());
                }
            ),
        );
        player.connect_notify_local(
            Some("duration"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |player, _| {
                    this.imp().seekbar.set_duration(player.duration() as f64);
                }
            ),
        );
    }
}
