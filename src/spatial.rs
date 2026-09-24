//! Uniform grid for near-neighbour queries (collisions, crowd, bites).

use crate::math::Vec2;

#[derive(Default)]
pub struct SpatialHash {
    cell: f32,
    origin_x: f32,
    origin_y: f32,
    nx: usize,
    ny: usize,
    buckets: Vec<Vec<usize>>,
}

impl SpatialHash {
    pub fn clear_and_build(&mut self, half_x: f32, half_y: f32, cell: f32, positions: &[Vec2]) {
        let cell = cell.max(0.08);
        let nx = ((2.0 * half_x / cell).ceil() as usize).max(1);
        let ny = ((2.0 * half_y / cell).ceil() as usize).max(1);
        let need = nx * ny;
        if self.buckets.len() != need {
            self.buckets = vec![Vec::new(); need];
        } else {
            for b in &mut self.buckets {
                b.clear();
            }
        }
        self.cell = cell;
        self.origin_x = -half_x;
        self.origin_y = -half_y;
        self.nx = nx;
        self.ny = ny;
        for (i, p) in positions.iter().enumerate() {
            let (cx, cy) = self.cell_of(*p);
            self.buckets[cy * self.nx + cx].push(i);
        }
    }

    fn cell_of(&self, p: Vec2) -> (usize, usize) {
        let cx = ((p.x - self.origin_x) / self.cell).floor() as i32;
        let cy = ((p.y - self.origin_y) / self.cell).floor() as i32;
        (
            cx.clamp(0, self.nx as i32 - 1) as usize,
            cy.clamp(0, self.ny as i32 - 1) as usize,
        )
    }

    /// Visit unique unordered pairs `(i, j)` with `i < j` in neighbouring cells.
    pub fn for_each_pair(&self, mut f: impl FnMut(usize, usize)) {
        for cy in 0..self.ny {
            for cx in 0..self.nx {
                let home = &self.buckets[cy * self.nx + cx];
                for &i in home {
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx < 0 || ny < 0 || nx >= self.nx as i32 || ny >= self.ny as i32 {
                                continue;
                            }
                            for &j in &self.buckets[ny as usize * self.nx + nx as usize] {
                                if j > i {
                                    f(i, j);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

}
