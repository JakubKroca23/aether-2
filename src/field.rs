use serde::{Deserialize, Serialize};

use crate::math::Vec2;
use crate::tune::{DISH, GRID};

#[derive(Clone, Copy)]
pub enum Channel {
    Signal,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Map {
    nx: usize,
    ny: usize,
    half_x: f32,
    half_y: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Fields {
    cells: usize,
    map: Map,
    signal: Vec<f32>,
    signal_b: Vec<f32>,
}

impl Fields {
    pub fn new() -> Self {
        let map = Map {
            nx: GRID,
            ny: GRID,
            half_x: DISH,
            half_y: DISH,
        };
        let z = vec![0.0; map.nx * map.ny];
        Self {
            cells: GRID,
            map,
            signal: z.clone(),
            signal_b: z,
        }
    }

    pub fn set_cells(&mut self, cells: usize) {
        self.cells = cells.clamp(32, 384);
    }

    pub fn cells(&self) -> usize {
        self.cells
    }

    /// The dish rectangle in world units. Cells stay square when the window is wider than tall.
    pub fn set_bounds(&mut self, half_x: f32, half_y: f32) {
        let half_x = half_x.max(0.25);
        let half_y = half_y.max(0.25);
        let ny = self.cells;
        let nx = ((self.cells as f32) * (half_x / half_y)).round().max(8.0) as usize;
        let next = Map {
            nx,
            ny,
            half_x,
            half_y,
        };
        if nx == self.map.nx
            && ny == self.map.ny
            && (half_x - self.map.half_x).abs() < 1e-4
            && (half_y - self.map.half_y).abs() < 1e-4
        {
            return;
        }
        let old = self.map;
        let pull = |src: &[f32]| {
            let mut out = vec![0.0; nx * ny];
            for y in 0..ny {
                for x in 0..nx {
                    let p = cell_center(next, x, y);
                    out[y * nx + x] = bilinear(src, old, p);
                }
            }
            out
        };
        self.signal = pull(&self.signal);
        self.signal_b = vec![0.0; nx * ny];
        self.map = next;
    }

    pub fn sample(&self, channel: Channel, p: Vec2) -> f32 {
        let grid = match channel {
            Channel::Signal => &self.signal,
        };
        bilinear(grid, self.map, p)
    }

    pub fn add_blob(&mut self, channel: Channel, p: Vec2, radius: f32, amount: f32) {
        if amount.abs() < 1e-8 || radius <= 1e-4 {
            return;
        }
        let grid = match channel {
            Channel::Signal => &mut self.signal,
        };
        paint(grid, self.map, p, radius, amount);
    }

    pub fn diffuse(&mut self, dt: f32) {
        blur(
            &mut self.signal,
            &mut self.signal_b,
            self.map,
            0.20,
            1.6,
            dt,
        );
    }

    /// True if any cell still carries a usable signal (skip diffuse when cold).
    pub fn has_signal(&self) -> bool {
        self.signal.iter().any(|&v| v > 1e-5)
    }

    pub fn fill_rgba(&self, width: usize, height: usize, out: &mut [u8]) {
        for y in 0..height {
            for x in 0..width {
                let p = Vec2::new(
                    ((x as f32 + 0.5) / width as f32 * 2.0 - 1.0) * self.map.half_x,
                    (1.0 - (y as f32 + 0.5) / height as f32 * 2.0) * self.map.half_y,
                );
                let i = (y * width + x) * 4;
                out[i] = 0;
                out[i + 1] = 0;
                out[i + 2] = encode(self.sample(Channel::Signal, p) * 1.6);
                out[i + 3] = 0;
            }
        }
    }
}

fn encode(x: f32) -> u8 {
    let y = 1.0 - (-x.max(0.0) * 1.15).exp();
    (y * 255.0).round().clamp(0.0, 255.0) as u8
}

fn in_grid(map: Map, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < map.nx && (y as usize) < map.ny
}

fn cell_of(map: Map, p: Vec2) -> (i32, i32) {
    let x = ((p.x / map.half_x + 1.0) * 0.5 * map.nx as f32).floor() as i32;
    let y = ((p.y / map.half_y + 1.0) * 0.5 * map.ny as f32).floor() as i32;
    (x.clamp(0, map.nx as i32 - 1), y.clamp(0, map.ny as i32 - 1))
}

fn cell_center(map: Map, x: usize, y: usize) -> Vec2 {
    Vec2::new(
        ((x as f32 + 0.5) / map.nx as f32 * 2.0 - 1.0) * map.half_x,
        ((y as f32 + 0.5) / map.ny as f32 * 2.0 - 1.0) * map.half_y,
    )
}

fn cells_for(map: Map, radius: f32) -> i32 {
    let cell = (2.0 * map.half_y / map.ny as f32).max(1e-4);
    (radius / cell).ceil() as i32 + 1
}

fn bilinear(grid: &[f32], map: Map, p: Vec2) -> f32 {
    if p.x.abs() > map.half_x || p.y.abs() > map.half_y {
        return 0.0;
    }
    let u = (p.x / map.half_x + 1.0) * 0.5 * map.nx as f32 - 0.5;
    let v = (p.y / map.half_y + 1.0) * 0.5 * map.ny as f32 - 0.5;
    let x0 = u.floor() as i32;
    let y0 = v.floor() as i32;
    let tx = u - x0 as f32;
    let ty = v - y0 as f32;
    let at = |x: i32, y: i32| {
        if in_grid(map, x, y) {
            grid[(y as usize) * map.nx + x as usize]
        } else {
            0.0
        }
    };
    let a = at(x0, y0);
    let b = at(x0 + 1, y0);
    let c = at(x0, y0 + 1);
    let d = at(x0 + 1, y0 + 1);
    let ab = a + (b - a) * tx;
    let cd = c + (d - c) * tx;
    ab + (cd - ab) * ty
}

fn paint(grid: &mut [f32], map: Map, p: Vec2, radius: f32, amount: f32) {
    let (cx, cy) = cell_of(map, p);
    let rad_cells = cells_for(map, radius);
    let mut sum = 0.0;
    let r2 = radius * radius;
    for y in cy - rad_cells..=cy + rad_cells {
        for x in cx - rad_cells..=cx + rad_cells {
            if !in_grid(map, x, y) {
                continue;
            }
            let q = cell_center(map, x as usize, y as usize);
            let d2 = (q - p).length().powi(2);
            if d2 <= r2 * 4.0 {
                sum += (-d2 / (2.0 * r2.max(1e-4))).exp();
            }
        }
    }
    if sum <= 1e-6 {
        return;
    }
    for y in cy - rad_cells..=cy + rad_cells {
        for x in cx - rad_cells..=cx + rad_cells {
            if !in_grid(map, x, y) {
                continue;
            }
            let q = cell_center(map, x as usize, y as usize);
            let d2 = (q - p).length().powi(2);
            if d2 > r2 * 4.0 {
                continue;
            }
            let w = (-d2 / (2.0 * r2.max(1e-4))).exp();
            let i = (y as usize) * map.nx + x as usize;
            grid[i] = (grid[i] + amount * w / sum).max(0.0);
        }
    }
}

fn blur(cur: &mut Vec<f32>, tmp: &mut Vec<f32>, map: Map, rate: f32, decay: f32, dt: f32) {
    let a = (rate * dt * 60.0).min(0.2);
    for y in 0..map.ny {
        for x in 0..map.nx {
            let i = y * map.nx + x;
            let c = cur[i];
            let l = if x > 0 { cur[i - 1] } else { 0.0 };
            let r = if x + 1 < map.nx { cur[i + 1] } else { 0.0 };
            let u = if y > 0 { cur[i - map.nx] } else { 0.0 };
            let d = if y + 1 < map.ny { cur[i + map.nx] } else { 0.0 };
            let v = (c + a * (l + r + u + d - 4.0 * c)) * (1.0 - decay * dt);
            tmp[i] = v.max(0.0);
        }
    }
    std::mem::swap(cur, tmp);
}
