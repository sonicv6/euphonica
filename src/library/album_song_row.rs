use std::{
    cell::{RefCell, OnceCell}
};
use gtk::{
    glib,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
    Label
};
use glib::{
    clone,
    Object,
    Binding,
    SignalHandlerId
};

use crate::{
    common::{Song, QualityGrade},
    utils::format_secs_as_duration
};

use super::Library;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/album-song-row.ui")]
    pub struct AlbumSongRow {
        #[template_child]
        pub quality_grade: TemplateChild<gtk::Image>,
        #[template_child]
        pub controls_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub replace_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub append_queue: TemplateChild<gtk::Button>,
        #[template_child]
        pub track_index: TemplateChild<Label>,
        #[template_child]
        pub song_name: TemplateChild<Label>,
        // Compilation/best-of albums usually have different artists.
        #[template_child]
        pub artist_name: TemplateChild<Label>,
        #[template_child]
        pub duration: TemplateChild<Label>,
        // Vector holding the bindings to properties of the Song GObject
        pub bindings: RefCell<Vec<Binding>>,
        // For unbinding the queue buttons when not bound to a song (i.e. being recycled)
        pub replace_queue_id: RefCell<Option<SignalHandlerId>>,
        pub append_queue_id: RefCell<Option<SignalHandlerId>>,
        pub library: OnceCell<Library>
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for AlbumSongRow {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaAlbumSongRow";
        type Type = super::AlbumSongRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for AlbumSongRow {}

    // Trait shared by all widgets
    impl WidgetImpl for AlbumSongRow {}

    // Trait shared by all boxes
    impl BoxImpl for AlbumSongRow {}
}

glib::wrapper! {
    pub struct AlbumSongRow(ObjectSubclass<imp::AlbumSongRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for AlbumSongRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AlbumSongRow {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn setup(&self, library: Library) {
        let _ = self.imp().library.set(library);
        // Reserve space since we know exactly how many IDs there are
        self.imp().bindings.borrow_mut().reserve_exact(7);
        // Must be called at setup time (use SignalListItemFactory::connect_setup)
        let revealer = self.imp().controls_revealer.get();
        let hover_ctl = gtk::EventControllerMotion::new();
        hover_ctl.connect_enter(
            clone!(
                #[weak]
                revealer,
                move |_,_,_| {
                    revealer.set_reveal_child(true);
                }
            )
        );
        hover_ctl.connect_leave(
            move |_| {
                revealer.set_reveal_child(false);
            }
        );
        self.add_controller(hover_ctl);
    }

    pub fn bind(&self, song: &Song) {
        let track_idx = self.imp().track_index.get();
        let duration = self.imp().duration.get();
        let song_name_label = self.imp().song_name.get();
        let artist_name_label = self.imp().artist_name.get();
        let quality_grade = self.imp().quality_grade.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        let track_idx_binding = song
            .bind_property("track", &track_idx, "label")
            .transform_to(|_, val: i64| {
                Some(val.to_string())
            })
            .sync_create()
            .build();
        // Save binding
        bindings.push(track_idx_binding);

        let track_idx_viz_binding = song
            .bind_property("track", &track_idx, "visible")
            .transform_to(|_, val: i64| {
                Some(val > 0)
            })
            .sync_create()
            .build();
        // Save binding
        bindings.push(track_idx_viz_binding);

        let duration_binding = song
            .bind_property("duration", &duration, "label")
            .transform_to(|_, val: u64| {
                Some(format_secs_as_duration(val as f64))
            })
            .sync_create()
            .build();
        // Save binding
        bindings.push(duration_binding);

        let song_name_binding = song
            .bind_property("name", &song_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(song_name_binding);

        let artist_name_binding = song
            .bind_property("artist", &artist_name_label, "label")
            .sync_create()
            .build();
        // Save binding
        bindings.push(artist_name_binding);

        let quality_binding = song
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
        bindings.push(quality_binding);

        let quality_viz_binding = song
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
        bindings.push(quality_viz_binding);

        // Bind the queue buttons
        let uri = song.get_uri();
        if let Some(old_id) = self.imp().replace_queue_id.replace(
            Some(
                self.imp().replace_queue.connect_clicked(
                    clone!(
                        #[weak(rename_to = this)]
                        self,
                        #[strong]
                        uri,
                        move |_| {
                            if let Some(library) = this.imp().library.get() {
                                library.queue_uri(&uri, true, true);
                            }
                        }
                    )
                )
            )
        ) {
            // Unbind old ID
            self.imp().replace_queue.disconnect(old_id);
        }
        if let Some(old_id) = self.imp().append_queue_id.replace(
            Some(
                self.imp().append_queue.connect_clicked(
                    clone!(
                        #[weak(rename_to = this)]
                        self,
                        #[strong]
                        uri,
                        move |_| {
                            if let Some(library) = this.imp().library.get() {
                                library.queue_uri(&uri, false, false);
                            }
                        }
                    )
                )
            )
        ) {
            // Unbind old ID
            self.imp().append_queue.disconnect(old_id);
        }
    }

    pub fn unbind(&self) {
        // Unbind all stored bindings
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
        if let Some(id) = self.imp().replace_queue_id.borrow_mut().take() {
            self.imp().replace_queue.disconnect(id);
        }
        if let Some(id) = self.imp().append_queue_id.borrow_mut().take() {
            self.imp().append_queue.disconnect(id);
        }
    }
}
