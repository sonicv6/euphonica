use adw::subclass::prelude::*;
use gtk::{gio, glib, prelude::*, CompositeTemplate};
use glib::clone;
use super::AlbumView;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/slamprust/Slamprust/gtk/library-view.ui")]
    pub struct LibraryView {
        #[template_child]
        pub prev_mode: TemplateChild<gtk::Button>,
        #[template_child]
        pub mode_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub next_mode: TemplateChild<gtk::Button>,
        #[template_child]
        pub mode_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub album_view: TemplateChild<AlbumView>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LibraryView {
        const NAME: &'static str = "SlamprustLibraryView";
        type Type = super::LibraryView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_layout_manager_type::<gtk::BinLayout>();
            // klass.set_css_name("library-view");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LibraryView {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for LibraryView {}
}

glib::wrapper! {
    pub struct LibraryView(ObjectSubclass<imp::LibraryView>)
        @extends gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for LibraryView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl LibraryView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_mode_name(&self, child: gtk::Widget) {
        // Get page of child
        let page = self.imp().mode_stack.page(&child);
        self.imp().mode_name.set_label(page.title().unwrap().as_str());
    }

    pub fn setup(&self) {
        // Order: albums - artists - folders (cyclical)
        self.imp().prev_mode.connect_clicked(clone!(@weak self as this => move |_| {
            let stack = this.imp().mode_stack.get();
            match this.imp().mode_stack.visible_child_name().unwrap().as_str() {
                "albums" => stack.set_visible_child_name("folders"),
                "artists" => stack.set_visible_child_name("albums"),
                "folders" => stack.set_visible_child_name("artists"),
                _ => unimplemented!()
            }
        }));
        self.imp().next_mode.connect_clicked(clone!(@weak self as this => move |_| {
            let stack = this.imp().mode_stack.get();
            match this.imp().mode_stack.visible_child_name().unwrap().as_str() {
                "albums" => stack.set_visible_child_name("artists"),
                "artists" => stack.set_visible_child_name("folders"),
                "folders" => stack.set_visible_child_name("albums"),
                _ => unimplemented!()
            }
        }));

        self.update_mode_name(self.imp().mode_stack.visible_child().unwrap());
        self.imp().mode_stack.connect_visible_child_notify(clone!(@weak self as this => move |stack| {
            this.update_mode_name(stack.visible_child().unwrap());
        }));
    }
}
