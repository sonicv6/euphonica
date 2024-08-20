use std::{
    cell::RefCell,
    rc::Rc,
    f64::consts::PI
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    Label,
    Image
};
use glib::{
    closure_local,
    Object,
    Binding,
    signal::SignalHandlerId
};

use crate::{
    utils::strip_filename_linux,
    common::Song,
    cache::{
        Cache,
        CacheState,
        placeholders::ALBUMART_PLACEHOLDER
    }
};

// fn ease_in_out_sine(progress: f64) -> f64 {
//     (1.0 - (progress * PI).cos()) / 2.0
// }

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/player/queue-row.ui")]
    pub struct QueueRow {
        #[template_child]
        pub thumbnail: TemplateChild<Image>,
        // #[template_child]
        // pub marquee: TemplateChild<Viewport>,
        #[template_child]
        pub song_name: TemplateChild<Label>,
         #[template_child]
        pub album_name: TemplateChild<Label>,
         #[template_child]
        pub artist_name: TemplateChild<Label>,
        #[template_child]
        pub playing_indicator: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Song GObject
        pub bindings: RefCell<Vec<Binding>>,
        // pub playing_signal_id: RefCell<Option<SignalHandlerId>>,
        pub thumbnail_signal_id: RefCell<Option<SignalHandlerId>>,
        // pub marquee_tick_callback_id: RefCell<Option<TickCallbackId>>,
        // pub marquee_forward: Cell<bool>,
        // pub marquee_progress: Cell<f64>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for QueueRow {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaQueueRow";
        type Type = super::QueueRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for QueueRow {}

    // Trait shared by all widgets
    impl WidgetImpl for QueueRow {}

    // Trait shared by all boxes
    impl BoxImpl for QueueRow {}
}

glib::wrapper! {
    pub struct QueueRow(ObjectSubclass<imp::QueueRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for QueueRow {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueRow {
    pub fn new() -> Self {
        let res: Self = Object::builder().build();

        // // Bind marquee controller only once here
        // let marquee = res.imp().marquee.get();
        // // Run marquee while hovered
        // let hover_ctl = EventControllerMotion::new();
        // hover_ctl.set_propagation_phase(gtk::PropagationPhase::Capture);
        // hover_ctl.connect_enter(clone!(@weak res as this => move |_, _, _| {
        //     this.start_marquee();
        // }));
        // hover_ctl.connect_leave(clone!(@weak res as this => move |_| {
        //     // Remove the marquee movement callback & set its position back to 0.
        //     if let Some(id) = this.imp().marquee_tick_callback_id.take() {
        //         id.remove();
        //     }
        //     marquee.hadjustment().set_value(
        //         marquee.hadjustment().lower()
        //     );
        // }));
        // res.add_controller(hover_ctl);

        res
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

    fn update_thumbnail(&self, folder_uri: &str, cache: Rc<Cache>) {
        if let Some(tex) = cache.load_local_album_art(folder_uri, true) {
            self.imp().thumbnail.set_paintable(Some(&tex));
        }
        else {
            self.imp().thumbnail.set_paintable(Some(&*ALBUMART_PLACEHOLDER))
        }
    }

    pub fn bind(&self, song: &Song, cache: Rc<Cache>) {
        // Get state
        let thumbnail_image = self.imp().thumbnail.get();
        let song_name_label = self.imp().song_name.get();
        let album_name_label = self.imp().album_name.get();
        let artist_name_label = self.imp().artist_name.get();
        let playing_label = self.imp().playing_indicator.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        // Set once first (like sync_create)
        self.update_thumbnail(strip_filename_linux(song.get_uri()), cache.clone());
        let thumbnail_binding = cache.get_cache_state().connect_closure(
            "album-art-downloaded",
            false,
            closure_local!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                song,
                #[weak]
                cache,
                move |_: CacheState, folder_uri: String| {
                    if strip_filename_linux(song.get_uri()) == folder_uri {
                        this.update_thumbnail(folder_uri.as_ref(), cache)
                    }
                }
            )
        );
        self.imp().thumbnail_signal_id.replace(Some(thumbnail_binding));

        let song_name_binding = song
            .bind_property("name", &song_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_name_binding);

        let album_name_binding = song
            .bind_property("album", &album_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(album_name_binding);

        let artist_name_binding = song
            .bind_property("artist", &artist_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(artist_name_binding);

        let song_is_playing_binding = song
            .bind_property("is-playing", &playing_label, "visible")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_is_playing_binding);

        // Set once first (like sync_create)
        // if song.is_playing() {
        //     self.start_marquee();
        // }
        // let playing_binding = song
        //     .connect_notify_local(
        //         Some("is-playing"),
        //         clone!(@weak self as this => move |this_song, _| {
        //             if this_song.is_playing() {
        //                 this.start_marquee();
        //             }
        //             else {
        //                 this.stop_marquee();
        //             }
        //         }),
        //     );
        // self.imp().playing_signal_id.replace(Some(playing_binding));
    }

    pub fn unbind(&self, cache: Rc<Cache>) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().thumbnail_signal_id.take() {
            cache.get_cache_state().disconnect(id);
        }

        // if let Some(id) = self.imp().playing_signal_id.take() {
        //     song.disconnect(id);
        // }
    }
}
