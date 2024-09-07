use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};
use glib::clone;

use crate::player::Player;

use super::SidebarButton;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
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

    impl ObjectImpl for Sidebar {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
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

    pub fn setup(&self, stack: gtk::Stack, player_bar_revealer: gtk::Revealer, player: Player) {
        // Set default view. TODO: remember last view
        stack.set_visible_child_name("albums");
        self.imp().albums_btn.set_active(true);
        // Hook each button to their respective views
        self.imp().albums_btn.connect_toggled(clone!(
            #[weak]
            stack,
            #[weak]
            player_bar_revealer,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("albums");
                player_bar_revealer.set_reveal_child(true);
            }
        }));

        self.imp().artists_btn.connect_toggled(clone!(
            #[weak]
            stack,
            #[weak]
            player_bar_revealer,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("artists");
                player_bar_revealer.set_reveal_child(true);
            }
        }));

        self.imp().queue_btn.connect_toggled(clone!(
            #[weak]
            stack,
            #[weak]
            player_bar_revealer,
            move |btn| {
            if btn.is_active() {
                stack.set_visible_child_name("queue");
                player_bar_revealer.set_reveal_child(false);
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
