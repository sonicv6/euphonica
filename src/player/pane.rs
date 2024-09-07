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
    cache::placeholders::ALBUMART_PLACEHOLDER, common::{paintables::FadePaintable, QualityGrade}, utils::settings_manager
};

use super::{
    Player, PlaybackControls,
    PlaybackState, VolumeKnob, MpdOutput
};

mod imp {
    use glib::{
        derived_properties, Properties
    };

    use super::*;

    #[derive(Default, Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::PlayerPane)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/player/pane.ui")]
    pub struct PlayerPane {
        // Song info
        #[template_child]
        pub info_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub albumart: TemplateChild<gtk::Picture>,
        #[template_child]
        pub song_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub artist: TemplateChild<gtk::Label>,
        #[template_child]
        pub album: TemplateChild<gtk::Label>,

        // TODO: Time-synced lyrics

        // Playback controls
        #[template_child]
        pub playback_controls: TemplateChild<PlaybackControls>,

        // Bottom: output info, volume control & quality
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
        #[template_child]
        pub quality_grade: TemplateChild<gtk::Image>,
        #[template_child]
        pub format_desc: TemplateChild<gtk::Label>,

        // Kept here so we can access it in snapshot()
        pub bg_paintable: FadePaintable,
        pub output_widgets: RefCell<Vec<MpdOutput>>,
        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set)]
        pub use_album_art_bg: Cell<bool>,
        #[property(get, set)]
        pub blur_radius: Cell<u32>,
        #[property(get, set)]
        pub opacity: Cell<f64>,
        #[property(get, set)]
        pub transition_duration: Cell<f64>,
        // Index of visible child in output_widgets
        pub current_output: Cell<usize>,
        pub output_count: Cell<usize>,

    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for PlayerPane {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaPlayerPane";
        type Type = super::PlayerPane;
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
    #[derived_properties]
    impl ObjectImpl for PlayerPane {
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
            let knob = self.vol_knob.get();
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
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for PlayerPane {
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

    impl BoxImpl for PlayerPane {}
}


glib::wrapper! {
    pub struct PlayerPane(ObjectSubclass<imp::PlayerPane>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PlayerPane {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PlayerPane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup(&self, player: Player) {
        self.setup_volume_knob(player.clone());
        self.bind_state(player.clone());
        self.imp().playback_controls.setup(player);
    }

    fn setup_volume_knob(&self, player: Player) {
        let knob = self.imp().vol_knob.get();
        knob.setup();

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
            self.imp().albumart.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
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
}
