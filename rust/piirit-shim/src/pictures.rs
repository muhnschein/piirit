//! Pictures drawn the way the app shows them, made ready before they reach
//! the screen: an avatar cut to a circle, greyed, tinted and faded; an
//! introduction drawing in the ambience's colours; a call's backdrop
//! blurred. QML asks for them as `image://piirit/<kind>?<recipe>`
//! ([`Recipe`]) and draws what comes back with a plain `Image`.
//!
//! They were `ShaderEffects`, and on Sailfish's Qt 5.6 a `ShaderEffect` made
//! after its window has once been hidden can be drawn with another
//! `ShaderEffect`'s program. Hiding a window with a persistent scene graph
//! -- the cover, every time the app comes to the front -- runs
//! `QQuickShaderEffectMaterial::cleanupMaterialCache()`, which deletes
//! every `ShaderEffect` material type of that window while the materials
//! and the renderer's program cache, both keyed by those types, live on.
//! The next `ShaderEffect` made there gets a type from the allocator, and
//! when that is the address of a deleted one, the renderer hands it the
//! deleted type's program -- with its own uniforms set by position into
//! that program's slots. On the cover that was the grid's fade: every new
//! face a square of its whole picture, its colour kept, its width written
//! into `qt_Opacity` (white), dimming only at its foot. Before 2.1.0 the
//! same thing among the `OpacityMask`, `Desaturate` and `ColorOverlay` of each
//! face made the flat squares of issue #102. Qt fixed it in 5.15 by
//! clearing the cache only when the scene graph goes with it; Sailfish
//! ships 5.6.
//!
//! A plain `Image` is drawn with Qt's own texture material, whose type is
//! static and never deleted, so nothing here can be drawn with anything
//! else. The arithmetic the shaders did per pixel on the GPU is done here
//! once per picture, and Qt's pixmap cache keeps the result.
//!
//! Decoding and scaling are Qt's (`QImageReader`, `QImage::scaled`), in
//! the C++ provider below; the arithmetic is Rust, on straight
//! (non-premultiplied) ARGB32 pixels, and is what the tests pin.

// `cpp!` expands to a call across the FFI boundary, which is `unsafe` by
// construction. Scoped to this file: the workspace denies it everywhere
// else, and docs/BUILDING.md says why this one is allowed.
#![allow(unsafe_code)]

use cpp::cpp;
use qmetaobject::QString;

