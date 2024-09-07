use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};
use glib::{Properties, clone};

use crate::player::Player;

use super::SidebarButton;

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, Properties, Default, CompositeTemplate)]
    #[properties(wrapper_type = super::Sidebar)]
    #[template(resource = "/org/euphonia/Euphonia/gtk/sidebar.ui")]
    pub struct Sidebar {
        #[template_child]
        pub albums_btn: TemplateChild<SidebarButton>,
        #[template_child]
        pub artists_btn: TemplateChild<SidebarButton>,
        #[template_child]
        pub queue_btn: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub queue_len: TemplateChild<gtk::Label>,
        #[property(get, set)]
        pub showing_queue_view: Cell<bool>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "EuphoniaSidebar";
        type Type = super::Sidebar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Sidebar {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Sidebar {}

    // Trait shared by all boxes
    impl BoxImpl for Sidebar {}
}

glib::wrapper! {
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for Sidebar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup(&self, stack: gtk::Stack, player: Player) {
        // Set default view. TODO: remember last view
        stack.set_visible_child_name("albums");
        stack
            .bind_property(
                "visible-child-name",
                self,
                "showing-queue-view"
            )
            .transform_to(|_, name: String| {Some(name == "queue")})
            .sync_create()
            .build();

        self.imp().albums_btn.set_active(true);
        // Hook each button to their respective views
        self.imp().albums_btn.connect_toggled(clone!(
            #[weak]
            stack,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("albums");

            }
        }));

        self.imp().artists_btn.connect_toggled(clone!(
            #[weak]
            stack,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("artists");
            }
        }));

        self.imp().queue_btn.connect_toggled(clone!(
            #[weak]
            stack,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("queue");
            }
        }));

        player.queue()
              .bind_property(
                  "n-items",
                  &self.imp().queue_len.get(),
                  "label"
              )
              .transform_to(|_, size: u32| {Some(size.to_string())})
              .build();
    }
}
