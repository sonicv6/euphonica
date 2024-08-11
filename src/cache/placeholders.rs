use once_cell::sync::Lazy;
use gtk::gdk::Texture;

pub static ALBUMART_PLACEHOLDER: Lazy<Texture> = Lazy::new(|| {
    println!("Loading placeholder texture...");
    Texture::from_resource("/org/euphonia/Euphonia/albumart-placeholder.png")
});