cpp! {{
    #include <QtCore/QUrl>
    #include <QtGui/QImage>
    #include <QtGui/QImageReader>
    #include <QtQml/QQmlEngine>
    #include <QtQuick/QQuickImageProvider>

    // What the provider reads: a file path, or a file URL (InkArt's
    // `source` is one).
    static QString piirit_local(const QString &file) {
        if (file.startsWith(QLatin1String("file:"))) {
            return QUrl(file).toLocalFile();
        }
        return file;
    }

    // Decode, scale and crop for one recipe, hand the pixels to Rust, and
    // return what it made. A null image is an Image in error, which every
    // caller already falls back from.
    static QImage piirit_picture(const QString &id, const QSize &requested) {
        QString file;
        int kind = rust!(PiiritPictures_kind [id: &QString as "const QString &", file: &mut QString as "QString &"] -> i32 as "int" {
            match crate::pictures::Recipe::parse(&id.to_string()) {
                Some(recipe) => {
                    *file = QString::from(recipe.file.as_str());
                    recipe.kind as i32
                }
                None => -1,
            }
        });
        if (kind < 0) {
            return QImage();
        }
        QImageReader reader(piirit_local(file));
        QImage source = reader.read();
        if (source.isNull()) {
            return QImage();
        }
        QSize natural = source.size();
        QSize want = requested.isValid() && requested.width() > 0 && requested.height() > 0
                     ? requested : natural.boundedTo(QSize(256, 256));
        QImage scaled;
        if (kind == 2) {
            // A blur keeps nothing a small copy does not have: blurred
            // small and drawn large is the same picture, for a fraction
            // of the work.
            want = natural.scaled(QSize(64, 64), Qt::KeepAspectRatio);
            scaled = source.scaled(want, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
        } else if (kind == 0) {
            // The middle of the long side, the whole of the short one:
            // what PreserveAspectCrop showed.
            QImage cover = source.scaled(want, Qt::KeepAspectRatioByExpanding,
                                         Qt::SmoothTransformation);
            scaled = cover.copy((cover.width() - want.width()) / 2,
                                (cover.height() - want.height()) / 2,
                                want.width(), want.height());
        } else {
            scaled = source.scaled(want, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
        }
        QImage out = scaled.convertToFormat(QImage::Format_ARGB32);
        int width = out.width();
        int height = out.height();
        int stride = out.bytesPerLine() / 4;
        uint *bits = reinterpret_cast<uint *>(out.bits());
        bool done = rust!(PiiritPictures_bake [id: &QString as "const QString &", bits: *mut u32 as "uint *", width: i32 as "int", height: i32 as "int", stride: i32 as "int"] -> bool as "bool" {
            let (Ok(width), Ok(height), Ok(stride)) =
                (usize::try_from(width), usize::try_from(height), usize::try_from(stride)) else {
                return false;
            };
            let Some(recipe) = crate::pictures::Recipe::parse(&id.to_string()) else {
                return false;
            };
            if bits.is_null() || width == 0 || height == 0 || stride < width {
                return false;
            }
            // SAFETY: `bits` is the QImage's own buffer, `stride` 32-bit
            // pixels a line for `height` lines, and the image outlives
            // this call.
            let pixels = unsafe { std::slice::from_raw_parts_mut(bits, stride * height) };
            recipe.bake(pixels, width, height, stride);
            true
        });
        return done ? out : QImage();
    }

    class PiiritPictures : public QQuickImageProvider {
    public:
        PiiritPictures() : QQuickImageProvider(QQuickImageProvider::Image) {}
        QImage requestImage(const QString &id, QSize *size, const QSize &requested) override {
            QImage image = piirit_picture(id, requested);
            if (size) {
                *size = image.size();
            }
            return image;
        }
    };

}}

/// Give `engine` the provider: `image://piirit/<recipe>` (its `Recipe` says what one holds).
///
/// An image provider belongs to one engine, so whoever makes an engine
/// that draws avatars adds it -- the app to its view's, and each test that
/// reads a picture to its own. Without it the pictures fail to load, and
/// an avatar shows its initial, as for any picture that does not load.
pub fn install(engine: &qmetaobject::QmlEngine) {
    let engine = engine.cpp_ptr();
    cpp!(unsafe [engine as "QQmlEngine *"] {
        if (engine) {
            engine->addImageProvider(QStringLiteral("piirit"), new PiiritPictures);
        }
    });
}

/// Draw `id` (what follows `image://piirit/`) the way the provider does, at
/// `width` by `height`: straight ARGB32, a line after another. `None` for a
/// recipe or file the provider would answer with no picture.
///
/// The provider's own path through Qt, for tests to read the pixels of.
#[must_use]
pub fn render(id: &str, width: i32, height: i32) -> Option<(Vec<u32>, usize, usize)> {
    let id = QString::from(id);
    let mut pixels: Vec<u32> = Vec::new();
    let mut size = (0_usize, 0_usize);
    let out = std::ptr::addr_of_mut!(pixels);
    let dims = std::ptr::addr_of_mut!(size);
    let found = cpp!(unsafe [id as "QString", width as "int", height as "int", out as "void *", dims as "void *"] -> bool as "bool" {
        QImage image = piirit_picture(id, QSize(width, height));
        if (image.isNull()) {
            return false;
        }
        QImage straight = image.convertToFormat(QImage::Format_ARGB32);
        for (int y = 0; y < straight.height(); ++y) {
            const uint *line = reinterpret_cast<const uint *>(straight.constScanLine(y));
            for (int x = 0; x < straight.width(); ++x) {
                uint pixel = line[x];
                rust!(PiiritPictures_push [out: *mut Vec<u32> as "void *", pixel: u32 as "uint"] {
                    // SAFETY: `out` is the vector `render` lent, alive
                    // for the whole of this block.
                    unsafe { (*out).push(pixel) };
                });
            }
        }
        int w = straight.width();
        int h = straight.height();
        rust!(PiiritPictures_size [dims: *mut (usize, usize) as "void *", w: i32 as "int", h: i32 as "int"] {
            // SAFETY: as above, for the size.
            unsafe { *dims = (usize::try_from(w).unwrap_or(0), usize::try_from(h).unwrap_or(0)) };
        });
        return true;
    });
    found.then_some((pixels, size.0, size.1))
}

/// Which picture a recipe makes; the numbers are what the C++ reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An avatar: the middle of the picture, cut to a circle.
    Face = 0,
    /// An introduction drawing: red is the ink, green the accent.
    Ink = 1,
    /// A call's backdrop.
    Blur = 2,
}

/// A colour as the straight channels a recipe carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    /// Red, 0 to 1.
    pub red: f64,
    /// Green, 0 to 1.
    pub green: f64,
    /// Blue, 0 to 1.
    pub blue: f64,
}

