use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len < 1e-5 {
            Self::new(1.0, 0.0)
        } else {
            self * (1.0 / len)
        }
    }

    pub fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    pub fn rotate(self, ang: f32) -> Self {
        let (s, c) = ang.sin_cos();
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Self) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Self) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}

pub fn gaussian(rng: &mut impl rand::Rng) -> f32 {
    let u: f32 = (rng.gen::<f32>() as f32).max(1e-6);
    let v = rng.gen::<f32>();
    (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
}

pub fn wrap_unit(x: f32) -> f32 {
    let x = x.rem_euclid(1.0);
    if x < 0.0 {
        x + 1.0
    } else {
        x
    }
}

pub fn hue_similarity(a: f32, b: f32) -> f32 {
    let d = (a - b).abs().min(1.0 - (a - b).abs());
    (1.0 - d * 2.0).clamp(0.0, 1.0)
}
