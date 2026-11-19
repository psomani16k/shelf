use shelf_proto::config::{ReaderThemeSpec, ThemeLayer, theme_layer};

/// A theme that ships with Shelf. A user's theme with the same id replaces it.
pub struct BuiltInTheme {
    pub name: &'static str,
    /// Applied to the page in order, first to last.
    pub layers: &'static [Layer],
}

#[derive(Clone, Copy)]
pub struct Layer {
    pub kind: Kind,
    /// Left off images while the user keeps their colours.
    pub skip_images: bool,
}

/// `ThemeLayer`'s kinds; see `proto/configpb/theme.proto`.
#[derive(Clone, Copy)]
pub enum Kind {
    Invert,
    HueRotate { degrees: f64 },
    Saturate { amount: f64 },
    BrightnessContrast { brightness: f64, contrast: f64 },
    Levels { black: &'static str, white: &'static str },
    Multiply { color: &'static str },
    Matrix([f64; 12]),
}

/// Applied to images too.
const fn all(kind: Kind) -> Layer {
    Layer { kind, skip_images: false }
}

/// Left off images under *Keep colours*.
const fn text(kind: Kind) -> Layer {
    Layer { kind, skip_images: true }
}

/// Dark text on light becomes light on dark, with hues kept: the two cancel out on images.
const FLIP: [Layer; 2] = [text(Kind::Invert), text(Kind::HueRotate { degrees: 180.0 })];

/// Turns every colour into its grey.
const GREY: Kind = Kind::Saturate { amount: 0.0 };

const fn theme(name: &'static str, layers: &'static [Layer]) -> BuiltInTheme {
    BuiltInTheme { name, layers }
}

/// The book's white becomes `page` and its black `text`.
const fn light(page: &'static str, text: &'static str) -> Layer {
    all(Kind::Levels { black: text, white: page })
}

/// The same after a `FLIP`, which has made the page black and the text white.
const fn dark(page: &'static str, text: &'static str) -> Layer {
    all(Kind::Levels { black: page, white: text })
}

/// In the order the reader lists them: light, then dark.
pub static BUILT_IN: &[BuiltInTheme] = &[
    theme("Light", &[]),
    theme("Sepia", &[light("#f4ecd8", "#5b4636")]),
    theme("Solarized Light", &[light("#fdf6e3", "#586e75")]),
    theme("Warm", &[all(Kind::Multiply { color: "#ffe4c2" })]),
    theme("Dark", &[FLIP[0], FLIP[1], dark("#202020", "#d4d4d4")]),
    theme("Slate", &[FLIP[0], FLIP[1], dark("#1b2129", "#c5ccd6")]),
    theme("Nord", &[FLIP[0], FLIP[1], dark("#2e3440", "#d8dee9")]),
    theme("Solarized Dark", &[FLIP[0], FLIP[1], dark("#002b36", "#93a1a1")]),
    theme("AMOLED", &[FLIP[0], FLIP[1], dark("#000000", "#cccccc")]),
    theme("Night", &[FLIP[0], FLIP[1], dark("#14110d", "#c8b08e")]),
    theme("Red Night", &[FLIP[0], FLIP[1], text(GREY), dark("#000000", "#b30000")]),
    theme("Amber", &[FLIP[0], FLIP[1], text(GREY), dark("#0c0800", "#ffb000")]),
];

impl BuiltInTheme {
    pub fn to_spec(&self) -> ReaderThemeSpec {
        ReaderThemeSpec {
            name: self.name.to_owned(),
            layers: self.layers.iter().map(Layer::to_proto).collect(),
        }
    }
}

impl Layer {
    fn to_proto(&self) -> ThemeLayer {
        use theme_layer::Kind as K;
        let kind = match self.kind {
            Kind::Invert => K::Invert(theme_layer::Invert {}),
            Kind::HueRotate { degrees } => K::HueRotate(theme_layer::HueRotate { degrees }),
            Kind::Saturate { amount } => K::Saturate(theme_layer::Saturate { amount: Some(amount) }),
            Kind::BrightnessContrast { brightness, contrast } => {
                K::BrightnessContrast(theme_layer::BrightnessContrast {
                    brightness: Some(brightness),
                    contrast: Some(contrast),
                })
            }
            Kind::Levels { black, white } => K::Levels(theme_layer::Levels {
                black: black.to_owned(),
                white: white.to_owned(),
            }),
            Kind::Multiply { color } => K::Multiply(theme_layer::Multiply {
                color: color.to_owned(),
            }),
            Kind::Matrix(values) => K::Matrix(theme_layer::ColorMatrix {
                values: values.to_vec(),
            }),
        };
        ThemeLayer {
            skip_images: self.skip_images,
            kind: Some(kind),
        }
    }
}
