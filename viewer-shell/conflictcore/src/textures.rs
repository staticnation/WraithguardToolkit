//! Texture conflicts (port step 4.3, slice 7): which two providers a comparison shows, and
//! what comparing their images concludes (`wraithguard/images/compare.py`, the pair from
//! `gui/conflicts.py` `_texture_provider_dirs`). Decoding is the caller's (the viewer's image
//! readers, or `wraithguard.images` through native); this works on RGBA.

/// A channel gap at or below this is the same colour: re-compressing moves nearly every pixel
/// by a level or two without changing the image.
pub const SAME: u32 = 2;

/// Above this many pixels a pair is not compared pixel by pixel.
pub const MAX_COMPARE: usize = 16 << 20;

/// The two providers a texture comparison shows: the one the winner replaced, then the
/// winner (the last two in load order); `None` with fewer than two.
pub fn pair(providers: &[String]) -> Option<(&str, &str)> {
    match providers {
        [.., a, b] => Some((a.as_str(), b.as_str())),
        _ => None,
    }
}

/// Whether a path is a texture the comparison reads (`_TEXTURE_EXTENSIONS`).
pub fn is_texture(path: &str) -> bool {
    let p = path.to_lowercase();
    [".dds", ".tga", ".bmp", ".png"].iter().any(|e| p.ends_with(e))
}

/// A decoded image: width, height and RGBA bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// What a comparison concluded (`Verdict`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Identical,
    SamePixels,
    Different,
    DifferentSize,
    Undecodable,
}

impl Verdict {
    /// The Python enum's value.
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Identical => "identical",
            Verdict::SamePixels => "same-pixels",
            Verdict::Different => "different",
            Verdict::DifferentSize => "different-size",
            Verdict::Undecodable => "undecodable",
        }
    }
}

/// The outcome and the numbers behind it (`Comparison`).
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub verdict: Verdict,
    pub detail: String,
    pub left_size: Option<(usize, usize)>,
    pub right_size: Option<(usize, usize)>,
    /// The share of pixels that differ beyond [`SAME`].
    pub changed_share: f64,
    /// The largest single-channel gap, 0-255.
    pub worst_channel: u32,
    /// The mean absolute channel gap.
    pub mean_channel: f64,
}

impl Comparison {
    fn bare(verdict: Verdict, detail: impl Into<String>) -> Self {
        Comparison {
            verdict,
            detail: detail.into(),
            left_size: None,
            right_size: None,
            changed_share: 0.0,
            worst_channel: 0,
            mean_channel: 0.0,
        }
    }

    /// Whether a user would see any difference.
    pub fn differs(&self) -> bool {
        matches!(self.verdict, Verdict::Different | Verdict::DifferentSize)
    }
}

/// Pixels changed beyond `same`, the largest channel gap, and the summed gap, over two
/// equal-length RGBA buffers (`_measure`).
pub fn measure(left: &[u8], right: &[u8], same: u32) -> (u64, u32, u64) {
    let (mut changed, mut worst, mut total) = (0u64, 0u32, 0u64);
    for (a, b) in left.chunks(4).zip(right.chunks(4)) {
        let mut biggest = 0u32;
        for (x, y) in a.iter().zip(b) {
            let gap = u32::from(x.abs_diff(*y));
            total += u64::from(gap);
            biggest = biggest.max(gap);
        }
        if biggest > same {
            changed += 1;
        }
        worst = worst.max(biggest);
    }
    (changed, worst, total)
}

/// Where two equal-sized images differ (`difference_image`): each channel's gap times
/// `amplify`, an alpha gap folded into the colour, fully opaque.
pub fn difference(left: &[u8], right: &[u8], amplify: u32) -> Vec<u8> {
    let scale = |g: u8| (u64::from(g) * u64::from(amplify)).min(255) as u8;
    let mut out = vec![0u8; left.len().min(right.len())];
    for ((o, a), b) in out.chunks_mut(4).zip(left.chunks(4)).zip(right.chunks(4)) {
        if o.len() < 4 || a.len() < 4 || b.len() < 4 {
            break;
        }
        for c in 0..3 {
            o[c] = scale(a[c].abs_diff(b[c]));
        }
        let alpha_gap = a[3].abs_diff(b[3]);
        if alpha_gap != 0 {
            let capped = scale(alpha_gap);
            for v in o.iter_mut().take(3) {
                *v = (*v).max(capped);
            }
        }
        o[3] = 255;
    }
    out
}

