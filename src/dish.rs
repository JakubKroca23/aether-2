//! One petri dish on the lab table, plus tubes that link dish ports.

use serde::{Deserialize, Serialize};

use crate::field::Fields;
use crate::math::Vec2;
use crate::tune::DISH;
use crate::world::{EdgeZone, Feeder, Food};

/// A single dish: local physics bounds, chemistry, feeders and food.
/// Organism / food positions stored elsewhere are in **dish-local** coordinates
/// (origin at dish center), unless converted via [`PetriDish::to_table`].
#[derive(Clone, Serialize, Deserialize)]
pub struct PetriDish {
    pub id: u32,
    /// Center of the dish on the lab table.
    pub pos: Vec2,
    pub half_x: f32,
    pub half_y: f32,
    pub fields: Fields,
    pub feeders: Vec<Feeder>,
    pub foods: Vec<Food>,
    pub viscosity: f32,
    pub edges: [EdgeZone; 4],
    /// Accumulated dt for throttled pheromone diffusion.
    #[serde(skip)]
    pub diffuse_accum: f32,
}

impl PetriDish {
    pub fn new(id: u32, pos: Vec2) -> Self {
        let mut fields = Fields::new();
        fields.set_bounds(DISH, DISH);
        Self {
            id,
            pos,
            half_x: DISH,
            half_y: DISH,
            fields,
            feeders: Vec::new(),
            foods: Vec::new(),
            viscosity: 1.0,
            edges: [EdgeZone::default(); 4],
            diffuse_accum: 0.0,
        }
    }

    pub fn set_bounds(&mut self, half_x: f32, half_y: f32) {
        self.half_x = half_x.clamp(0.45, 4.0);
        self.half_y = half_y.clamp(0.45, 4.0);
        self.fields.set_bounds(self.half_x, self.half_y);
    }

    pub fn to_table(&self, local: Vec2) -> Vec2 {
        self.pos + local
    }

    pub fn to_local(&self, table: Vec2) -> Vec2 {
        table - self.pos
    }

    pub fn contains_local(&self, p: Vec2, inset: f32) -> bool {
        p.x.abs() <= self.half_x - inset && p.y.abs() <= self.half_y - inset
    }

    pub fn clamp_local(&self, p: Vec2, inset: f32) -> Vec2 {
        Vec2::new(
            p.x.clamp(-self.half_x + inset, self.half_x - inset),
            p.y.clamp(-self.half_y + inset, self.half_y - inset),
        )
    }

    pub fn rim_pos(&self, side: u8, along: f32) -> Vec2 {
        let t = along.clamp(0.0, 1.0);
        match side.min(3) {
            0 => Vec2::new(-self.half_x, -self.half_y + t * 2.0 * self.half_y),
            1 => Vec2::new(self.half_x, -self.half_y + t * 2.0 * self.half_y),
            2 => Vec2::new(-self.half_x + t * 2.0 * self.half_x, -self.half_y),
            _ => Vec2::new(-self.half_x + t * 2.0 * self.half_x, self.half_y),
        }
    }

    pub fn port_inward(side: u8) -> Vec2 {
        match side.min(3) {
            0 => Vec2::new(1.0, 0.0),
            1 => Vec2::new(-1.0, 0.0),
            2 => Vec2::new(0.0, 1.0),
            _ => Vec2::new(0.0, -1.0),
        }
    }

    /// Diffuse pheromones every few subticks, or skip when the field is cold.
    pub fn tick_fields(&mut self, dt: f32) {
        const PERIOD: f32 = 3.0 / 60.0;
        if !self.fields.has_signal() {
            self.diffuse_accum = 0.0;
            return;
        }
        self.diffuse_accum += dt;
        if self.diffuse_accum >= PERIOD {
            self.fields.diffuse(self.diffuse_accum);
            self.diffuse_accum = 0.0;
        }
    }
}

/// Bidirectional tube linking a port on dish A to a port on dish B.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tube {
    pub a_dish: u32,
    pub a_side: u8,
    pub a_along: f32,
    pub b_dish: u32,
    pub b_side: u8,
    pub b_along: f32,
    /// Mouth capture radius in dish-local units.
    pub radius: f32,
}

impl Tube {
    pub fn new(
        a_dish: u32,
        a_side: u8,
        a_along: f32,
        b_dish: u32,
        b_side: u8,
        b_along: f32,
    ) -> Self {
        Self {
            a_dish,
            a_side: a_side.min(3),
            a_along: a_along.clamp(0.0, 1.0),
            b_dish,
            b_side: b_side.min(3),
            b_along: b_along.clamp(0.0, 1.0),
            radius: 0.09,
        }
    }
}
