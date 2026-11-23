//! The filters of the equalizer and the maths of their response.
//!
//! PipeWire's filter-chain does the processing with its built-in biquads;
//! the shell needs the same filters on paper to draw the curve and to know
//! how far the sum of the bands boosts, so the preamp can make room for it.
//! The formulas are Robert Bristow-Johnson's Audio EQ Cookbook, which is
//! what PipeWire's `bq_peaking`, `bq_lowshelf` and `bq_highshelf` use too.

use std::f32::consts::PI;

pub const BAND_COUNT: usize = 10;

/// Centre of each band, one octave apart.
pub const FREQUENCIES: [f32; BAND_COUNT] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// What a band is called on screen.
pub const LABELS: [&str; BAND_COUNT] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

/// The most a band boosts or cuts, in dB.
pub const MAX_GAIN_DB: f32 = 12.0;

/// The most the preamp boosts or cuts, in dB.
pub const MAX_PREAMP_DB: f32 = 12.0;

/// Sample rate the curve is drawn for. The graph itself runs at whatever
/// rate PipeWire does; the difference is invisible at this resolution.
const CURVE_RATE: f32 = 48_000.0;

/// Q of a peaking band: an octave wide, so neighbours overlap into a smooth
/// curve.
const PEAK_Q: f32 = 1.41;
/// Q of the shelves at either end: no overshoot before the corner.
const SHELF_Q: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    LowShelf,
    Peaking,
    HighShelf,
}

impl Shape {
    /// The filter-chain plugin label.
    pub fn label(self) -> &'static str {
        match self {
            Self::LowShelf => "bq_lowshelf",
            Self::Peaking => "bq_peaking",
            Self::HighShelf => "bq_highshelf",
        }
    }
}

/// The lowest and highest bands are shelves, so a boost there lifts
/// everything beyond it; the rest are peaks.
pub fn shape(band: usize) -> Shape {
    match band {
        0 => Shape::LowShelf,
        b if b == BAND_COUNT - 1 => Shape::HighShelf,
        _ => Shape::Peaking,
    }
}

pub fn q(band: usize) -> f32 {
    match shape(band) {
        Shape::Peaking => PEAK_Q,
        _ => SHELF_Q,
    }
}

/// One biquad section, coefficients divided through by `a0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    pub fn design(shape: Shape, freq: f32, q: f32, gain_db: f32, rate: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let shelf = 2.0 * a.sqrt() * alpha;

        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            Shape::LowShelf => (
                a * ((a + 1.0) - (a - 1.0) * cos + shelf),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - shelf),
                (a + 1.0) + (a - 1.0) * cos + shelf,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - shelf,
            ),
            Shape::HighShelf => (
                a * ((a + 1.0) + (a - 1.0) * cos + shelf),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - shelf),
                (a + 1.0) - (a - 1.0) * cos + shelf,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - shelf,
            ),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Gain at `freq`, in dB.
    pub fn magnitude_db(&self, freq: f32, rate: f32) -> f32 {
        let w = 2.0 * PI * freq / rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        10.0 * (num / den.max(f32::MIN_POSITIVE)).max(f32::MIN_POSITIVE).log10()
    }
}

/// The biquad of `band` at `gain_db`.
pub fn band_filter(band: usize, gain_db: f32) -> Biquad {
    Biquad::design(shape(band), FREQUENCIES[band], q(band), gain_db, CURVE_RATE)
}

/// What all bands together do at `freq`, in dB, preamp not included.
pub fn response_db(gains: &[f32; BAND_COUNT], freq: f32) -> f32 {
    gains
        .iter()
        .enumerate()
        .filter(|(_, gain)| gain.abs() > f32::EPSILON)
        .map(|(band, gain)| band_filter(band, *gain).magnitude_db(freq, CURVE_RATE))
        .sum()
}

/// The response at `points` frequencies, spaced evenly on a log axis from
/// 20 Hz to 20 kHz.
pub fn curve(gains: &[f32; BAND_COUNT], points: usize) -> Vec<f32> {
    let filters: Vec<Biquad> = gains
        .iter()
        .enumerate()
        .filter(|(_, gain)| gain.abs() > f32::EPSILON)
        .map(|(band, gain)| band_filter(band, *gain))
        .collect();
    let (low, high) = (20f32.ln(), 20_000f32.ln());
    (0..points)
        .map(|i| {
            let t = if points > 1 {
                i as f32 / (points - 1) as f32
            } else {
                0.0
            };
            let freq = (low + (high - low) * t).exp();
            filters
                .iter()
                .map(|filter| filter.magnitude_db(freq, CURVE_RATE))
                .sum()
        })
        .collect()
}

/// How far above 0 dB the bands lift the signal at their loudest, in dB; 0
/// when they only cut. Taking this off the preamp keeps a boosted mix from
/// clipping.
pub fn headroom_db(gains: &[f32; BAND_COUNT]) -> f32 {
    curve(gains, 200)
        .into_iter()
        .fold(0.0f32, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 0.05;

    #[test]
    fn a_flat_curve_is_flat() {
        let gains = [0.0; BAND_COUNT];
        assert!(curve(&gains, 50).iter().all(|db| db.abs() < 1e-4));
        assert_eq!(headroom_db(&gains), 0.0);
    }

    #[test]
    fn a_peak_reaches_its_gain_at_its_centre() {
        let filter = band_filter(5, 6.0);
        assert!((filter.magnitude_db(FREQUENCIES[5], CURVE_RATE) - 6.0).abs() < EPS);
        // Far from the centre it leaves the signal alone.
        assert!(filter.magnitude_db(60.0, CURVE_RATE).abs() < 0.3);
        let cut = band_filter(5, -9.0);
        assert!((cut.magnitude_db(FREQUENCIES[5], CURVE_RATE) + 9.0).abs() < EPS);
    }

    #[test]
    fn the_shelves_lift_everything_beyond_them() {
        let low = band_filter(0, 8.0);
        assert!((low.magnitude_db(20.0, CURVE_RATE) - 8.0).abs() < 1.0);
        assert!(low.magnitude_db(8000.0, CURVE_RATE).abs() < 0.2);
        let high = band_filter(BAND_COUNT - 1, -8.0);
        assert!((high.magnitude_db(20_000.0, CURVE_RATE) + 8.0).abs() < 1.5);
        assert!(high.magnitude_db(500.0, CURVE_RATE).abs() < 0.2);
    }

    #[test]
    fn headroom_is_the_loudest_boost_and_zero_for_cuts() {
        let mut gains = [0.0; BAND_COUNT];
        gains[5] = 6.0;
        let headroom = headroom_db(&gains);
        assert!(headroom > 5.0 && headroom < 7.0, "{headroom}");

        let cuts = [-3.0; BAND_COUNT];
        assert_eq!(headroom_db(&cuts), 0.0);
    }

    #[test]
    fn the_curve_covers_the_audible_range() {
        let mut gains = [0.0; BAND_COUNT];
        gains[0] = 6.0;
        let points = curve(&gains, 64);
        assert_eq!(points.len(), 64);
        assert!(points[0] > 4.0);
        assert!(points[63].abs() < 0.2);
    }
}