impl Colour {
    /// `rrggbb` or `#rrggbb`, which is how QML prints an opaque colour;
    /// `aarrggbb` loses its alpha, which a recipe carries apart.
    fn parse(text: &str) -> Option<Self> {
        let hex = text.trim_start_matches('#');
        let hex = match hex.len() {
            6 => hex,
            8 => hex.get(2..)?,
            _ => return None,
        };
        let channel = |at: usize| {
            u8::from_str_radix(hex.get(at..at + 2)?, 16)
                .ok()
                .map(|value| f64::from(value) / 255.0)
        };
        Some(Self {
            red: channel(0)?,
            green: channel(2)?,
            blue: channel(4)?,
        })
    }
}

/// What to draw and how, read off the provider's id: the kind, then a
/// query of `key=value` pairs, each value percent-encoded as
/// `encodeURIComponent` leaves it.
///
/// - `file`: the picture, a path or a `file:` URL. Every kind.
/// - `grey=1`: the colour taken out (a face).
/// - `tint`, `strength`: a colour laid over the greyed face, part of the
///   way (a face the cover lights).
/// - `from`, `to`: where a face fades out, as fractions of its height from
///   its top -- whole above `from`, gone by `to`, eased in between.
/// - `ink`, `lit`, `inkStrength`, `litStrength`: the drawing's colours
///   and how much of each.
/// - `radius`: how far a blur reaches, in pixels of the 64 it is made at.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Which picture.
    pub kind: Kind,
    /// The file it is made from.
    pub file: String,
    /// Drawn without its colour.
    pub grey: bool,
    /// Laid over the face, and how much of the way.
    pub tint: Option<(Colour, f64)>,
    /// Where the face fades out.
    pub fade: Option<(f64, f64)>,
    /// The drawing's ink and accent, each with its strength.
    pub ink: Option<((Colour, f64), (Colour, f64))>,
    /// How far a blur reaches.
    pub radius: usize,
}

