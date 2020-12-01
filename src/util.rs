pub struct EMA<T> {
    ema: T,
    decay: T,
}

impl<T> EMA<T>
where
    T: std::ops::Sub<T, Output = T>
        + std::ops::Mul<T, Output = T>
        + std::ops::Add<T, Output = T>
        + Copy,
{
    pub fn new(initial: T, decay: T) -> Self {
        Self {
            ema: initial,
            decay,
        }
    }

    pub fn update(&mut self, measurement: T) {
        self.ema = self.ema + self.decay * (measurement - self.ema);
    }

    pub fn get(&self) -> &T {
        &self.ema
    }
}
