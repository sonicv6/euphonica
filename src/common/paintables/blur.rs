use std::cell::{Cell, RefCell};
use gtk::{
    glib,
    gdk,
    gdk::subclass::paintable::*,
    prelude::*,
    subclass::prelude::*
};
use image::DynamicImage;

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
    use glib::Properties;
    use image::{imageops::FilterType, DynamicImage};
    use libblur::{stack_blur, FastBlurChannels, ThreadingPolicy};

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::BlurPaintable)]
    pub struct BlurPaintable {
        pub content: RefCell<Option<DynamicImage>>, // unblurred content
        pub cached: RefCell<Option<gdk::MemoryTexture>>, // cached blurred content
        // Kept here so snapshot() does not have to query GSettings on every frame
        #[property(get, set = Self::set_blur_radius)]
        pub blur_radius: Cell<u32>,

        #[property(get)]
        pub needs_reblur: Cell<bool>,
        // Kept here to detect window size changes, which necessitate re-blurring
        #[property(get)]
        pub curr_width: Cell<f64>,
        #[property(get)]
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
            if let Some(cached) = self.cached.borrow().as_ref() {
                cached.intrinsic_width()
            }
            else {1}
        }

        fn intrinsic_height(&self) -> i32 {
            if let Some(cached) = self.cached.borrow().as_ref() {
                cached.intrinsic_height()
            }
            else {1}
        }

        fn intrinsic_aspect_ratio(&self) -> f64 {
            if let Some(cached) = self.cached.borrow().as_ref() {
                cached.intrinsic_aspect_ratio()
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
                self.update_blur(width.ceil() as u32, height.ceil() as u32);
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

        /// Scale the image to the current size, then blur them.
        /// Here we will scale to fill, centering the content paintable in the drawing area.
        pub fn update_blur(&self, width: u32, height: u32) {
            if let Some(di) = self.content.borrow().as_ref() {
                let scaled = di.resize_to_fill(
                    width,
                    height,
                    FilterType::Nearest
                );
                let mut dst_bytes: Vec<u8> = scaled.as_bytes().to_vec();
                // Always assume RGB8 (no alpha channel)
                // This works since we're the ones who wrote the original images
                // to disk in the first place.
                stack_blur(
                    &mut dst_bytes,
                    width * 3,
                    width,
                    height,
                    self.blur_radius.get(),
                    FastBlurChannels::Channels3,
                    ThreadingPolicy::Adaptive
                );
                // Wrap in MemoryTexture for snapshotting
                let mem_tex = gdk::MemoryTexture::new(
                    width as i32,
                    height as i32,
                    gdk::MemoryFormat::R8g8b8,
                    &glib::Bytes::from_owned(dst_bytes),
                    (width * 3) as usize
                );
                self.cached.replace(Some(mem_tex));
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

    /// Set new content to be blurred.
    /// This will immediately force a reblur and texture upload to GPU, so be sure to
    /// finish running this before starting the animation.
    pub fn set_content(&self, new: Option<DynamicImage>) {
        self.imp().content.replace(new);
        self.invalidate_contents();
        self.imp().update_blur(
            self.imp().curr_width.get().round() as u32,
            self.imp().curr_height.get().round() as u32
        );
    }

    /// Take content and cached blur from another paintable, if blur config & size are similar.
    /// This helps when migrating content between current and previous paintables (avoids one blur & upload).
    pub fn take_from(&self, other: &Self) {
        let cache_updated = !other.needs_reblur();
        self.imp().content.replace(other.take_content());
        if self.curr_width() == other.curr_width() && self.curr_height() == other.curr_height()  && self.blur_radius() == other.blur_radius() && cache_updated {
            self.imp().cached.replace(other.get_cached());
            self.imp().needs_reblur.replace(false);
        }
        self.invalidate_contents();
    }

    pub fn has_content(&self) -> bool {
        self.imp().content.borrow().as_ref().is_some()
    }

    pub fn take_content(&self) -> Option<DynamicImage> {
        self.imp().needs_reblur.replace(true);
        self.imp().content.take()
    }

    pub fn get_cached(&self) -> Option<gdk::MemoryTexture> {
        self.imp().cached.borrow().as_ref().cloned()
    }
}

impl Default for BlurPaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}
