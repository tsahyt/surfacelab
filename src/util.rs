/// Exponential Moving Average of a type. Used to keep track of recent timing
/// information (e.g. frame times or compute times for nodes).
///
/// Computation of EMA follows the formula `EMA_t = EMA_(t-1) + alpha * (new -
/// EMA_(t-1))`, where alpha is the decay parameter and new is the latest
/// measurement.
pub struct EMA<T> {
    ema: T,
    alpha: T,
}

impl<T> EMA<T>
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Mul<T, Output = T>
        + std::ops::Add<T, Output = T>
        + Copy,
{
    /// Create a new EMA with a given initial value and a decay parameter,
    /// called alpha, documentation.
    pub fn new(initial: T, alpha: T) -> Self {
        Self {
            ema: initial,
            alpha,
        }
    }

    /// Update the EMA with a new measurement
    pub fn update(&mut self, measurement: T) {
        self.ema = self.ema + self.alpha * (measurement - self.ema);
    }

    /// Get the current value of the EMA
    pub fn get(&self) -> T {
        self.ema
    }
}
    }
}
