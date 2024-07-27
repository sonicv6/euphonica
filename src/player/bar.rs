use std::cell::{Cell, RefCell};
use gtk::{
    glib,
    gdk::Texture,
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};
use glib::{
    clone,
};
use async_channel::Sender;

use crate::{
    utils::format_secs_as_duration,
    client::MpdMessage,
    player::Player,
    common::QualityGrade
};

use super::{PlaybackState, VolumeKnob};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/player-bar.ui")]
    pub struct PlayerBar {
        // Left side: current song info
        #[template_child]
        pub info_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub seekbar_box: TemplateChild<gtk::CenterBox>,
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
        pub play_pause_symbol: TemplateChild<gtk::Image>,  // inside the play/pause button
        #[template_child]
        pub prev_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub seekbar: TemplateChild<gtk::Scale>,
        #[template_child]
        pub elapsed: TemplateChild<gtk::Label>,
        #[template_child]
        pub duration: TemplateChild<gtk::Label>,

        // TODO: Right side: output info
        #[template_child]
        pub vol_knob: TemplateChild<VolumeKnob>,

        // Handle to seekbar polling task
        pub seekbar_poller_handle: RefCell<Option<glib::JoinHandle<()>>>,
        // Temporary place for seekbar position before sending seekcur
        // TODO: move both of these into a custom seekbar widget.
        // pub new_position: Cell<f64>,
        pub seekbar_clicked: Cell<bool>
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
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for PlayerBar {}

    // Trait shared by all widgets
    impl WidgetImpl for PlayerBar {}

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

    pub fn setup(&self, player: Player, sender: Sender<MpdMessage>) {
        self.imp().vol_knob.setup();
        self.bind_state(player.clone(), sender.clone());
        self.setup_seekbar(player, sender);
    }

    fn bind_state(&self, player: Player, sender: Sender<MpdMessage>) {
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

        let seekbar_box = imp.seekbar_box.get();
        player
            .bind_property(
                "playback-state",
                &seekbar_box,
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
                "icon-name"
            )
            .transform_to(
                |_, state: PlaybackState| {
                    match state {
	                    PlaybackState::Playing => {
	                        Some("pause-large-symbolic")
                        },
	                    PlaybackState::Paused | PlaybackState::Stopped => {
	                        Some("play-large-symbolic")
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
            clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                sender,
                move |player, _| {
                    if player.playback_state() == PlaybackState::Playing {
                        this.maybe_start_polling(player.clone(), sender.clone());
                    }
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
                    let seekbar = this.imp().seekbar.get();
                    seekbar.set_range(0.0, player.duration() as f64);
                    seekbar.set_value(player.position());
                }
            ),
        );

        let elapsed = imp.elapsed.get();
        player
            .bind_property(
                "position",
                &elapsed,
                "label"
            )
            .transform_to(|_, pos| {
                Some(format_secs_as_duration(pos))
            })
            .sync_create()
            .build();

        let duration = imp.duration.get();
        player
            .bind_property(
                "duration",
                &duration,
                "label"
            )
            .transform_to(|_, dur: u64| {
                // If duration is 0s (no song), show --:-- instead
                if dur > 0 {
                    return Some(format_secs_as_duration(dur as f64));
                }
                Some("--:--".to_owned())

            })
            .sync_create()
            .build();

        self.update_album_art(player.album_art().as_ref());
        player.connect_notify_local(
            Some("album-art"),
            clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                player,
                move |_, _| {
                    this.update_album_art(player.album_art().as_ref());
                }
            )
        );

        self.imp().prev_btn.connect_clicked(
            clone!(
                #[strong]
                sender,
                move |_| {
                    let _ = sender.send_blocking(MpdMessage::Prev);
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
                sender,
                move |_| {
                    let _ = sender.send_blocking(MpdMessage::Next);
                }
            )
        );
    }

    fn update_album_art(&self, tex: Option<&Texture>) {
        // Use high-resolution version here
        if tex.is_some() {
            self.imp().albumart.set_paintable(tex);
        }
        else {
            self.imp().albumart.set_resource(
                Some("/org/euphonia/Euphonia/albumart-placeholder.png")
            );
        }
    }

    fn setup_seekbar(&self, player: Player,  sender: Sender<MpdMessage>) {
        // Capture mouse button release action for seekbar
        // Funny story: gtk::Scale has its own GestureClick controller which will eat up mouse button events.
        // Workaround: capture mouse button release event at window level in capture phase, using a bool
        // set by the seekbar's change-value signal to determine whether it is related to the seekbar or not.
        let seekbar_gesture = gtk::GestureClick::new();
        seekbar_gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        seekbar_gesture.connect_released(
            clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                sender,
                #[strong]
                player,
                move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::None); // allow propagating to seekbar
                    if this.imp().seekbar_clicked.get() {
                        let _ = sender.send_blocking(
                            MpdMessage::SeekCur(
                                this.imp().seekbar.adjustment().value()
                            )
                        );
                        this.imp().seekbar_clicked.replace(false);
                        this.maybe_start_polling(player.clone(), sender.clone());
                    }
                }
            )
        );

        self.imp().seekbar.connect_change_value(
            clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::signal::Propagation::Proceed,
                move |_, _, _| {
                    let _ = this.imp().seekbar_clicked.replace(true);
                    if let Some(handle) = this.imp().seekbar_poller_handle.take() {
                        handle.abort();
                    }
                    glib::signal::Propagation::Proceed
                }
            )
        );

        self.add_controller(seekbar_gesture);
    }

    fn maybe_start_polling(&self, player: Player, sender: Sender<MpdMessage>) {
        // Periodically poll for player progress to update seekbar
        // Won't start a new loop if there is already one
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
}
