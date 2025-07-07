use glib::{clone, closure_local, signal::SignalHandlerId, Object};
use gtk::{glib, gio, prelude::*, subclass::prelude::*, CompositeTemplate, Image, Label};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

use crate::{
    cache::{placeholders::ALBUMART_PLACEHOLDER, Cache, CacheState},
    common::{Album, AlbumInfo, Marquee, TextDisplayMode},
    utils,
};

mod imp {
    use std::cell::Cell;

    use crate::common::Rating;

    use super::*;
    use glib::{ParamSpec, ParamSpecChar, ParamSpecString};
    use once_cell::sync::Lazy;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/io/github/htkhiem/Euphonica/gtk/library/album-cell.ui")]
    pub struct AlbumCell {
        #[template_child]
        pub cover: TemplateChild<gtk::Picture>, // Use high-resolution version
        #[template_child]
        pub title_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub title_ellipsis: TemplateChild<Label>,
        #[template_child]
        pub title_wrap: TemplateChild<Label>,
        #[template_child]
        pub title_marquee: TemplateChild<Marquee>,
        #[template_child]
        pub artist: TemplateChild<Label>,
        #[template_child]
        pub quality_grade: TemplateChild<Image>,
        #[template_child]
        pub rating: TemplateChild<Rating>,
        pub rating_val: Cell<i8>,
        pub album: RefCell<Option<Album>>,
        // Vector holding the bindings to properties of the Album GObject
        pub cover_signal_ids: RefCell<Option<(SignalHandlerId, SignalHandlerId)>>,
        pub cache: OnceCell<Rc<Cache>>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for AlbumCell {
        // `NAME` needs to match `class` attribute of template
        const NAME: &'static str = "EuphonicaAlbumCell";
        type Type = super::AlbumCell;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for AlbumCell {
        fn constructed(&self) {
            self.parent_constructed();

            self.obj()
                .bind_property("rating", &self.rating.get(), "value")
                .sync_create()
                .build();

            self.obj()
                .bind_property("rating", &self.rating.get(), "visible")
                .transform_to(|_, r: i8| {Some(r >= 0)})
                .sync_create()
                .build();

            // Connect to settings to watch for text display mode changes
            let settings = utils::settings_manager().child("ui");
            let obj_weak = self.obj().downgrade();
            settings.connect_changed(Some("album-cell-text-display"), move |settings, _| {
                if let Some(obj) = obj_weak.upgrade() {
                    obj.update_text_display_mode(settings);
                }
            });
            
            // Set initial text display mode
            self.obj().update_text_display_mode(&settings);
        }

        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("title").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecString::builder("quality-grade").build(),
                    ParamSpecChar::builder("rating").build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            match pspec.name() {
                "title" => {
                    // Return the text from whichever title widget is currently active
                    let stack = &self.title_stack;
                    match stack.visible_child_name().as_deref() {
                        Some("ellipsis") => self.title_ellipsis.label().to_value(),
                        Some("wrap") => self.title_wrap.label().to_value(),
                        Some("marquee") => self.title_marquee.label().label().to_value(),
                        _ => self.title_ellipsis.label().to_value(),
                    }
                }
                "artist" => self.artist.label().to_value(),
                "quality-grade" => self.quality_grade.icon_name().to_value(),
                "rating" => self.rating_val.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "title" => {
                    if let Ok(title) = value.get::<&str>() {
                        // Set the title on all label widgets
                        self.title_ellipsis.set_label(title);
                        self.title_wrap.set_label(title);
                        self.title_marquee.label().set_label(title);
                        obj.notify("title");
                    }
                }
                "artist" => {
                    if let Ok(artist) = value.get::<&str>() {
                        self.artist.set_label(artist);
                        obj.notify("artist");
                    }
                }
                "quality-grade" => {
                    if let Ok(icon_name) = value.get::<&str>() {
                        self.quality_grade.set_icon_name(Some(icon_name));
                        self.quality_grade.set_visible(true);
                    } else {
                        self.quality_grade.set_icon_name(None);
                        self.quality_grade.set_visible(false);
                    }
                }
                "rating" => {
                    if let Ok(new) = value.get::<i8>() {
                        let old = self.rating_val.replace(new);
                        if old != new {
                            obj.notify("rating");
                        }
                    }
                },
                _ => unimplemented!(),
            }
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for AlbumCell {}

    // Trait shared by all boxes
    impl BoxImpl for AlbumCell {}
}

glib::wrapper! {
    pub struct AlbumCell(ObjectSubclass<imp::AlbumCell>)
        @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl AlbumCell {
    pub fn new(item: &gtk::ListItem, cache: Rc<Cache>) -> Self {
        let res: Self = Object::builder().build();
        res.imp()
           .cache
           .set(cache)
           .expect("AlbumCell cannot bind to cache");
        res.setup(item);
        
        // Add hover controller for marquee
        res.setup_marquee_hover();
        
        let cache_state = res.imp()
               .cache
               .get()
               .unwrap()
               .get_cache_state();
        let _ = res.imp().cover_signal_ids.replace(Some((
            cache_state
               .connect_closure(
                   "album-art-downloaded",
                   false,
                   closure_local!(
                       #[weak(rename_to = this)]
                       res,
                       move |_: CacheState, folder_uri: String| {
                           if let Some(album) = this.imp().album.borrow().as_ref() {
                               if album.get_uri() == &folder_uri {
                                   this.update_album_art(album.get_info());
                               }
                           }
                       }
                   ),
               ),
            cache_state
               .connect_closure(
                   "album-art-cleared",
                   false,
                   closure_local!(
                       #[weak(rename_to = this)]
                       res,
                       move |_: CacheState, folder_uri: String| {
                           if let Some(album) = this.imp().album.borrow().as_ref() {
                               if album.get_uri() == &folder_uri {
                                   this.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
                               }
                           }
                       }
                   ),
               ),
        )));
        res
    }

    #[inline(always)]
    pub fn setup(&self, item: &gtk::ListItem) {
        item.property_expression("item")
            .chain_property::<Album>("title")
            .bind(self, "title", gtk::Widget::NONE);

        item.property_expression("item")
            .chain_property::<Album>("artist")
            .bind(self, "artist", gtk::Widget::NONE);

        item.property_expression("item")
            .chain_property::<Album>("quality-grade")
            .bind(self, "quality-grade", gtk::Widget::NONE);

        item.property_expression("item")
            .chain_property::<Album>("rating")
            .bind(self, "rating", gtk::Widget::NONE);
    }

    fn update_album_art(&self, info: &AlbumInfo) {
        if let Some(tex) = self
            .imp()
            .cache
            .get()
            .unwrap()
            .load_cached_album_art(info, true, true)
        {
            self.imp().cover.set_paintable(Some(&tex));
        } else {
            self.imp().cover.set_paintable(Some(&*ALBUMART_PLACEHOLDER));
        }
    }

    pub fn bind(&self, album: &Album) {
        // The string properties are bound using property expressions in setup().
        // Here we only need to manually bind to the cache controller to fetch album art.
        // Set once first (like sync_create)
        self.update_album_art(album.get_info());
        let _ = self.imp().album.replace(Some(album.clone()));
    }

    pub fn unbind(&self) {
        self.imp().album.replace(None).unwrap();
    }

    pub fn teardown(&self) {
        if let Some((update_id, clear_id)) = self.imp().cover_signal_ids.take() {
            let cache_state = self.imp()
                .cache
                .get()
                .unwrap()
                .get_cache_state();

            cache_state.disconnect(update_id);
            cache_state.disconnect(clear_id);
        }
    }

    pub fn update_text_display_mode(&self, settings: &gio::Settings) {
        let mode_str = settings.string("album-cell-text-display");
        let mode = match mode_str.as_str() {
            "ellipsis" => TextDisplayMode::Ellipsis,
            "wrap" => TextDisplayMode::Wrap,
            "marquee" => TextDisplayMode::Marquee,
            _ => TextDisplayMode::Ellipsis,
        };

        let stack = &self.imp().title_stack;
        match mode {
            TextDisplayMode::Ellipsis => {
                stack.set_visible_child_name("ellipsis");
            }
            TextDisplayMode::Wrap => {
                stack.set_visible_child_name("wrap");
            }
            TextDisplayMode::Marquee => {
                stack.set_visible_child_name("marquee");
            }
        }
    }

    fn setup_marquee_hover(&self) {
        // Add hover controller for marquee scrolling
        let hover_ctl = gtk::EventControllerMotion::new();
        hover_ctl.set_propagation_phase(gtk::PropagationPhase::Capture);
        hover_ctl.connect_enter(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _, _| {
                let stack = &this.imp().title_stack;
                if stack.visible_child_name().as_deref() == Some("marquee") {
                    this.imp().title_marquee.set_should_run_and_check(true);
                }
            }
        ));
        hover_ctl.connect_leave(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.imp().title_marquee.set_should_run_and_check(false);
            }
        ));
        self.add_controller(hover_ctl);
    }
}
