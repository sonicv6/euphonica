use gtk::glib;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDisplayMode {
    Ellipsis = 0,
    Wrap = 1,
    Marquee = 2,
}

impl Default for TextDisplayMode {
    fn default() -> Self {
        Self::Ellipsis
    }
}

impl From<u32> for TextDisplayMode {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Ellipsis,
            1 => Self::Wrap,
            2 => Self::Marquee,
            _ => Self::Ellipsis,
        }
    }
}

impl From<TextDisplayMode> for u32 {
    fn from(mode: TextDisplayMode) -> Self {
        mode as u32
    }
}

impl glib::variant::StaticVariantType for TextDisplayMode {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        u32::static_variant_type()
    }
}

impl glib::variant::FromVariant for TextDisplayMode {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        u32::from_variant(variant).map(Self::from)
    }
}

impl glib::variant::ToVariant for TextDisplayMode {
    fn to_variant(&self) -> glib::Variant {
        u32::from(*self).to_variant()
    }
}