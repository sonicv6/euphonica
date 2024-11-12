use std::cell::Cell;
use gtk::{
    gdk::{self, subclass::paintable::*}, prelude::*, subclass::prelude::*, glib
};

use crate::common::paintables::BlurPaintable;

// Background paintable implementation.
// Euphonica can optionally use the currently-playing track's album art as its
// background. This is always scaled to fill the whole window and can be further
// blurred. When the next song has a different album art, a fade animation will
// be played.
mod imp {
    use glib::Properties;

    use crate::common::paintables::BlurPaintable;

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::FadePaintable)]
    pub struct FadePaintable {
        pub current: BlurPaintable,
        pub previous: BlurPaintable,
        // 0 = previous, 0.5 = halfway, 1.0 = current
        pub fade: Cell<f64>,
        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set)]
        pub transition_duration: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FadePaintable {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaFadePaintable";
        type Type = super::FadePaintable;
        type Interfaces = (gdk::Paintable,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for FadePaintable {}

    impl PaintableImpl for FadePaintable {
        fn current_image(&self) -> gdk::Paintable {
            self.current.current_image()
        }

        fn intrinsic_width(&self) -> i32 {
            self.current.intrinsic_width()
        }

        fn intrinsic_height(&self) -> i32 {
            self.current.intrinsic_height()
        }

        fn intrinsic_aspect_ratio(&self) -> f64 {
            self.current.intrinsic_aspect_ratio()
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            // "Fade" is defined as the progress of the transition from the previous to the current texture.
            // A value of 0.0 indicates that only the previous texture is visible, while 1.0 means only the
            // current texture is visible.
            // Fading procedure:
            // 1. Check if there's a current texture. If there is:
            //     1a. If there is no previous texture and fade != 1.0, then we're fading in from nothing.
            //     Draw the current picture at fade opacity.
            //     1b. Else (there is a previous texture and/or fade is at 1.0), just draw at full opacity.
            // 2. Check if there's a previous texture. If there is and fade < 1.0, draw it at 1-fade opacity.
            let fade = self.fade.get();
            if self.current.has_content() {
                if self.previous.has_content() && fade < 1.0 {
                    snapshot.push_opacity(fade);
                    self.current.snapshot(snapshot, width, height);
                    snapshot.pop();
                }
                else {
                    self.current.snapshot(snapshot, width, height);
                }
            }
            if self.previous.has_content() && fade < 1.0 {
                snapshot.push_opacity(1.0 - fade);
                self.previous.snapshot(snapshot, width, height);
                snapshot.pop();
            }
        }
    }
}

glib::wrapper! {
    pub struct FadePaintable(ObjectSubclass<imp::FadePaintable>) @implements gdk::Paintable;
}

impl FadePaintable {
    pub fn current(&self) -> &BlurPaintable {
        &self.imp().current
    }

    pub fn previous(&self) -> &BlurPaintable {
        &self.imp().previous
    }

    pub fn get_fade(&self) -> f64 {
        self.imp().fade.get()
    }

    pub fn set_fade(&self, new: f64) {
        let true_new = new.clamp(0.0, 1.0);
        let old = self.imp().fade.replace(true_new);
        if old != true_new {
            self.invalidate_contents();
        }
    }

    pub fn set_new_paintable(&self, new: Option<gdk::Paintable>) {
        self.imp().previous.set_content(self.imp().current.content());
        self.imp().current.set_content(new);
        self.set_fade(0.0);
    }
}

impl Default for FadePaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}
