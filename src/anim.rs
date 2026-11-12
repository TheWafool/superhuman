//! Critically damped springs for UI motion.

/// Response time: roughly how long a move takes to settle visually.
const SETTLE: f32 = 0.18;

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub target: f32,
    velocity: f32,
}

impl Spring {
    pub fn new(v: f32) -> Self {
        Self { value: v, target: v, velocity: 0.0 }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    pub fn snap(&mut self, v: f32) {
        *self = Self::new(v);
    }

    pub fn settled(&self) -> bool {
        (self.value - self.target).abs() < 0.05 && self.velocity.abs() < 0.05
    }

    /// Advance by `dt` seconds. `instant` jumps straight to the target (reduce motion).
    pub fn step(&mut self, dt: f32, instant: bool) {
        if instant || self.settled() {
            self.snap(self.target);
            return;
        }
        // Critically damped: omega = 2 / settle_time-ish.
        let omega = 4.0 / SETTLE;
        let mut x = self.value - self.target;
        let mut v = self.velocity;
        // Sub-step for stability on long frames.
        let steps = ((dt.min(0.1) / 0.004).ceil() as usize).max(1);
        let h = dt.min(0.1) / steps as f32;
        for _ in 0..steps {
            let a = -omega * omega * x - 2.0 * omega * v;
            v += a * h;
            x += v * h;
        }
        self.velocity = v;
        self.value = self.target + x;
    }
}

/// A 0..1 spring with a precision suited to blends rather than pixels.
#[derive(Debug, Clone, Copy)]
pub struct Blend(Spring);

impl Blend {
    pub fn new(v: f32) -> Self {
        Self(Spring::new(v * 100.0))
    }
    pub fn set(&mut self, t: f32) {
        self.0.set(t * 100.0);
    }
    pub fn snap(&mut self, t: f32) {
        self.0.snap(t * 100.0);
    }
    pub fn get(&self) -> f32 {
        (self.0.value / 100.0).clamp(0.0, 1.0)
    }
    pub fn settled(&self) -> bool {
        self.0.settled()
    }
    pub fn step(&mut self, dt: f32, instant: bool) {
        self.0.step(dt, instant);
    }
}
