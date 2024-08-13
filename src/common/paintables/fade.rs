use std::cell::{Cell, RefCell};
use gtk::{
    glib,
    gdk,
    gdk::subclass::paintable::*,
    graphene,
    prelude::*,
    subclass::prelude::*
};

// A Paintable for fading between textures. They are always scaled & cropped
// to both maintain their aspect ratio and to fill the widest dimension of
// this Paintable (equal to GTK_CONTENT_FIT_COVER in GtkPictures).
// To fade between textures of different aspect ratios, we stick to the new
// texture's coordinate system and figure out the scale at which to draw the
// old texture such that there is no visible shift when switching to the
// new texture but standing at fade == 0.0 (i.e. the old texture is still
// displayed at the same position and scale as before).
// The new texture is always drawn first at 100% opacity, optionally followed
// by the old texture (being faded out) to avoid the short dip in opacity half-
// way through the transition.
// Adopted with modifications from Nanling Zheng's implementation for Gapless.
// The original implementation was in Vala and can be found at
// https://gitlab.gnome.org/neithern/g4music/-/blob/master/src/ui/paintables.vala.
// Both the original implementation and this one are licensed under GPLv3.
mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FadePaintable {
        pub current: RefCell<Option<gdk::Paintable>>,
        pub previous: RefCell<Option<gdk::Paintable>>,
        // 0 = previous, 0.5 = halfway, 1.0 = current
        pub fade: Cell<f64>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FadePaintable {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphoniaFadePaintable";
        type Type = super::FadePaintable;
        type Interfaces = (gdk::Paintable,);

        fn new() -> Self {
            Self {
                current: RefCell::new(None),
                previous: RefCell::new(None),
                fade: Cell::new(0.0)
            }
        }
    }

    impl ObjectImpl for FadePaintable {}

    impl PaintableImpl for FadePaintable {
        fn current_image(&self) -> gdk::Paintable {
            if let Some(tex) = self.current.borrow().as_ref() {
                tex.current_image()
            } else {
                self.obj().clone().upcast::<gdk::Paintable>()
            }
        }

        fn intrinsic_width(&self) -> i32 {
            if let Some(current) = self.current.borrow().as_ref() {
                current.intrinsic_width()
            }
            else {1}
        }

        fn intrinsic_height(&self) -> i32 {
            if let Some(current) = self.current.borrow().as_ref() {
                current.intrinsic_height()
            }
            else {1}
        }

        fn intrinsic_aspect_ratio(&self) -> f64 {
            if let Some(current) = self.current.borrow().as_ref() {
                current.intrinsic_aspect_ratio()
            }
            else {1.0}
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let fade = self.fade.get();
            // Check if there's a current texture.
            if let Some(curr) = self.current.borrow().as_ref() {
                if fade < 1.0 && self.previous.borrow().as_ref().is_none() {
                    // If there is one, but nothing previously and fade is != 1,
                    // then we're fading in from nothing.
                    snapshot.push_opacity(fade);
                    curr.snapshot(snapshot, width, height);
                    snapshot.pop();
                }
                else {
                    // Draw at full opacity (skip the opacity node).
                    curr.snapshot(snapshot, width, height);
                }
            }
            if fade < 1.0 {
                let delta_x: f64;
                let delta_y: f64;
                if let Some(prev) = self.previous.borrow().as_ref() {
                    // Previous texture is still visible
                    let prev_ratio = prev.intrinsic_aspect_ratio();
                    let different_ratios = prev_ratio != self.intrinsic_aspect_ratio();
                    // Relative to current texture size
                    let prev_width: f64;
                    let prev_height: f64;
                    if different_ratios {
                        let curr_max_side = width.max(height);
                        if prev_ratio < 1.0 {
                            prev_height = curr_max_side;
                            prev_width = prev_height * prev_ratio;
                        }
                        else {
                            prev_width = curr_max_side;
                            prev_height = prev_width / prev_ratio;
                        }
                        // Move origin to the scaled prev texture's upper left corner
                        delta_x = (width - prev_width) / 2.0;
                        delta_y = (height - prev_height) / 2.0;
                        snapshot.translate(&graphene::Point::new(
                            delta_x as f32,
                            delta_y as f32
                        ));
                    }
                    else {
                        delta_x = 0.0;
                        delta_y = 0.0;
                    }
                    // Fade previous out
                    snapshot.push_opacity(1.0 - fade);
                    prev.snapshot(snapshot, width, height);
                    snapshot.pop();
                    snapshot.translate(&graphene::Point::new(
                            -delta_x as f32,
                            -delta_y as f32
                    ));
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct FadePaintable(ObjectSubclass<imp::FadePaintable>) @implements gdk::Paintable;
}

impl FadePaintable {
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
        if let Some(tex) = self.imp().current.take() {
            let _ = self.imp().previous.replace(Some(tex));
        }
        self.imp().current.replace(new);
        self.imp().fade.replace(0.0);
        self.invalidate_contents();
    }
}

impl Default for FadePaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}
