use std::cell::Cell;
use gtk::{
    gdk::{self, subclass::paintable::*}, glib::{self, clone}, prelude::*, subclass::prelude::*
};
use image::DynamicImage;

use crate::common::paintables::BlurPaintable;

// A specialised fade paintable meant for the blurred background effect.
// Whenever it is resized, it does not immediately redraw the child paintables at that
// new size. Instead, it starts a countdown timer, upon whose end the paintables will
// receive the new size. This timer is also restarted on every size change, meaning
// continuous changes such as click-and-drag window resizes will prevent it from finishing.
//
// To look less annoying, this paintable also fades between blur resolutions as follows:
// 1. Clone current into previous and keep rendering it at the old size.
// 2. Set progress to 0, showing previous, which is "current but at old size".
// 3. Resize current.
// 4. Fade back to current (the one blurred to the new size).
mod imp {
    use std::cell::RefCell;

    use glib::{clone, Properties, SourceId};

    use crate::common::paintables::BlurPaintable;

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::BlurredBackgroundPaintable)]
    pub struct BlurredBackgroundPaintable {
        pub current: BlurPaintable,
        pub previous: BlurPaintable,
        // 0 = previous, 0.5 = halfway, 1.0 = current
        pub fade: Cell<f64>,
        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set)]
        pub transition_duration: Cell<f64>,

        pub fade_anim: RefCell<Option<adw::TimedAnimation>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BlurredBackgroundPaintable {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaBlurredBackgroundPaintable";
        type Type = super::BlurredBackgroundPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for BlurredBackgroundPaintable {}

    impl PaintableImpl for BlurredBackgroundPaintable {
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
            let fade = self.fade.get();
            if self.current.has_content() {
                if !self.previous.has_content() && fade < 1.0 {
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
    pub struct BlurredBackgroundPaintable(ObjectSubclass<imp::BlurredBackgroundPaintable>) @implements gdk::Paintable;
}

impl BlurredBackgroundPaintable {
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

    pub fn set_new_content(&self, new: Option<DynamicImage>) {
        if new.is_none() {
            println!("Clearing...");
        }
        self.imp().previous.take_from(&self.imp().current);
        self.set_fade(0.0);
        self.imp().current.set_content(new);
    }
}

impl Default for BlurredBackgroundPaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}
