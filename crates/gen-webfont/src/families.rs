#[derive(Debug, Clone, Copy)]
pub struct WebFontFamily {
    pub key: &'static str,
    pub css_family: &'static str,
    pub dist_folder: &'static str,
    pub file_prefix: &'static str,
}

pub const WEBFONT_FAMILIES: &[WebFontFamily] = &[
    WebFontFamily {
        key: "normal",
        css_family: "Gen Interface JP",
        dist_folder: "Gen Interface JP",
        file_prefix: "GenInterfaceJP",
    },
    WebFontFamily {
        key: "display",
        css_family: "Gen Interface JP Display",
        dist_folder: "Gen Interface JP Display",
        file_prefix: "GenInterfaceJPDisplay",
    },
];

pub const WEIGHTS: &[(u16, &str)] = &[
    (100, "Thin"),
    (200, "ExtraLight"),
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "SemiBold"),
    (700, "Bold"),
    (800, "ExtraBold"),
];
