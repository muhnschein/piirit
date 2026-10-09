//! What the app's image provider makes of a picture (`src/pictures.rs`),
//! through the same C++ that hands it to QML: read off the file, cut and
//! sized, and baked.
//!
//! Everything a shader used to do to a picture is done here now -- an
//! avatar's circle, grey and tint, the cover's fade, an introduction
//! drawing's ink, a call's blurred backdrop -- because on Sailfish's Qt
//! 5.6 a `ShaderEffect` can be drawn with another one's program (see
//! `qml_avatar_shaders.rs`). So this is what keeps those looking as they
//! did.
//!
//! The picture is made here, opaque all over, so a transparent corner is
//! one the provider cut and not one the file came with.

#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use piirit_shim::render_picture;

/// The two colours a test picture is made of: its left half and its right.
const LEFT: (u8, u8, u8) = (200, 40, 40);
const RIGHT: (u8, u8, u8) = (40, 160, 60);

/// What was made, as `(a, r, g, b)` rows, and its width and height.
type Made = (Vec<(u8, u8, u8, u8)>, usize, usize);

/// A directory of this test's own, so tests running side by side do not
/// share a picture.
fn a_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("piirit-pictures-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    dir
}

/// A wide, opaque picture, written as a PPM, which Qt reads with no plugin.
fn a_picture(dir: &Path) -> PathBuf {
    a_picture_of(dir, 40, 30)
}

fn a_picture_of(dir: &Path, width: usize, height: usize) -> PathBuf {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    for _ in 0..height {
        for x in 0..width {
            let (r, g, b) = if x < width / 2 { LEFT } else { RIGHT };
            bytes.extend([r, g, b]);
        }
    }
    let path = dir.join("picture.ppm");
    std::fs::write(&path, bytes).expect("write the test picture");
    path
}

/// What the provider made for `recipe` from `file`.
fn made(recipe: &str, file: &Path, size: i32) -> Made {
    made_from(&format!("{recipe}?file={}", file.display()), size)
}

fn made_from(id: &str, size: i32) -> Made {
    let (pixels, width, height) =
        render_picture(id, size, size).unwrap_or_else(|| panic!("nothing made for {id}"));
    let pixels = pixels
        .into_iter()
        .map(|argb| {
            let [a, r, g, b] = argb.to_be_bytes();
            (a, r, g, b)
        })
        .collect();
    (pixels, width, height)
}

fn at(pixels: &[(u8, u8, u8, u8)], width: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
    pixels[y * width + x]
}

#[test]
fn a_face_is_the_middle_of_its_picture_cut_to_a_circle_at_its_size() {
    let file = a_picture(&a_dir("circle"));
    let (pixels, width, height) = made("face", &file, 24);

    assert_eq!((width, height), (24, 24), "a face is not made at its size");
    for (x, y) in [(0, 0), (23, 0), (0, 23), (23, 23), (2, 2)] {
        assert_eq!(
            at(&pixels, width, x, y).0,
            0,
            "the face's corner ({x}, {y}) is drawn: it is square, not a circle"
        );
    }
    for (x, y) in [(12, 12), (12, 1), (1, 12), (12, 22), (22, 12)] {
        assert_eq!(
            at(&pixels, width, x, y).0,
            255,
            "the face is not whole at ({x}, {y}), inside its circle"
        );
    }
    // The middle of the picture: its long side cropped evenly, so the
    // circle's left is the picture's left half and its right its right.
    let (_, r, g, b) = at(&pixels, width, 3, 12);
    assert_eq!((r, g, b), LEFT, "the face's left is not the picture's");
    let (_, r, g, b) = at(&pixels, width, 20, 12);
    assert_eq!((r, g, b), RIGHT, "the face's right is not the picture's");
}

#[test]
fn a_face_drawn_without_its_colours_is_grey_and_a_lit_one_goes_through_the_tint() {
    let file = a_picture(&a_dir("colours"));

    let (grey, width, _) = made_from(&format!("face?file={}&grey=1", file.display()), 24);
    for x in [3, 20] {
        let (_, r, g, b) = at(&grey, width, x, 12);
        assert!(
            r == g && g == b,
            "a face drawn without its colours still has them: ({r}, {g}, {b})"
        );
    }

    // Three quarters of the way to the stub ambience's highlight, from
    // grey: blue above red, as the highlight is, and no channel at the
    // picture's own.
    let (lit, width, _) = made_from(
        &format!("face?file={}&tint=%2380c0ff&strength=0.75", file.display()),
        24,
    );
    let (_, r, g, b) = at(&lit, width, 3, 12);
    assert!(
        b > g && g > r,
        "a lit face is not drawn through the highlight: ({r}, {g}, {b})"
    );
    assert_ne!((r, g, b), LEFT, "a lit face is drawn in its own colours");
}

#[test]
fn a_fading_face_is_whole_above_the_fade_and_gone_at_its_foot() {
    let file = a_picture(&a_dir("fade"));
    let (pixels, width, _) = made_from(&format!("face?file={}&from=0.5&to=1", file.display()), 24);

    assert_eq!(
        at(&pixels, width, 12, 4).0,
        255,
        "the face fades above the band"
    );
    let middle = at(&pixels, width, 12, 17).0;
    assert!(
        middle > 0 && middle < 255,
        "the face does not fade inside the band: {middle}"
    );
    assert!(
        at(&pixels, width, 12, 23).0 <= 2,
        "the face's foot is still drawn where the fade has ended"
    );
}

#[test]
fn a_picture_that_is_not_there_is_nothing_rather_than_a_crash() {
    assert!(
        render_picture("face?file=/no/such/picture.png", 24, 24).is_none(),
        "a face was made of a file that is not there"
    );
    assert!(
        render_picture("teapot?file=/no/such/picture.png", 24, 24).is_none(),
        "a picture was made of a kind there is none of"
    );
}

#[test]
fn a_call_backdrop_is_small_and_soft() {
    // Larger than the backdrop is blurred at, so what is read is the
    // blur's and not the scaling's.
    let file = a_picture_of(&a_dir("blur"), 400, 300);
    let (pixels, width, height) = made("blur", &file, -1);

    assert!(
        width <= 64 && height <= 64 && width > 0,
        "the backdrop is blurred at {width} by {height}, not small"
    );
    // A few pixels either side of where the two halves meet, each side
    // has some of the other.
    let middle = width / 2;
    let (_, left, _, _) = at(&pixels, width, middle - 3, height / 2);
    let (_, right, _, _) = at(&pixels, width, middle + 3, height / 2);
    assert!(
        left < LEFT.0 && right > RIGHT.0,
        "the backdrop's halves meet hard, so it is not blurred: {left}, {right}"
    );
}

#[test]
fn an_introduction_drawing_is_inked_where_it_is_drawn_and_clear_elsewhere() {
    let art = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../qml/art/intro-lock.png")
        .canonicalize()
        .expect("the committed art is there");
    let (pixels, _, _) = made_from(
        &format!(
            "ink?file={}&ink=%23ff0000&lit=%230000ff&inkStrength=1&litStrength=1",
            art.display()
        ),
        96,
    );

    let drawn: Vec<_> = pixels.iter().filter(|pixel| pixel.0 > 200).collect();
    assert!(!drawn.is_empty(), "nothing of the drawing is inked");
    assert!(
        pixels.iter().any(|pixel| pixel.0 == 0),
        "the drawing has no clear ground: it is a square of ink"
    );
    assert!(
        drawn
            .iter()
            .all(|&&(_, r, g, b)| g == 0 && (r > 0 || b > 0)),
        "the drawing is inked in a colour it was not given"
    );
}
