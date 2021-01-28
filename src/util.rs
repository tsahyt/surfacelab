/// Exponential Moving Average of a type. Used to keep track of recent timing
/// information (e.g. frame times or compute times for nodes).
///
/// Computation of EMA follows the formula `EMA_t = EMA_(t-1) + alpha * (new -
/// EMA_(t-1))`, where alpha is the decay parameter and new is the latest
/// measurement.
pub struct EMA<T> {
    ema: T,
    alpha: T,
    extra: T,
}

impl<T> EMA<T>
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Mul<T, Output = T>
        + std::ops::Add<T, Output = T>
        + num::Num
        + Copy,
{
    /// Create a new EMA with a given initial value and a decay parameter,
    /// called alpha, documentation.
    pub fn new(alpha: T) -> Self {
        Self {
            ema: T::zero(),
            alpha,
            extra: T::one(),
        }
    }

    /// Update the EMA with a new measurement
    pub fn update(&mut self, measurement: T) {
        self.ema = self.alpha * self.ema + (T::one() - self.alpha) * measurement;
        self.extra = self.alpha * self.extra
    }

    /// Get the current value of the EMA
    pub fn get(&self) -> T {
        self.ema / (T::one() - self.extra)
    }
}

/// An iterator over a 2D (2,3)-Halton sequence for QMC, except index 0 is added
/// as (0.5, 0.5) to get a clean center sample first.
pub struct HaltonSequence2D {
    idx: usize,
    base1: f32,
    base2: f32,
}

impl Default for HaltonSequence2D {
    fn default() -> Self {
        Self::new(2, 3)
    }
}

impl HaltonSequence2D {
    /// Initialize an n,m-Halton sequence
    pub fn new(base1: usize, base2: usize) -> Self {
        Self {
            idx: 0,
            base1: base1 as f32,
            base2: base2 as f32,
        }
    }

    fn halton_1d(mut idx: f32, base: f32) -> f32 {
        let mut fraction = 1.0;
        let mut result = 0.0;

        while idx > 0.0 {
            fraction /= base;
            result += fraction * (idx % base);
            idx = (idx / base).floor();
        }

        result
    }
}

impl Iterator for HaltonSequence2D {
    type Item = (f32, f32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == 0 {
            self.idx += 1;
            Some((0.5, 0.5))
        } else {
            let x = Self::halton_1d((self.idx - 1) as f32, self.base1);
            let y = Self::halton_1d((self.idx - 1) as f32, self.base2);
            self.idx += 1;
            Some((x, y))
        }
    }
}
