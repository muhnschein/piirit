//! Screen-ready pictures (avatar circle, tinted face, intro drawing, blurred backdrop), drawn
//! by QML as plain `Image` of `image://piirit/<kind>?<recipe>` ([`Recipe`]).
//!
//! Not `ShaderEffect`: Sailfish Qt 5.6 (fixed 5.15) hiding a persistent-scene-graph window
//! (cover, each return to front) runs `cleanupMaterialCache()`, freeing material types while
//! program cache keyed by them lives on; new `ShaderEffect` at freed address gets dead one's
//! program, uniforms by position: flat squares (issue #102).
//!
//! `Image`'s texture material is static, never freed. Shader math done once per picture,
//! kept in Qt's pixmap cache. Qt decodes, scales (C++ below); Rust does arithmetic on
//! straight ARGB32, pinned by tests.

// `cpp!` is FFI, so `unsafe`. Allowed here only: see docs/BUILDING.md.
#![allow(unsafe_code)]

use cpp::cpp;
use qmetaobject::QString;

cpp! {{
    #include <QtCore/QUrl>
    #include <QtGui/QImage>
    #include <QtGui/QImageReader>
    #include <QtQml/QQmlEngine>
    #include <QtQuick/QQuickImageProvider>

    // Provider reads a file path or file URL.
    static QString piirit_local(const QString &file) {
        if (file.startsWith(QLatin1String("file:"))) {
            return QUrl(file).toLocalFile();
        }
        return file;
    }

    // Size per recipe kind; `requested` from Image or not.
    static QSize piirit_want(int kind, const QSize &natural, const QSize &requested) {
        if (kind == 2) {
            // Blur loses nothing to small copy: blur small, draw large.
            return natural.scaled(QSize(64, 64), Qt::KeepAspectRatio);
        }
        return requested.isValid() && requested.width() > 0 && requested.height() > 0
               ? requested : natural.boundedTo(QSize(256, 256));
    }

    // Decode, scale, crop per recipe; pixels to Rust. Null: fallback.
    static QImage piirit_picture(const QString &id, const QSize &requested) {
        QString file;
        int kind = rust!(PiiritPictures_kind [id: &QString as "const QString &", file: &mut QString as "QString &"] -> i32 as "int" {
            match Recipe::parse(&id.to_string()) {
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
        // Decode at drawn size (12 MP photo = 48 MB for 200 px circle): covering draw in own
        // proportions, never above natural.
        QSize natural = reader.size();
        if (natural.isValid() && !natural.isEmpty()) {
            QSize decode = natural.scaled(piirit_want(kind, natural, requested),
                                          Qt::KeepAspectRatioByExpanding);
            if (!decode.isEmpty() && decode.width() < natural.width()
                    && decode.height() < natural.height()) {
                reader.setScaledSize(decode);
            }
        }
        QImage source = reader.read();
        if (source.isNull()) {
            return QImage();
        }
        if (!natural.isValid() || natural.isEmpty()) {
            natural = source.size();
        }
        QSize want = piirit_want(kind, natural, requested);
        QImage scaled;
        if (kind == 2) {
            scaled = source.scaled(want, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
        } else if (kind == 0) {
            // Middle of long side, whole short: as `PreserveAspectCrop`.
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
            let Some(recipe) = Recipe::parse(&id.to_string()) else {
                return false;
            };
            if bits.is_null() || width == 0 || height == 0 || stride < width {
                return false;
            }
            let Some(length) = stride.checked_mul(height) else {
                return false;
            };
            // SAFETY: `bits` is QImage's buffer, `stride` 32-bit pixels per line for `height`
            // lines, alive for this call.
            let pixels = unsafe { std::slice::from_raw_parts_mut(bits, length) };
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

/// Give `engine` the provider: `image://piirit/<recipe>`. Per engine: app's view and each
/// picture test add it; without, avatars show initial.
pub fn install(engine: &qmetaobject::QmlEngine) {
    let engine = engine.cpp_ptr();
    cpp!(unsafe [engine as "QQmlEngine *"] {
        if (engine) {
            engine->addImageProvider(QStringLiteral("piirit"), new PiiritPictures);
        }
    });
}

/// Draw `id` as provider would, `width` by `height`, straight ARGB32; `None` if no picture.
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
                    // SAFETY: `out` is `render`'s vector, alive for this block.
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

/// Picture a recipe makes; numbers are read by C++.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Avatar: middle, cut to a circle.
    Face = 0,
    /// Intro drawing: red ink, green accent.
    Ink = 1,
    /// Call backdrop.
    Blur = 2,
}

/// Colour as straight channels.
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
    /// `rrggbb` or `#rrggbb`; `aarrggbb` drops its alpha.
    fn parse(text: &str) -> Option<Self> {
        let hex = text.trim_start_matches('#');
        let hex = match hex.len() {
            6 => hex,
            8 => hex.get(2..)?,
            _ => return None,
        };
        let channel = |at: usize| {
            u8::from_str_radix(hex.get(at..at.checked_add(2)?)?, 16)
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

/// Draw spec: kind, then `encodeURIComponent`-encoded `key=value` pairs.
///
/// - `file`: path or `file:` URL.
/// - `grey=1`: colour removed (a face).
/// - `tint`, `strength`: tint over the face, part of the way.
/// - `from`, `to`: face fade, height fractions from top: whole above `from`, gone by `to`, eased
///   between.
/// - `ink`, `lit`, `inkStrength`, `litStrength`: drawing colours, strengths.
/// - `radius`: blur reach, in pixels of the 64-pixel copy.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Picture kind.
    pub kind: Kind,
    /// File it is made from.
    pub file: String,
    /// Drawn without colour.
    pub grey: bool,
    /// Laid over the face, and how much.
    pub tint: Option<(Colour, f64)>,
    /// Where the face fades out.
    pub fade: Option<(f64, f64)>,
    /// Ink and accent, each with strength.
    pub ink: Option<((Colour, f64), (Colour, f64))>,
    /// Blur reach, pixels.
    pub radius: usize,
}

impl Recipe {
    /// Read an id; `None` if nothing drawable.
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
            // 64-pixel copy: past half is flat colour anyway.
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

    /// Make the picture from `pixels`: straight ARGB32, `stride` per line.
    pub fn bake(&self, pixels: &mut [u32], width: usize, height: usize, stride: usize) {
        // Provider refuses these; repeated so `stride` never steps by 0.
        if width == 0 || height == 0 || stride < width {
            return;
        }
        match self.kind {
            Kind::Face => self.face(pixels, width, height, stride),
            Kind::Ink => self.colour_ink(pixels, width, height, stride),
            Kind::Blur => blur(pixels, width, height, stride, self.radius),
        }
    }

    /// Face: greyed or tinted, cut to a circle, rim softened, faded.
    fn face(&self, pixels: &mut [u32], width: usize, height: usize, stride: usize) {
        let diameter = to_f64(width.min(height));
        let (middle_x, middle_y) = (to_f64(width) / 2.0, to_f64(height) / 2.0);
        for (y, line) in pixels.chunks_mut(stride).take(height).enumerate() {
            let across = (to_f64(y) + 0.5) / to_f64(height);
            let kept = self.fade.map_or(1.0, |(from, to)| eased(across, from, to));
            for (x, pixel) in line.iter_mut().take(width).enumerate() {
                let [alpha, mut red, mut green, mut blue] = unpack(*pixel);
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
                *pixel = pack([alpha * rim * kept, red, green, blue]);
            }
        }
    }

    /// Two colours: red is ink share, green accent share.
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
        for line in pixels.chunks_mut(stride).take(height) {
            for pixel in line.iter_mut().take(width) {
                let [_, red, green, _] = unpack(*pixel);
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
                *pixel = pack([
                    alpha,
                    colour(ink.red, lit.red),
                    colour(ink.green, lit.green),
                    colour(ink.blue, lit.blue),
                ]);
            }
        }
    }
}

/// Share of face kept at `across` height: all above `from`, none past `to`, squared between
/// (straight ramp reads as wash).
fn eased(across: f64, from: f64, to: f64) -> f64 {
    let sink = ((to - across) / (to - from)).clamp(0.0, 1.0);
    sink * sink
}

/// Three box passes each way: close to Gaussian for a backdrop.
fn blur(pixels: &mut [u32], width: usize, height: usize, stride: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let mut line = Vec::new();
    for _ in 0..3 {
        for row in pixels.chunks_mut(stride).take(height) {
            line.clear();
            line.extend(row.iter().take(width));
            let averaged = box_pass(&line, radius);
            for (pixel, value) in row.iter_mut().zip(averaged) {
                *pixel = value;
            }
        }
        for x in 0..width {
            // The column: every `stride`th pixel from `x`.
            line.clear();
            line.extend(pixels.iter().skip(x).step_by(stride).take(height));
            let averaged = box_pass(&line, radius);
            let column = pixels.iter_mut().skip(x).step_by(stride).take(height);
            for (pixel, value) in column.zip(averaged) {
                *pixel = value;
            }
        }
    }
}

/// Line pixel: mean within `radius`; line ends stand in past them.
fn box_pass(line: &[u32], radius: usize) -> Vec<u32> {
    let (Some(&first), Some(&last)) = (line.first(), line.last()) else {
        return Vec::new();
    };
    let first = unpack(first);
    (0..line.len())
        .map(|at| {
            let mut sum = [0.0_f64; 4];
            let span = at.saturating_sub(radius)..=at.saturating_add(radius);
            let mut count = 0.0;
            for near in span {
                let channels = unpack(line.get(near).copied().unwrap_or(last));
                for (total, channel) in sum.iter_mut().zip(channels) {
                    *total += channel;
                }
                count += 1.0;
            }
            // Before start: first pixel, once per step window reaches past.
            let before = radius.saturating_sub(at);
            for (total, channel) in sum.iter_mut().zip(first) {
                *total += channel * to_f64(before);
            }
            count += to_f64(before);
            pack(sum.map(|total| total / count))
        })
        .collect()
}

/// ARGB32 pixel to channels, alpha first, 0 to 1.
fn unpack(pixel: u32) -> [f64; 4] {
    pixel.to_be_bytes().map(|byte| f64::from(byte) / 255.0)
}

/// Channels to pixel, rounding and clamping.
fn pack(channels: [f64; 4]) -> u32 {
    u32::from_be_bytes(channels.map(|channel| {
        let scaled = (channel.clamp(0.0, 1.0) * 255.0).round();
        // In 0..=255 by the clamp.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = scaled as u8;
        byte
    }))
}

/// Pixel count as coordinate; pictures far below 2^52 pixels.
#[allow(clippy::cast_precision_loss)]
fn to_f64(count: usize) -> f64 {
    count as f64
}

/// `encodeURIComponent` undone. `None` on broken escape or non-UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let mut rest = text.as_bytes();
    let mut out = Vec::with_capacity(rest.len());
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        if byte == b'%' {
            let [high, low, after @ ..] = rest else {
                return None;
            };
            let hex = [*high, *low];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
            rest = after;
        } else {
            out.push(byte);
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
        // Fade ending before its start: none.
        assert_eq!(Recipe::parse("face?file=a&from=1&to=1").unwrap().fade, None);

        let ink = Recipe::parse("ink?file=a&ink=ffffff&lit=%2300ff00&litStrength=0.5").unwrap();
        let ((ink_colour, ink_strength), (lit, lit_strength)) = ink.ink.unwrap();
        assert!((ink_colour.blue - 1.0).abs() < 1e-9 && (ink_strength - 1.0).abs() < 1e-9);
        assert!((lit.green - 1.0).abs() < 1e-9 && (lit_strength - 0.5).abs() < 1e-9);

        assert_eq!(Recipe::parse("blur?file=a&radius=99").unwrap().radius, 32);
        assert_eq!(Recipe::parse("blur?file=a").unwrap().radius, 8);
        // #aarrggbb: alpha dropped.
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
        // Greyed: 1/3 each; 3/4 toward tint: red, green 1/12; blue 5/6.
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
