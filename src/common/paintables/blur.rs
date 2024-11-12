use std::cell::{Cell, RefCell};
use gtk::{
    glib,
    gdk,
    gdk::subclass::paintable::*,
    graphene,
    prelude::*,
    subclass::prelude::*
};

// Background paintable implementation.
// Euphonica can optionally use the currently-playing track's album art as its
// background. This is always scaled to fill the whole window and can be further
// blurred. When the next song has a different album art, a fade animation will
// be played.
// For performance reasons, we avoid performing blurring on every frame. Instead,
// we only blur when:
// 1. The album art has changed, or
// 2. Blur configuration has changed, or
// 2. The window is being resized.
// To make this easier to implement, we implement all the blurring and caching in
// this separate GdkPaintable.
mod imp {
    use glib::{closure_local, Properties};

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::BlurPaintable)]
    pub struct BlurPaintable {
        pub content: RefCell<Option<gdk::Paintable>>, // unblurred content
        pub cached: RefCell<Option<gdk::Paintable>>, // cached blurred content
        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set = Self::set_blur_radius)]
        pub blur_radius: Cell<u32>,

        pub needs_reblur: Cell<bool>,
        // Kept here to detect window size changes, which necessitate re-blurring
        pub curr_width: Cell<f64>,
        pub curr_height: Cell<f64>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BlurPaintable {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaBlurPaintable";
        type Type = super::BlurPaintable;
        type Interfaces = (gdk::Paintable,);

        fn new() -> Self {
            Self {
                content: RefCell::new(None),
                cached: RefCell::new(None),
                blur_radius: Cell::new(1),
                curr_width: Cell::new(16.0),
                curr_height: Cell::new(16.0),
                needs_reblur: Cell::new(true)
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for BlurPaintable {}

    impl PaintableImpl for BlurPaintable {
        fn current_image(&self) -> gdk::Paintable {
            if let Some(tex) = self.cached.borrow().as_ref() {
                tex.current_image()
            } else {
                self.obj().clone().upcast::<gdk::Paintable>()
            }
        }

        fn intrinsic_width(&self) -> i32 {
            if let Some(content) = self.content.borrow().as_ref() {
                content.intrinsic_width()
            }
            else {1}
        }

        fn intrinsic_height(&self) -> i32 {
            if let Some(content) = self.content.borrow().as_ref() {
                content.intrinsic_height()
            }
            else {1}
        }

        fn intrinsic_aspect_ratio(&self) -> f64 {
            if let Some(content) = self.content.borrow().as_ref() {
                content.intrinsic_aspect_ratio()
            }
            else {1.0}
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let old_width = self.curr_width.replace(width);
            let old_height = self.curr_height.replace(height);
            if (old_width != width) || (old_height != height) {
                self.needs_reblur.replace(true);
            }
            // Can also be set to true by set_blur_radius
            if self.needs_reblur.get() {
                // Regenerate blur first, then draw the blurred texture
                self.update_blur(width as f32, height as f32);
            }
            // Check if there is a texture (might have been called before being given a texture)
            if let Some(cached) = self.cached.borrow().as_ref() {
                cached.snapshot(snapshot, width, height);
            }
        }
    }

    impl BlurPaintable {
        pub fn set_blur_radius(&self, new_radius: u32) {
            let old_radius = self.blur_radius.replace(new_radius);
            if old_radius != new_radius {
                self.needs_reblur.replace(true);
            }
        }

        /// Scale the paintable to the current size, then blur them.
        /// Here we will scale to fill, centering the content paintable in the drawing area.
        pub fn update_blur(&self, width: f32, height: f32) {
            if let Some(paintable) = self.content.borrow().as_ref() {
                // Create a separate snapshot to cache the blur
                let snapshot = gtk::Snapshot::new();
                let blur_radius = self.blur_radius.get() as f32;
                let bg_width = paintable.intrinsic_width() as f32;
                let bg_height = paintable.intrinsic_height() as f32;
                // Scale a bit more to hide the semitransparent blurred edges
                let scale_x = width / (bg_width - 2.0 * blur_radius);
                let scale_y = height / (bg_height - 2.0 * blur_radius);
                let scale_max = scale_x.max(scale_y);  // Scale by this much to completely fill the requested area

                // Figure out where to position the upper left corner of the content paintable such that
                // when scaled (keeping the upper left corner static) it would fill the whole drawing area.
                let view_width = bg_width * scale_max;
                let view_height = bg_height * scale_max;
                let delta_x = (width - view_width) * 0.5;
                let delta_y = (height - view_height) * 0.5;
                if blur_radius > 0.0 {
                    snapshot.push_blur(blur_radius.into());
                }
                // To further optimise performance, clip areas that are outside the viewport (plus some margins
                // for the blur radius) before blurring.
                snapshot.push_clip(&graphene::Rect::new(
                    -blur_radius, -blur_radius, width + blur_radius * 2.0, height + blur_radius * 2.0
                ));
                snapshot.translate(&graphene::Point::new(
                    delta_x,
                    delta_y
                ));
                paintable.snapshot(&snapshot, view_width.into(), view_height.into());
                snapshot.translate(&graphene::Point::new(
                    -delta_x,
                    -delta_y
                ));
                // Clip
                snapshot.pop();
                // Blur
                if blur_radius > 0.0 {
                    snapshot.pop();
                }
                // Cache immutable texture
                if let Some(rendered) = snapshot.to_paintable(Some(&graphene::Size::new(width, height))) {
                    self.cached.replace(Some(rendered.current_image()));
                }
            }
            else {
                // Content image has been removed => remove blurred version too.
                let _ = self.cached.take();
            }
            self.needs_reblur.replace(false);
        }
    }
}

glib::wrapper! {
    pub struct BlurPaintable(ObjectSubclass<imp::BlurPaintable>) @implements gdk::Paintable;
}

impl BlurPaintable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_content(&self, new: Option<gdk::Paintable>) {
        self.imp().content.replace(new);
        self.imp().needs_reblur.replace(true);
    }

    pub fn has_content(&self) -> bool {
        self.imp().content.borrow().as_ref().is_some()
    }

    pub fn content(&self) -> Option<gdk::Paintable> {
        self.imp().content.borrow().clone()
    }
}

impl Default for BlurPaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}