impl Recipe {
    /// Read an id, `None` for one that says nothing drawable.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        let (kind, query) = id.split_once('?')?;
        let kind = match kind {
            "face" => Kind::Face,
            "ink" => Kind::Ink,
            "blur" => Kind::Blur,
            _ => return None,
        };
        let mut pairs = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            pairs.push((key, percent_decode(value)?));
        }
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.as_str())
        };
        let number = |name: &str| get(name).and_then(|value| value.parse::<f64>().ok());
        let file = get("file").filter(|file| !file.is_empty())?.to_string();
        let tint = get("tint")
            .and_then(Colour::parse)
            .map(|colour| (colour, number("strength").unwrap_or(1.0).clamp(0.0, 1.0)));
        let fade = match (number("from"), number("to")) {
            (Some(from), Some(to)) if to > from => Some((from, to)),
            _ => None,
        };
        let ink = match (
            get("ink").and_then(Colour::parse),
            get("lit").and_then(Colour::parse),
        ) {
            (Some(ink), Some(lit)) => Some((
                (ink, number("inkStrength").unwrap_or(1.0)),
                (lit, number("litStrength").unwrap_or(1.0)),
            )),
            _ => None,
        };
        let radius = number("radius").map_or(8, |radius| {
            // Whole pixels of a 64-pixel copy: anything past half of it
            // is the same flat colour.
            let clamped = radius.clamp(0.0, 32.0).round();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = clamped as usize;
            whole
        });
        Some(Self {
            kind,
            file,
            grey: get("grey") == Some("1"),
            tint,
            fade,
            ink,
            radius,
        })
    }

    /// Make the picture out of `pixels`, straight ARGB32 as Qt's
    /// `Format_ARGB32` lays them out, `stride` to a line.
    pub fn bake(&self, pixels: &mut [u32], width: usize, height: usize, stride: usize) {
        match self.kind {
            Kind::Face => self.face(pixels, width, height, stride),
            Kind::Ink => self.colour_ink(pixels, width, height, stride),
            Kind::Blur => blur(pixels, width, height, stride, self.radius),
        }
    }

    /// The face: greyed and tinted as asked, cut to the circle that fits,
    /// its rim softened over a pixel, and faded towards its foot.
    fn face(&self, pixels: &mut [u32], width: usize, height: usize, stride: usize) {
        let diameter = to_f64(width.min(height));
        let (middle_x, middle_y) = (to_f64(width) / 2.0, to_f64(height) / 2.0);
        for y in 0..height {
            let across = (to_f64(y) + 0.5) / to_f64(height);
            let kept = self.fade.map_or(1.0, |(from, to)| eased(across, from, to));
            for x in 0..width {
                let at = y * stride + x;
                let [alpha, mut red, mut green, mut blue] = unpack(pixels[at]);
                if self.grey || self.tint.is_some() {
                    let grey = (red + green + blue) / 3.0;
                    red = grey;
                    green = grey;
                    blue = grey;
                }
                if let Some((tint, strength)) = self.tint {
                    red += (tint.red - red) * strength;
                    green += (tint.green - green) * strength;
                    blue += (tint.blue - blue) * strength;
                }
                let dx = to_f64(x) + 0.5 - middle_x;
                let dy = to_f64(y) + 0.5 - middle_y;
                let rim = (diameter / 2.0 - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
                pixels[at] = pack([alpha * rim * kept, red, green, blue]);
            }
        }
    }

    /// The drawing in two colours: red is how much of the ink, green how
    /// much of the accent, and nothing is drawn where neither is.
    fn colour_ink(&self, pixels: &mut [u32], width: usize, height: usize, stride: usize) {
        let ((ink, ink_strength), (lit, lit_strength)) = self.ink.unwrap_or((
            (
                Colour {
                    red: 1.0,
                    green: 1.0,
                    blue: 1.0,
                },
                1.0,
            ),
            (
                Colour {
                    red: 1.0,
                    green: 1.0,
                    blue: 1.0,
                },
                1.0,
            ),
        ));
        for y in 0..height {
            for x in 0..width {
                let at = y * stride + x;
                let [_, red, green, _] = unpack(pixels[at]);
                let body = red * ink_strength;
                let accent = green * lit_strength;
                let alpha = (body + accent).clamp(0.0, 1.0);
                let colour = |ink: f64, lit: f64| {
                    if alpha > 0.0 {
                        ((ink * body + lit * accent) / alpha).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                };
                pixels[at] = pack([
                    alpha,
                    colour(ink.red, lit.red),
                    colour(ink.green, lit.green),
                    colour(ink.blue, lit.blue),
                ]);
            }
        }
    }
}

/// How much of a face is kept `across` its height: all of it above
/// `from`, none past `to`, and squared between, so most of the way down
/// it keeps its strength and gives up the rest near the end -- a straight
/// ramp reads as a wash laid over it.
fn eased(across: f64, from: f64, to: f64) -> f64 {
    let sink = ((to - across) / (to - from)).clamp(0.0, 1.0);
    sink * sink
}

/// A blur three box passes deep each way, which is close enough to a
/// Gaussian for a backdrop nobody reads.
fn blur(pixels: &mut [u32], width: usize, height: usize, stride: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let mut line = Vec::new();
    for _ in 0..3 {
        for y in 0..height {
            line.clear();
            line.extend((0..width).map(|x| pixels[y * stride + x]));
            let averaged = box_pass(&line, radius);
            for (x, pixel) in averaged.into_iter().enumerate() {
                pixels[y * stride + x] = pixel;
            }
        }
        for x in 0..width {
            line.clear();
            line.extend((0..height).map(|y| pixels[y * stride + x]));
            let averaged = box_pass(&line, radius);
            for (y, pixel) in averaged.into_iter().enumerate() {
                pixels[y * stride + x] = pixel;
            }
        }
    }
}

/// Each pixel of a line as the mean of those within `radius` of it, the
/// line's ends standing in for what is past them.
fn box_pass(line: &[u32], radius: usize) -> Vec<u32> {
    let last = line.len().saturating_sub(1);
    (0..line.len())
        .map(|at| {
            let mut sum = [0.0_f64; 4];
            let span = at.saturating_sub(radius)..=(at + radius);
            let mut count = 0.0;
            for near in span {
                let channels = unpack(line[near.min(last)]);
                for (total, channel) in sum.iter_mut().zip(channels) {
                    *total += channel;
                }
                count += 1.0;
            }
            // Pixels before the start are the first one, as many times as
            // the window reaches past it.
            let before = radius.saturating_sub(at);
            let first = unpack(line[0]);
            for (total, channel) in sum.iter_mut().zip(first) {
                *total += channel * to_f64(before);
            }
            count += to_f64(before);
            pack(sum.map(|total| total / count))
        })
        .collect()
}

/// The channels of a straight ARGB32 pixel, alpha first, 0 to 1.
fn unpack(pixel: u32) -> [f64; 4] {
    pixel.to_be_bytes().map(|byte| f64::from(byte) / 255.0)
}

/// The other way, rounding and clamping.
fn pack(channels: [f64; 4]) -> u32 {
    u32::from_be_bytes(channels.map(|channel| {
        let scaled = (channel.clamp(0.0, 1.0) * 255.0).round();
        // In 0..=255 by the clamp.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = scaled as u8;
        byte
    }))
}

/// A pixel count as a coordinate. Pictures are far below 2^52 pixels.
#[allow(clippy::cast_precision_loss)]
fn to_f64(count: usize) -> f64 {
    count as f64
}

/// `encodeURIComponent` undone. `None` for a broken escape or bytes that
/// are not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey_square(side: usize, level: u8) -> Vec<u32> {
        vec![u32::from_be_bytes([255, level, level, level]); side * side]
    }

    #[test]
    fn a_recipe_reads_its_kind_and_decodes_its_file() {
        let recipe = Recipe::parse("face?file=%2Fhome%2Fa%20b%2Fpic.jpg&grey=1").unwrap();
        assert_eq!(recipe.kind, Kind::Face);
        assert_eq!(recipe.file, "/home/a b/pic.jpg");
        assert!(recipe.grey);
        assert_eq!(recipe.tint, None);
        assert_eq!(recipe.fade, None);
    }

    #[test]
    fn a_recipe_without_a_file_or_kind_is_nothing() {
        assert_eq!(Recipe::parse("face?grey=1"), None);
        assert_eq!(Recipe::parse("face"), None);
        assert_eq!(Recipe::parse("mask?file=x"), None);
        assert_eq!(Recipe::parse("face?file=%zz"), None);
        assert_eq!(Recipe::parse("face?file=%2"), None);
    }

    #[test]
    fn a_recipe_reads_tint_fade_ink_and_radius() {
        let face =
            Recipe::parse("face?file=a&tint=%23ff8000&strength=0.75&from=0.2&to=1.4").unwrap();
        let (tint, strength) = face.tint.unwrap();
        assert!((tint.red - 1.0).abs() < 1e-9 && (tint.green - 128.0 / 255.0).abs() < 1e-9);
        assert!((strength - 0.75).abs() < 1e-9);
        assert_eq!(face.fade, Some((0.2, 1.4)));
        // A fade that ends before it starts is none.
        assert_eq!(Recipe::parse("face?file=a&from=1&to=1").unwrap().fade, None);

        let ink = Recipe::parse("ink?file=a&ink=ffffff&lit=%2300ff00&litStrength=0.5").unwrap();
        let ((ink_colour, ink_strength), (lit, lit_strength)) = ink.ink.unwrap();
        assert!((ink_colour.blue - 1.0).abs() < 1e-9 && (ink_strength - 1.0).abs() < 1e-9);
        assert!((lit.green - 1.0).abs() < 1e-9 && (lit_strength - 0.5).abs() < 1e-9);

        assert_eq!(Recipe::parse("blur?file=a&radius=99").unwrap().radius, 32);
        assert_eq!(Recipe::parse("blur?file=a").unwrap().radius, 8);
        // An eight-digit colour is QML's #aarrggbb; its alpha is dropped.
        let tinted = Recipe::parse("face?file=a&tint=80ff0000").unwrap();
        assert!((tinted.tint.unwrap().0.red - 1.0).abs() < 1e-9);
        assert_eq!(Recipe::parse("face?file=a&tint=red").unwrap().tint, None);
    }

    #[test]
    fn a_face_is_a_circle_with_its_corners_gone() {
        let side = 40;
        let mut pixels = grey_square(side, 200);
        Recipe::parse("face?file=a")
            .unwrap()
            .bake(&mut pixels, side, side, side);
        let alpha = |x: usize, y: usize| pixels[y * side + x].to_be_bytes()[0];
        assert_eq!(alpha(0, 0), 0, "a corner is drawn: the face is square");
        assert_eq!(alpha(side - 1, side - 1), 0);
        assert_eq!(alpha(side / 2, side / 2), 255, "the middle is not drawn");
        assert_eq!(
            alpha(side / 2, 1),
            255,
            "the circle is not as wide as the face"
        );
        // Its colour untouched.
        assert_eq!(pixels[(side / 2) * side + side / 2].to_be_bytes()[1], 200);
    }

    #[test]
    fn a_grey_face_loses_its_colour_and_a_lit_one_wears_the_tint() {
        let side = 8;
        let red = u32::from_be_bytes([255, 255, 0, 0]);
        let mut grey = vec![red; side * side];
        Recipe::parse("face?file=a&grey=1")
            .unwrap()
            .bake(&mut grey, side, side, side);
        let [_, r, g, b] = grey[4 * side + 4].to_be_bytes();
        assert_eq!(
            (r, g, b),
            (85, 85, 85),
            "the grey is the mean of the channels"
        );

        let mut lit = vec![red; side * side];
        Recipe::parse("face?file=a&tint=0000ff&strength=0.75")
            .unwrap()
            .bake(&mut lit, side, side, side);
        let [_, r, g, b] = lit[4 * side + 4].to_be_bytes();
        // Greyed first, then three quarters of the way to the tint: a
        // third, then a twelfth for red and green and five sixths for blue.
        assert_eq!((r, g), (21, 21));
        assert!((212..=213).contains(&b), "blue is {b}");
    }

    #[test]
    fn a_face_fades_out_towards_its_foot() {
        let side = 20;
        let mut pixels = grey_square(side, 100);
        Recipe::parse("face?file=a&from=0.5&to=1")
            .unwrap()
            .bake(&mut pixels, side, side, side);
        let alpha = |y: usize| pixels[y * side + side / 2].to_be_bytes()[0];
        assert_eq!(alpha(4), 255, "above the band the face is whole");
        assert!(
            alpha(12) < 255 && alpha(12) > alpha(16),
            "the band does not fade"
        );
        assert!(alpha(19) < 5, "the foot is still there");
    }

    #[test]
    fn ink_is_drawn_in_the_colours_given() {
        let mut pixels = vec![
            u32::from_be_bytes([255, 255, 0, 0]),
            u32::from_be_bytes([255, 0, 255, 0]),
            u32::from_be_bytes([255, 0, 0, 0]),
        ];
        Recipe::parse("ink?file=a&ink=ff0000&lit=0000ff&inkStrength=0.5")
            .unwrap()
            .bake(&mut pixels, 3, 1, 3);
        assert_eq!(pixels[0].to_be_bytes(), [128, 255, 0, 0]);
        assert_eq!(pixels[1].to_be_bytes(), [255, 0, 0, 255]);
        assert_eq!(pixels[2].to_be_bytes()[0], 0, "blank paper is drawn");
        // With no colours given the drawing is white.
        let mut plain = vec![u32::from_be_bytes([255, 255, 0, 0])];
        Recipe::parse("ink?file=a")
            .unwrap()
            .bake(&mut plain, 1, 1, 1);
        assert_eq!(plain[0].to_be_bytes(), [255, 255, 255, 255]);
    }

    #[test]
    fn a_blur_spreads_a_point_and_keeps_a_flat_field_flat() {
        let side = 9;
        let mut pixels = grey_square(side, 0);
        pixels[4 * side + 4] = u32::from_be_bytes([255, 255, 255, 255]);
        Recipe::parse("blur?file=a&radius=2")
            .unwrap()
            .bake(&mut pixels, side, side, side);
        let level = |x: usize, y: usize| pixels[y * side + x].to_be_bytes()[1];
        assert!(level(4, 4) < 255, "the point is still sharp");
        assert!(level(3, 4) > 0 && level(4, 6) > 0, "nothing spread");

        let mut flat = grey_square(side, 77);
        Recipe::parse("blur?file=a&radius=3")
            .unwrap()
            .bake(&mut flat, side, side, side);
        assert!(flat
            .iter()
            .all(|&pixel| pixel.to_be_bytes() == [255, 77, 77, 77]));

        let mut untouched = grey_square(side, 10);
        untouched[0] = u32::from_be_bytes([255, 200, 200, 200]);
        let before = untouched.clone();
        Recipe::parse("blur?file=a&radius=0")
            .unwrap()
            .bake(&mut untouched, side, side, side);
        assert_eq!(untouched, before);
    }
}
