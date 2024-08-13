use std::{
    cell::Cell,
    f64::consts::PI
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    cairo
};
use cairo::LineCap;
use glib::{
    clone,
    Object,
};

fn convert_to_dbfs(pct: f64) -> Result<f64, ()> {
    // Accepts 0-100
    if pct > 0.0 && pct < 100.0 {
        return Ok(10.0 * (pct / 100.0).log10());
    }
    Err(())
}

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecDouble,
        ParamSpecBoolean
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/volume-knob.ui")]
    pub struct VolumeKnob {
        #[template_child]
        pub draw_area: TemplateChild<gtk::DrawingArea>,
        #[template_child]
        pub readout: TemplateChild<gtk::Label>,
        #[template_child]
        pub unit: TemplateChild<gtk::Label>,
        #[template_child]
        pub readout_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub knob_btn: TemplateChild<gtk::ToggleButton>,
        // Stored here & bound to the settings manager so we can avoid having
        // to query the setting on every frame while scrolling.
        pub sensitivity: Cell<f64>,
        pub use_dbfs: Cell<bool>,
        // 0 to 100. Full precision for smooth scrolling effect.
        pub value: Cell<f64>,
        pub is_muted: Cell<bool>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for VolumeKnob {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaVolumeKnob";
        type Type = super::VolumeKnob;
        type ParentType = gtk::Box;

        fn new() -> Self {
            Self {
                draw_area: TemplateChild::default(),
                readout: TemplateChild::default(),
                unit: TemplateChild::default(),
                readout_stack: TemplateChild::default(),
                knob_btn: TemplateChild::default(),
                use_dbfs: Cell::new(false),
                sensitivity: Cell::new(1.0),
                value: Cell::new(0.0),
                is_muted: Cell::new(false)
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for VolumeKnob {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    // Only modifiable via internal setter
                    ParamSpecDouble::builder("value").read_only().build(),
                    ParamSpecDouble::builder("sensitivity").build(),
                    ParamSpecBoolean::builder("is-muted").build(),
                    ParamSpecBoolean::builder("use-dbfs").build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            match pspec.name() {
                "value" => self.value.get().to_value(),
                "is-muted" => self.is_muted.get().to_value(),
                "sensitivity" => self.sensitivity.get().to_value(),
                "use-dbfs" => self.use_dbfs.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "sensitivity" => {
                    if let Ok(s) = value.get::<f64>() {
                        // No checks performed here (UI widget should be a GtkScale).
                        let old_sensitivity = self.sensitivity.replace(s);
                        if old_sensitivity != s {
                            obj.notify("sensitivity");
                        }
                    }
                },
                "is-muted" => {
                    if let Ok(is_muted) = value.get::<bool>() {
                        let was_muted = self.is_muted.replace(is_muted);
                        if was_muted != is_muted {
                            obj.notify("is-muted");
                        }
                    }
                },
                "use-dbfs" => {
                    if let Ok(b) = value.get::<bool>() {
                        let old_use_dbfs = self.use_dbfs.replace(b);
                        if old_use_dbfs != b {
                            obj.notify("use-dbfs");
                            obj.notify("value");  // Fire this too to redraw the readout
                        }
                    }
                },
                _ => unimplemented!(),
            }
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for VolumeKnob {}

    // Trait shared by all boxes
    impl BoxImpl for VolumeKnob {}
}

glib::wrapper! {
    pub struct VolumeKnob(ObjectSubclass<imp::VolumeKnob>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for VolumeKnob {
    fn default() -> Self {
        Self::new()
    }
}

impl VolumeKnob {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn sensitivity(&self) -> f64 {
        self.imp().sensitivity.get()
    }

    pub fn use_dbfs(&self) -> bool {
        self.imp().use_dbfs.get()
    }

    pub fn value(&self) -> f64 {
        self.imp().value.get()
    }

    pub fn is_muted(&self) -> bool {
        self.imp().is_muted.get()
    }

    pub fn set_value(&self, val: f64) {
        let old_val = self.imp().value.replace(val);
        if old_val != val {
            self.notify("value");
        }
    }

    pub fn sync_value(&self, new_rounded: i8) {
        // Set volume based on rounded i8 value silently.
        // Useful for syncing to external changes.
        // Will only update our full-precision value when it's "different" enough.
        let old_rounded = self.imp().value.get().round() as i8;
        if old_rounded != new_rounded {
            let _ = self.imp().value.replace(new_rounded as f64);
            self.notify("value");
            // Will not emit a signal (doing so would result in an infinite loop
            // between parent widget and this one).
        }
    }

    fn update_readout(&self) {
        let readout = self.imp().readout.get();
        let val = self.imp().value.get();
        if self.imp().use_dbfs.get() {
            if let Ok(dbfs) = convert_to_dbfs(val) {
                readout.set_label(&format!("{:.2}", dbfs));
            }
            else if val > 0.0 {
                readout.set_label("0");
            }
            else {
                readout.set_label("-∞");
            }
        }
        else {
            readout.set_label(&format!("{:.0}", val));
        }
    }

    pub fn setup(&self) {
        let imp = self.imp();
        // Bind readouts
        let knob_btn = imp.knob_btn.get();
        let readout_stack = imp.readout_stack.get();
        knob_btn
            .bind_property(
                "active",
                &readout_stack,
                "visible-child-name"
            )
            .transform_to(|_, active: bool| {
                if active {
                    return Some("mute");
                }
                Some("readout")
            })
            .sync_create()
            .build();

        knob_btn
            .bind_property(
                "active",
                self,
                "is-muted"
            )
            .sync_create()
            .build();

        self.update_readout();
        self.connect_notify_local(
            Some("value"),
            |this, _| {
                this.update_readout();
            }
        );
        let unit = imp.unit.get();
        self
            .bind_property(
                "use-dbfs",
                &unit,
                "label"
            )
            .transform_to(|_, use_dbfs: bool| {
                if use_dbfs {
                    return Some("dBFS");
                }
                Some("%")
            })
            .sync_create()
            .build();
        // Draw curve from 0 to current volume level
        // (goes from 7:30 to 4:30 CW, which is -270deg to 45deg for cairo_arc).
        // Currently hardcoding diameter to 96px.
        let draw_area = imp.draw_area.get();
        draw_area.set_draw_func(
            clone!(
                #[weak(rename_to = this)]
                self,
                move |_, cr, w, h| {
                    let fg = this.color();
                    cr.set_source_rgb(
                        fg.red() as f64,
                        fg.green() as f64,
                        fg.blue() as f64,
                    );
                    cr.set_line_width(5.0);
                    cr.set_line_cap(LineCap::Round);
                    // Starting
                    // At 0 => 5pi/4
                    let angle = -1.25 * PI + 1.5 * PI * this.imp().value.get() / 100.0;
                    // u w0t m8
                    cr.arc(
                        w as f64 / 2.0,
                        h as f64 / 2.0,
                        40.0,
                        -1.25 * PI, angle
                    );
                    let _ = cr.stroke();
                }
            )
        );

        // Enable scrolling to change volume
        // TODO: Implement vertical dragging & keyboard controls
        // TODO: Let user control scroll sensitivity
        let scroll_ctl = gtk::EventControllerScroll::default();
        scroll_ctl.set_flags(gtk::EventControllerScrollFlags::VERTICAL);
        scroll_ctl.set_propagation_phase(gtk::PropagationPhase::Capture);
        scroll_ctl.connect_scroll(
            clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::signal::Propagation::Proceed,
                move |_, _, dy| {
                    let new_vol = this.imp().value.get() - dy * this.sensitivity();
                    if (0.0..=100.0).contains(&new_vol) {
                        this.set_value(new_vol);
                    }
                    this.imp().draw_area.queue_draw();
                    glib::signal::Propagation::Proceed
                }
            )
        );
        self.add_controller(scroll_ctl);

        // Update level arc upon changing foreground colour, for example when switching dark/light mode
        self.connect_notify_local(
            Some("color"),
            |this, _| {
                this.imp().draw_area.queue_draw();
            }
        );
    }

    // fn start_marquee(&self) {
    //     let marquee = self.imp().marquee.get();
    //     let adj = marquee.hadjustment().expect("No adjustment?");
    //     self.imp().marquee_forward.replace(true);
    //     self.imp().marquee_progress.replace(0.0);
    //     let this = self.clone();
    //     let id = marquee.add_tick_callback(move |_, frame_clock| {
    //         // TODO: customisable interval. For now hardcoding to 5000ms each direction (10s full cycle).
    //         // Full range = upper - page_size, where page is the "content width" and upper is
    //         // the maximum "coordinate" that can be seen by the ScrolledWindow, i.e. the far end
    //         // of the content.
    //         // Value on the other hand is the "coordinate" of the beginning of the content.
    //         // Recalculate range at every tick since user might have resized the window.
    //         let range = adj.upper() - adj.page_size();
    //         if range > 0.0 {
    //             let progress_step = (1000.0 / frame_clock.fps()) / 5000.0;  // in milliseconds
    //             // Calculate progress value at next frame.
    //             if this.imp().marquee_forward.get() {
    //                 let next_progress = this.imp().marquee_progress.get() + progress_step;
    //                 if next_progress >= 1.0 {
    //                     // Do not advance. Instead, simply flip direction for next frame.
    //                     let _ = this.imp().marquee_forward.replace(false);
    //                 }
    //                 else {
    //                     // Not at the end yet => advance
    //                     let next_value = ease_in_out_sine(next_progress);
    //                     let _ = this.imp().marquee_progress.replace(next_progress);
    //                     adj.set_value(next_value * range);
    //                 }
    //             }
    //             else {
    //                 let next_progress = this.imp().marquee_progress.get() - progress_step;
    //                 if next_progress <= 0.0 {
    //                     let _ = this.imp().marquee_forward.replace(true);
    //                 }
    //                 else {
    //                     // Not at the end yet => advance
    //                     let next_value = ease_in_out_sine(next_progress);
    //                     let _ = this.imp().marquee_progress.replace(next_progress);
    //                     adj.set_value(next_value * range);
    //                 }
    //             }
    //         }
    //         ControlFlow::Continue
    //     });
    //     if let Some(old_id) = self.imp().marquee_tick_callback_id.replace(Some(id)) {
    //         old_id.remove();
    //     }
    // }

    // fn stop_marquee(&self) {
    //     let marquee = self.imp().marquee.get();
    //     // Remove the marquee movement callback & set its position back to 0.
    //     if let Some(id) = self.imp().marquee_tick_callback_id.take() {
    //         id.remove();
    //     }
    //     let adj = marquee.hadjustment().expect("No adjustment?");
    //     adj.set_value(
    //         adj.lower()
    //     );
    // }

    // pub fn bind(&self,) {
    //     // Get state
    //     let thumbnail_image = self.imp().thumbnail.get();
    //     let song_name_label = self.imp().song_name.get();
    //     let album_name_label = self.imp().album_name.get();
    //     let artist_name_label = self.imp().artist_name.get();
    //     let playing_label = self.imp().playing_indicator.get();
    //     let mut bindings = self.imp().bindings.borrow_mut();

    //     // Set once first (like sync_create)
    //     let thumbnail = song.get_thumbnail();
    //     // println!("Thumbnail exists: {}", thumbnail.is_some());
    //     thumbnail_image.set_paintable(thumbnail.as_ref());
    //     let thumbnail_binding = song
    //         .connect_notify_local(
    //             Some("thumbnail"),
    //             move |this_song, _| {
    //                 let thumbnail = this_song.get_thumbnail();
    //                 // println!("Thumbnail exists: {}", thumbnail.is_some());
    //                 thumbnail_image.set_paintable(thumbnail.as_ref());
    //             },
    //         );
    //     self.imp().thumbnail_signal_id.replace(Some(thumbnail_binding));

    //     let song_name_binding = song
    //         .bind_property("name", &song_name_label, "label")
    //         .sync_create()
    //         .build();
    //     // Save binding
    //     bindings.push(song_name_binding);

    //     let album_name_binding = song
    //         .bind_property("album", &album_name_label, "label")
    //         .sync_create()
    //         .build();
    //     // Save binding
    //     bindings.push(album_name_binding);

    //     let artist_name_binding = song
    //         .bind_property("artist", &artist_name_label, "label")
    //         .sync_create()
    //         .build();
    //     // Save binding
    //     bindings.push(artist_name_binding);

    //     let song_is_playing_binding = song
    //         .bind_property("is-playing", &playing_label, "visible")
    //         .sync_create()
    //         .build();
    //     // Save binding
    //     bindings.push(song_is_playing_binding);

    //     // Set once first (like sync_create)
    //     // if song.is_playing() {
    //     //     self.start_marquee();
    //     // }
    //     // let playing_binding = song
    //     //     .connect_notify_local(
    //     //         Some("is-playing"),
    //     //         clone!(@weak self as this => move |this_song, _| {
    //     //             if this_song.is_playing() {
    //     //                 this.start_marquee();
    //     //             }
    //     //             else {
    //     //                 this.stop_marquee();
    //     //             }
    //     //         }),
    //     //     );
    //     // self.imp().playing_signal_id.replace(Some(playing_binding));
    // }

    // pub fn unbind(&self, song: &Song) {
    //     // Unbind all stored bindings
    //     for binding in self.imp().bindings.borrow_mut().drain(..) {
    //         binding.unbind();
    //     }
    //     if let Some(id) = self.imp().thumbnail_signal_id.take() {
    //         song.disconnect(id);
    //     }

    //     // if let Some(id) = self.imp().playing_signal_id.take() {
    //     //     song.disconnect(id);
    //     // }
    // }
}