/// Two decoded images compared (`compare_images`, same role on both sides).
pub fn compare_images(left: &Rgba, right: &Rgba) -> Comparison {
    let ls = (left.width, left.height);
    let rs = (right.width, right.height);
    let sized = |verdict, detail: String| Comparison { left_size: Some(ls), right_size: Some(rs), ..Comparison::bare(verdict, detail) };
    if ls != rs {
        return sized(
            Verdict::DifferentSize,
            format!("{}x{} against {}x{}, so the pixels were not compared", ls.0, ls.1, rs.0, rs.1),
        );
    }
    if left.pixels == right.pixels {
        return sized(Verdict::SamePixels, "different files, identical images -- re-saved or re-compressed".into());
    }
    let pixels = left.width * left.height;
    if pixels > MAX_COMPARE {
        return sized(Verdict::Different, format!("{}x{} is too large to compare pixel by pixel", ls.0, ls.1));
    }
    let (changed, worst, total) = measure(&left.pixels, &right.pixels, SAME);
    let share = if pixels > 0 { changed as f64 / pixels as f64 } else { 0.0 };
    let mean = if pixels > 0 { total as f64 / (pixels as f64 * 4.0) } else { 0.0 };
    Comparison {
        changed_share: share,
        worst_channel: worst,
        mean_channel: mean,
        ..sized(Verdict::Different, format!("{:.1}% of pixels differ, worst channel by {worst}", share * 100.0))
    }
}

/// Two texture files compared (`compare_bytes`): `decode` reads one into RGBA, or says why
/// it cannot.
pub fn compare_bytes(left: &[u8], right: &[u8], decode: impl Fn(&[u8]) -> Result<Rgba, String>) -> Comparison {
    if left == right {
        return Comparison::bare(Verdict::Identical, "byte-for-byte identical");
    }
    let first = match decode(left) {
        Ok(i) => i,
        Err(e) => return Comparison::bare(Verdict::Undecodable, format!("the first could not be decoded: {e}")),
    };
    let second = match decode(right) {
        Ok(i) => i,
        Err(e) => {
            return Comparison {
                left_size: Some((first.width, first.height)),
                ..Comparison::bare(Verdict::Undecodable, format!("the second could not be decoded: {e}"))
            };
        }
    };
    compare_images(&first, &second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(px: &[[u8; 4]]) -> Rgba {
        Rgba { width: px.len(), height: 1, pixels: px.iter().flatten().copied().collect() }
    }

    #[test]
    fn the_pair_is_the_winner_and_what_it_replaced() {
        let p: Vec<String> = ["a", "b", "c"].iter().map(|s| (*s).to_string()).collect();
        assert_eq!(pair(&p), Some(("b", "c")));
        assert_eq!(pair(&p[..1]), None);
    }

    #[test]
    fn a_comparison_says_how_much_and_where() {
        let a = img(&[[0, 0, 0, 255], [10, 10, 10, 255]]);
        let b = img(&[[1, 1, 1, 255], [60, 10, 10, 200]]);
        let c = compare_images(&a, &b);
        assert_eq!(c.verdict, Verdict::Different);
        assert_eq!(c.worst_channel, 55);
        assert!((c.changed_share - 0.5).abs() < 1e-9);
        assert_eq!(c.detail, "50.0% of pixels differ, worst channel by 55");
        assert_eq!(difference(&a.pixels, &b.pixels, 4), [4, 4, 4, 255, 220, 220, 220, 255]);
        assert_eq!(compare_images(&a, &a.clone()).verdict, Verdict::SamePixels);
        let wide = Rgba { width: 1, height: 2, pixels: a.pixels.clone() };
        assert_eq!(compare_images(&a, &wide).verdict, Verdict::DifferentSize);
        assert_eq!(compare_bytes(b"x", b"x", |_| Err(String::new())).verdict, Verdict::Identical);
        assert_eq!(compare_bytes(b"x", b"y", |_| Err("bad".into())).detail, "the first could not be decoded: bad");
    }
}
