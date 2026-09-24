use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::dish::{PetriDish, Tube};
use crate::field::Channel;
use crate::genome::Genome;
use crate::math::{hue_similarity, Vec2};
use crate::organism::{lay_inputs, random_axis, Organism, Senses};
use crate::spatial::SpatialHash;
use crate::tune::*;

pub struct Spark {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
    pub hue: f32,
    pub size: f32,
    /// 0 = soft mote, 1 = blood spray, 2 = death chunk, 3 = bite flash ring.
    pub style: u8,
}

pub struct Flash {
    pub pos: Vec2,
    pub life: f32,
    pub max_life: f32,
}

/// One placed meal. Smell cloud blooms in, then fades center→out when eaten.
#[derive(Clone, Serialize, Deserialize)]
pub struct Food {
    pub pos: Vec2,
    pub kind: FoodKind,
    /// How far the smell has expanded outward `[0, 1]`.
    pub bloom: f32,
    /// When set, the meal was eaten; fade progresses `[0, 1]` clearing center first.
    pub fade: Option<f32>,
}

impl Food {
    pub fn fresh(pos: Vec2, kind: FoodKind) -> Self {
        Self {
            pos,
            kind,
            bloom: 0.0,
            fade: None,
        }
    }

    pub fn alive(&self) -> bool {
        self.fade.is_none()
    }

    /// Visual / sensory smell radius scale.
    pub fn smell_scale(&self) -> f32 {
        if self.fade.is_some() {
            0.0
        } else {
            // ease-out bloom
            let t = self.bloom.clamp(0.0, 1.0);
            1.0 - (1.0 - t) * (1.0 - t)
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum FoodKind {
    /// Common green — balanced smell and energy.
    Green = 0,
    /// Rich amber — short smell, high energy.
    Amber = 1,
    /// Toxic violet — shorter smell than before so it does not dominate the dish.
    Toxic = 2,
}

impl FoodKind {
    pub const ALL: [FoodKind; FOOD_KIND_COUNT] =
        [FoodKind::Green, FoodKind::Amber, FoodKind::Toxic];

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % FOOD_KIND_COUNT]
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "zelené",
            Self::Amber => "zlaté",
            Self::Toxic => "jedovaté",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Green => "Zel",
            Self::Amber => "Zla",
            Self::Toxic => "Jed",
        }
    }
}

/// Mostly green starter meals, some amber, rare toxic — founders are not baited into poison first.
fn starter_food_kind(i: usize) -> FoodKind {
    match i % 8 {
        0 | 1 | 2 | 3 | 4 => FoodKind::Green,
        5 | 6 => FoodKind::Amber,
        _ => FoodKind::Toxic,
    }
}

/// Tunable attributes for one food variety.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FoodSpec {
    pub sense: f32,
    pub energy: f32,
    pub harm: f32,
    /// Legacy field — auto spawn is replaced by edge feeders.
    #[serde(default)]
    pub auto: bool,
    pub batch: usize,
    /// Default interval suggested for new feeders of this kind.
    pub interval: f32,
    pub color: (f32, f32, f32),
}

impl FoodSpec {
    pub fn defaults() -> [FoodSpec; FOOD_KIND_COUNT] {
        [
            FoodSpec {
                sense: 0.42,
                energy: 0.55,
                harm: 0.0,
                auto: false,
                batch: 1,
                interval: 5.0,
                color: (0.32, 0.85, 0.42),
            },
            FoodSpec {
                sense: 0.26,
                energy: 0.88,
                harm: 0.0,
                auto: false,
                batch: 1,
                interval: 7.0,
                color: (0.95, 0.72, 0.22),
            },
            FoodSpec {
                sense: 0.55,
                energy: 0.22,
                harm: 0.35,
                auto: false,
                batch: 1,
                interval: 9.0,
                color: (0.72, 0.28, 0.88),
            },
        ]
    }
}

fn default_feeder_radius() -> f32 {
    0.35
}

fn default_feeder_rate() -> f32 {
    1.0
}

/// Nutrient feeder dispenser in the dish.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feeder {
    #[serde(default)]
    pub pos: Vec2,
    #[serde(default = "default_feeder_radius")]
    pub radius: f32,
    pub kind: FoodKind,
    #[serde(default = "default_feeder_rate")]
    pub rate: f32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timer: f32,
    // Legacy fields for backward compatibility with old saves:
    #[serde(default)]
    pub side: u8,
    #[serde(default)]
    pub along: f32,
    #[serde(default)]
    pub interval: f32,
    #[serde(default)]
    pub batch: usize,
}

impl Feeder {
    pub fn new(pos: Vec2, kind: FoodKind) -> Self {
        Self {
            pos,
            radius: 0.35,
            kind,
            rate: 1.0,
            enabled: false,
            timer: 0.0,
            side: 0,
            along: 0.0,
            interval: 1.0,
            batch: 1,
        }
    }
}

pub struct Appearance<'a> {
    pub id: u64,
    /// Table-space offset of this organism's dish center.
    pub dish_pos: Vec2,
    pub nodes: &'a [Vec2],
    pub radius: f32,
    pub node_radius: f32,
    pub hue: f32,
    pub thrust: f32,
    pub energy: f32,
    pub mouth: f32,
    pub generation: u32,
}

impl Appearance<'_> {
    pub fn world_node(&self, i: usize) -> Vec2 {
        self.nodes.get(i).copied().unwrap_or(Vec2::ZERO) + self.dish_pos
    }

    pub fn world_nodes(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.nodes.iter().map(|n| *n + self.dish_pos)
    }
}

#[derive(Clone)]
pub struct Stats {
    pub id: u64,
    pub hue: f32,
    pub energy: f32,
    pub hunger: f32,
    pub pain: f32,
    pub novelty: f32,
    pub repro: f32,
    pub pulse: f32,
    pub heart: f32,
    pub tonic: f32,
    pub age: f32,
    pub generation: u32,
    pub neurons: usize,
    pub synapses: usize,
    pub nodes: u8,
    pub mass: f32,
    pub body: f32,
    pub hetero: f32,
    pub setpoint: f32,
    pub learn: f32,
    pub hunger_w: f32,
    pub pain_w: f32,
    pub novelty_w: f32,
    pub birth_w: f32,
    pub maturity: f32,
    pub root_nodes: u8,
    pub root_mass: f32,
    pub root_body: f32,
    pub root_hue: f32,
    pub root_hetero: f32,
    pub root_setpoint: f32,
    pub root_learn: f32,
    pub root_hunger_w: f32,
    pub root_pain_w: f32,
    pub root_novelty_w: f32,
    pub root_birth_w: f32,
    pub root_maturity: f32,
    pub gene_drift: f32,
    pub food_taste: [f32; 3],
}

pub struct Wire {
    pub from: usize,
    pub to: usize,
    pub weight: f32,
}

pub struct Net {
    pub kind: Vec<u8>,
    pub act: Vec<f32>,
    pub wires: Vec<Wire>,
    pub labels: Vec<String>,
}

pub struct Census {
    pub alive: usize,
    pub food: usize,
    pub mean_energy: f32,
    pub max_generation: u32,
    pub mean_neurons: f32,
    pub mean_synapses: f32,
    pub mean_age: f32,
    pub mean_hunger: f32,
    pub mean_mass: f32,
    pub hungry: usize,
    pub time: f32,
    pub births: u64,
    pub deaths: u64,
    pub feeders: usize,
    pub food_sense: f32,
}

/// Who is allowed to continue when a generation ends.
/// `Live` keeps the dish on energy and damage, which is the running default.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum Survival {
    Live,
    RightSide,
    Edges,
    Center,
    Shifting,
}

/// Effect of a dish edge zone. Applied when an organism enters the reach band.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EdgeEffect {
    None,
    Dopamine,
    Hurt,
    Heal,
    Energy,
}

impl EdgeEffect {
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Dopamine,
            Self::Dopamine => Self::Hurt,
            Self::Hurt => Self::Heal,
            Self::Heal => Self::Energy,
            Self::Energy => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "žádný",
            Self::Dopamine => "dopamin",
            Self::Hurt => "poškození",
            Self::Heal => "léčení",
            Self::Energy => "energie",
        }
    }

    pub fn tint(self) -> (f32, f32, f32) {
        match self {
            Self::None => (0.2, 0.35, 0.4),
            Self::Dopamine => (0.55, 0.85, 0.35),
            Self::Hurt => (0.95, 0.3, 0.35),
            Self::Heal => (0.35, 0.85, 0.9),
            Self::Energy => (0.95, 0.75, 0.3),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EdgeZone {
    pub effect: EdgeEffect,
    pub reach: f32,
}

impl Default for EdgeZone {
    fn default() -> Self {
        Self {
            effect: EdgeEffect::None,
            reach: 0.18,
        }
    }
}

/// Exact simulation state for save/load (no VFX).
#[derive(Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub version: u32,
    pub seed: u64,
    pub time: f32,
    pub organisms: Vec<Organism>,
    pub dishes: Vec<PetriDish>,
    pub tubes: Vec<Tube>,
    pub next_id: u64,
    pub next_dish_id: u32,
    pub steps: u32,
    pub survival: Survival,
    pub food_kinds: [FoodSpec; FOOD_KIND_COUNT],
    pub food_timers: [f32; FOOD_KIND_COUNT],
    pub births: u64,
    pub deaths: u64,
}

pub struct World {
    time: f32,
    dishes: Vec<PetriDish>,
    tubes: Vec<Tube>,
    next_dish_id: u32,
    organisms: Vec<Organism>,
    sparks: Vec<Spark>,
    flashes: Vec<Flash>,
    rng: StdRng,
    next_id: u64,
    steps: u32,
    survival: Survival,
    food_kinds: [FoodSpec; FOOD_KIND_COUNT],
    food_timers: [f32; FOOD_KIND_COUNT],
    births: u64,
    deaths: u64,
    // Hot-loop scratch (not saved).
    scratch_centers: Vec<Vec2>,
    scratch_radii: Vec<f32>,
    scratch_push: Vec<Vec2>,
    scratch_touch: Vec<[f32; 4]>,
    spatial: SpatialHash,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self::new_with(seed, 16, 12)
    }

    pub fn new_with(seed: u64, population: usize, start_food: usize) -> Self {
        let mut world = Self {
            time: 0.0,
            dishes: vec![PetriDish::new(0, Vec2::ZERO)],
            tubes: Vec::new(),
            next_dish_id: 1,
            organisms: Vec::new(),
            sparks: Vec::new(),
            flashes: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
            next_id: 1,
            steps: 0,
            survival: Survival::Live,
            food_kinds: FoodSpec::defaults(),
            food_timers: [0.0; FOOD_KIND_COUNT],
            births: 0,
            deaths: 0,
            scratch_centers: Vec::new(),
            scratch_radii: Vec::new(),
            scratch_push: Vec::new(),
            scratch_touch: Vec::new(),
            spatial: SpatialHash::default(),
        };
        let pop = population.min(MAX_POP);
        let meals = start_food.min(FOOD_CAP);
        for _ in 0..pop {
            let p = world.random_point(0.82);
            world.spawn_at(p);
        }
        for i in 0..meals {
            let p = world.random_dish_point();
            world.drop_food_kind(p, starter_food_kind(i));
        }
        world.births = 0;
        world
    }

    fn primary(&self) -> &PetriDish {
        &self.dishes[0]
    }

    fn primary_mut(&mut self) -> &mut PetriDish {
        &mut self.dishes[0]
    }

    pub fn set_dish_bounds(&mut self, half_x: f32, half_y: f32) {
        self.primary_mut().set_bounds(half_x, half_y);
    }

    #[cfg(test)]
    fn foods_mut(&mut self) -> &mut Vec<Food> {
        &mut self.primary_mut().foods
    }

    fn dish_index(&self, id: u32) -> Option<usize> {
        self.dishes.iter().position(|d| d.id == id)
    }

    pub fn dishes(&self) -> &[PetriDish] {
        &self.dishes
    }

    pub fn primary_dish_id(&self) -> u32 {
        self.primary().id
    }

    pub fn tubes(&self) -> &[Tube] {
        &self.tubes
    }

    /// Half-extents from the table origin that cover every dish (camera clamp / fit).
    pub fn table_bounds(&self) -> (f32, f32) {
        let mut mx = 0.45f32;
        let mut my = 0.45f32;
        for d in &self.dishes {
            mx = mx.max(d.pos.x.abs() + d.half_x);
            my = my.max(d.pos.y.abs() + d.half_y);
        }
        (mx, my)
    }

    /// Whether a new dish with the primary size can sit at `pos` without overlapping.
    pub fn can_place_dish(&self, pos: Vec2) -> bool {
        if self.dishes.len() >= MAX_DISHES {
            return false;
        }
        let (hx, hy) = self.bounds();
        let gap = 0.14;
        for d in &self.dishes {
            let dx = (d.pos.x - pos.x).abs();
            let dy = (d.pos.y - pos.y).abs();
            if dx < d.half_x + hx + gap && dy < d.half_y + hy + gap {
                return false;
            }
        }
        true
    }

    /// Place a dish matching the primary size. Returns `None` if capped or overlapping.
    pub fn try_add_dish(&mut self, pos: Vec2) -> Option<u32> {
        if !self.can_place_dish(pos) {
            return None;
        }
        let (hx, hy) = self.bounds();
        Some(self.add_dish(pos, hx, hy))
    }

    /// Add a second (or further) dish on the table and return its id.
    pub fn add_dish(&mut self, pos: Vec2, half_x: f32, half_y: f32) -> u32 {
        let id = self.next_dish_id;
        self.next_dish_id = self.next_dish_id.saturating_add(1);
        let mut dish = PetriDish::new(id, pos);
        dish.set_bounds(half_x, half_y);
        self.dishes.push(dish);
        id
    }

    /// Link two dish rim ports with a tube. Returns false if either dish is missing.
    pub fn add_tube(
        &mut self,
        a_dish: u32,
        a_side: u8,
        a_along: f32,
        b_dish: u32,
        b_side: u8,
        b_along: f32,
    ) -> bool {
        if self.dish_index(a_dish).is_none() || self.dish_index(b_dish).is_none() {
            return false;
        }
        if a_dish == b_dish {
            return false;
        }
        self.tubes
            .push(Tube::new(a_dish, a_side, a_along, b_dish, b_side, b_along));
        true
    }

    /// Fixed dish extents in world units (independent of window size / aspect).
    pub fn set_bounds(&mut self, half_x: f32, half_y: f32) {
        self.primary_mut().set_bounds(half_x, half_y);
    }

    /// Grow one side of the dish, keeping existing contents fixed in absolute space.
    /// `side`: 0 left (−x), 1 right (+x), 2 bottom (−y), 3 top (+y).
    pub fn expand_side(&mut self, side: u8, delta: f32) {
        let d = delta.clamp(0.08, 0.45);
        let max_h = 4.0;
        let (nx, ny, dhx, dhy) = {
            let dish = self.primary();
            match side {
                0 if dish.half_x + d * 0.5 <= max_h => (d * 0.5, 0.0, d * 0.5, 0.0),
                1 if dish.half_x + d * 0.5 <= max_h => (-d * 0.5, 0.0, d * 0.5, 0.0),
                2 if dish.half_y + d * 0.5 <= max_h => (0.0, d * 0.5, 0.0, d * 0.5),
                3 if dish.half_y + d * 0.5 <= max_h => (0.0, -d * 0.5, 0.0, d * 0.5),
                _ => return,
            }
        };
        self.nudge_contents(0, nx, ny);
        let dish = self.primary_mut();
        dish.half_x = (dish.half_x + dhx).min(max_h);
        dish.half_y = (dish.half_y + dhy).min(max_h);
        dish.fields.set_bounds(dish.half_x, dish.half_y);
    }

    fn nudge_contents(&mut self, dish_id: u32, dx: f32, dy: f32) {
        if dx.abs() < 1e-8 && dy.abs() < 1e-8 {
            return;
        }
        let delta = Vec2::new(dx, dy);
        for org in &mut self.organisms {
            if org.dish_id != dish_id {
                continue;
            }
            for n in &mut org.nodes {
                *n += delta;
            }
        }
        if let Some(di) = self.dish_index(dish_id) {
            for food in &mut self.dishes[di].foods {
                food.pos += delta;
            }
        }
        // Sparks/flashes are table-space; offset when primary dish at origin expands.
        if dish_id == self.primary().id && self.primary().pos.length() < 1e-6 {
            for spark in &mut self.sparks {
                spark.pos += delta;
            }
            for flash in &mut self.flashes {
                flash.pos += delta;
            }
        }
    }

    pub fn bounds(&self) -> (f32, f32) {
        (self.primary().half_x, self.primary().half_y)
    }

    /// Cells along the short side of the dish. The field is resampled in place.
    pub fn set_resolution(&mut self, cells: usize) {
        let dish = self.primary_mut();
        dish.fields.set_cells(cells);
        dish.fields.set_bounds(dish.half_x, dish.half_y);
    }

    pub fn resolution(&self) -> usize {
        self.primary().fields.cells()
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn alive(&self) -> usize {
        self.organisms.len()
    }

    pub fn food_count(&self) -> usize {
        self.dishes.iter().map(|d| d.foods.len()).sum()
    }

    pub fn foods(&self) -> &[Food] {
        &self.primary().foods
    }

    /// All meals across dishes as `(dish_id, table_pos, food)`.
    pub fn foods_table(&self) -> Vec<(u32, Vec2, Food)> {
        let mut out = Vec::new();
        for dish in &self.dishes {
            for food in &dish.foods {
                out.push((dish.id, dish.to_table(food.pos), food.clone()));
            }
        }
        out
    }

    pub fn food_kinds(&self) -> &[FoodSpec; FOOD_KIND_COUNT] {
        &self.food_kinds
    }

    pub fn food_spec(&self, kind: FoodKind) -> FoodSpec {
        self.food_kinds[kind.index()]
    }

    pub fn set_food_spec(&mut self, kind: FoodKind, spec: FoodSpec) {
        let mut s = spec;
        s.sense = s.sense.clamp(0.12, 1.2);
        s.energy = s.energy.clamp(0.05, 1.4);
        s.harm = s.harm.clamp(0.0, 1.0);
        s.batch = s.batch.clamp(0, 16);
        s.interval = s.interval.clamp(1.0, 60.0);
        self.food_kinds[kind.index()] = s;
    }

    pub fn set_food_sense(&mut self, kind: FoodKind, radius: f32) {
        let mut s = self.food_kinds[kind.index()];
        s.sense = radius.clamp(0.12, 1.2);
        self.food_kinds[kind.index()] = s;
    }

    pub fn set_food_energy(&mut self, kind: FoodKind, energy: f32) {
        let mut s = self.food_kinds[kind.index()];
        s.energy = energy.clamp(0.05, 1.4);
        self.food_kinds[kind.index()] = s;
    }

    pub fn set_food_harm(&mut self, kind: FoodKind, harm: f32) {
        let mut s = self.food_kinds[kind.index()];
        s.harm = harm.clamp(0.0, 1.0);
        self.food_kinds[kind.index()] = s;
    }

    pub fn set_food_kind_auto(&mut self, kind: FoodKind, on: bool) {
        self.food_kinds[kind.index()].auto = on;
    }

    pub fn set_food_kind_batch(&mut self, kind: FoodKind, n: usize) {
        self.food_kinds[kind.index()].batch = n.clamp(0, 16);
    }

    pub fn set_food_kind_interval(&mut self, kind: FoodKind, secs: f32) {
        self.food_kinds[kind.index()].interval = secs.clamp(1.0, 60.0);
    }

    /// Largest smell radius among kinds (census / legacy display).
    pub fn food_sense_radius(&self) -> f32 {
        self.food_kinds
            .iter()
            .map(|k| k.sense)
            .fold(0.0_f32, f32::max)
    }

    pub fn food_sensor_reach(&self) -> f32 {
        FOOD_SENSOR_REACH
    }

    pub fn food_auto(&self) -> bool {
        self.dishes
            .iter()
            .flat_map(|d| d.feeders.iter())
            .any(|f| f.enabled && f.batch > 0)
    }

    pub fn set_food_auto(&mut self, on: bool) {
        for d in &mut self.dishes {
            for f in &mut d.feeders {
                f.enabled = on;
            }
        }
    }

    pub fn feeders(&self) -> &[Feeder] {
        &self.primary().feeders
    }

    pub fn feeder_count(&self) -> usize {
        self.dishes.iter().map(|d| d.feeders.len()).sum()
    }

    pub fn active_feeder_count(&self) -> usize {
        self.dishes
            .iter()
            .flat_map(|d| d.feeders.iter())
            .filter(|f| f.enabled)
            .count()
    }

    pub fn mutate_organism(&mut self, id: u64) -> bool {
        if let Some(o) = self.organisms.iter_mut().find(|o| o.id == id) {
            o.mutate(&mut self.rng);
            true
        } else {
            false
        }
    }

    /// Project a world point onto the nearest dish rim. Returns (side, along, rim_pos, distance).
    pub fn project_rim(&self, p: Vec2) -> (u8, f32, Vec2, f32) {
        let dish = self.primary();
        let hx = dish.half_x;
        let hy = dish.half_y;
        let candidates: [(u8, Vec2, f32); 4] = [
            (
                0,
                Vec2::new(-hx, p.y.clamp(-hy, hy)),
                ((p.y + hy) / (2.0 * hy).max(1e-4)).clamp(0.0, 1.0),
            ),
            (
                1,
                Vec2::new(hx, p.y.clamp(-hy, hy)),
                ((p.y + hy) / (2.0 * hy).max(1e-4)).clamp(0.0, 1.0),
            ),
            (
                2,
                Vec2::new(p.x.clamp(-hx, hx), -hy),
                ((p.x + hx) / (2.0 * hx).max(1e-4)).clamp(0.0, 1.0),
            ),
            (
                3,
                Vec2::new(p.x.clamp(-hx, hx), hy),
                ((p.x + hx) / (2.0 * hx).max(1e-4)).clamp(0.0, 1.0),
            ),
        ];
        let mut best = candidates[0];
        let mut best_d = (p - candidates[0].1).length();
        for c in &candidates[1..] {
            let d = (p - c.1).length();
            if d < best_d {
                best_d = d;
                best = *c;
            }
        }
        (best.0, best.2, best.1, best_d)
    }

    pub fn feeder_rim_pos(&self, feeder: &Feeder) -> Vec2 {
        self.primary().rim_pos(feeder.side, feeder.along)
    }

    pub fn feeder_outward(side: u8) -> Vec2 {
        match side {
            0 => Vec2::new(-1.0, 0.0),
            1 => Vec2::new(1.0, 0.0),
            2 => Vec2::new(0.0, -1.0),
            _ => Vec2::new(0.0, 1.0),
        }
    }

    pub fn feeder_inward(side: u8) -> Vec2 {
        Self::feeder_outward(side) * -1.0
    }

    pub fn add_feeder(&mut self, pos: Vec2, kind: FoodKind) -> usize {
        let f = Feeder::new(pos, kind);
        let feeders = &mut self.primary_mut().feeders;
        feeders.push(f);
        feeders.len() - 1
    }

    pub fn remove_feeder(&mut self, index: usize) -> bool {
        let feeders = &mut self.primary_mut().feeders;
        if index < feeders.len() {
            feeders.remove(index);
            true
        } else {
            false
        }
    }

    pub fn set_feeder_enabled(&mut self, index: usize, on: bool) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            f.enabled = on;
        }
    }

    pub fn set_feeder_kind(&mut self, index: usize, kind: FoodKind) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            f.kind = kind;
        }
    }

    pub fn set_feeder_rate(&mut self, index: usize, rate: f32) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            f.rate = rate.clamp(0.1, 20.0);
        }
    }

    pub fn set_feeder_radius(&mut self, index: usize, radius: f32) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            f.radius = radius.clamp(0.05, 3.0);
        }
    }

    pub fn set_feeder_interval(&mut self, index: usize, secs: f32) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            let s = secs.clamp(0.1, 60.0);
            f.rate = 1.0 / s;
            f.interval = s;
        }
    }

    pub fn set_feeder_batch(&mut self, index: usize, n: usize) {
        if let Some(f) = self.primary_mut().feeders.get_mut(index) {
            f.batch = n.clamp(1, 16);
        }
    }

    /// Pick feeder near a world point.
    pub fn pick_feeder(&self, p: Vec2, max_dist: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, f) in self.primary().feeders.iter().enumerate() {
            let d = (p - f.pos).length();
            if d <= max_dist && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn viscosity(&self) -> f32 {
        self.primary().viscosity
    }

    pub fn set_viscosity(&mut self, v: f32) {
        self.primary_mut().viscosity = v.clamp(0.25, 4.0);
    }

    pub fn edges(&self) -> [EdgeZone; 4] {
        self.primary().edges
    }

    pub fn cycle_edge_effect(&mut self, side: usize) {
        if let Some(e) = self.primary_mut().edges.get_mut(side) {
            e.effect = e.effect.next();
        }
    }

    pub fn set_edge_reach(&mut self, side: usize, reach: f32) {
        if let Some(e) = self.primary_mut().edges.get_mut(side) {
            e.reach = reach.clamp(0.05, 0.85);
        }
    }

    pub fn set_edges(&mut self, edges: [EdgeZone; 4]) {
        self.primary_mut().edges = edges.map(|mut e| {
            e.reach = e.reach.clamp(0.05, 0.85);
            e
        });
    }

    pub fn sparks(&self) -> &[Spark] {
        &self.sparks
    }

    pub fn flashes(&self) -> &[Flash] {
        &self.flashes
    }

    pub fn to_snapshot(&self) -> WorldSnapshot {
        let seed = self
            .steps
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(self.next_id as u32)
            as u64
            ^ (self.time.to_bits() as u64)
            ^ self.births.wrapping_mul(0xC2B2_AE3D);
        WorldSnapshot {
            version: 2,
            seed,
            time: self.time,
            organisms: self.organisms.clone(),
            dishes: self.dishes.clone(),
            tubes: self.tubes.clone(),
            next_id: self.next_id,
            next_dish_id: self.next_dish_id,
            steps: self.steps,
            survival: self.survival,
            food_kinds: self.food_kinds,
            food_timers: self.food_timers,
            births: self.births,
            deaths: self.deaths,
        }
    }

    pub fn from_snapshot(snap: WorldSnapshot) -> Self {
        let mut dishes = snap.dishes;
        if dishes.is_empty() {
            dishes.push(PetriDish::new(0, Vec2::ZERO));
        }
        for d in &mut dishes {
            d.viscosity = d.viscosity.clamp(0.35, 2.5);
            d.edges = d.edges.map(|mut e| {
                e.reach = e.reach.clamp(0.05, 0.85);
                e
            });
            d.fields.set_bounds(d.half_x.max(0.25), d.half_y.max(0.25));
        }
        let next_dish_id = snap
            .next_dish_id
            .max(dishes.iter().map(|d| d.id + 1).max().unwrap_or(1));
        Self {
            time: snap.time,
            dishes,
            tubes: snap.tubes,
            next_dish_id,
            organisms: snap.organisms,
            sparks: Vec::new(),
            flashes: Vec::new(),
            rng: StdRng::seed_from_u64(snap.seed),
            next_id: snap.next_id.max(1),
            steps: snap.steps,
            survival: snap.survival,
            food_kinds: snap.food_kinds,
            food_timers: snap.food_timers,
            births: snap.births,
            deaths: snap.deaths,
            scratch_centers: Vec::new(),
            scratch_radii: Vec::new(),
            scratch_push: Vec::new(),
            scratch_touch: Vec::new(),
            spatial: SpatialHash::default(),
        }
    }

    pub fn appearances(&self) -> Vec<Appearance<'_>> {
        self.organisms
            .iter()
            .map(|o| {
                let dish_pos = self
                    .dish_index(o.dish_id)
                    .map(|i| self.dishes[i].pos)
                    .unwrap_or(Vec2::ZERO);
                Appearance {
                    id: o.id,
                    dish_pos,
                    nodes: &o.nodes,
                    radius: o.radius(),
                    node_radius: o.node_radius(),
                    hue: o.genome.morph.hue,
                    thrust: o.actuators.thrust,
                    energy: o.energy,
                    mouth: o.actuators.mouth,
                    generation: o.genome.generation,
                }
            })
            .collect()
    }

    pub fn census(&self) -> Census {
        let n = self.organisms.len();
        if n == 0 {
            return Census {
                alive: 0,
                food: self.food_count(),
                mean_energy: 0.0,
                max_generation: 0,
                mean_neurons: 0.0,
                mean_synapses: 0.0,
                mean_age: 0.0,
                mean_hunger: 0.0,
                mean_mass: 0.0,
                hungry: 0,
                time: self.time,
                births: self.births,
                deaths: self.deaths,
                feeders: self.feeder_count(),
                food_sense: self.food_sense_radius(),
            };
        }
        let mut energy = 0.0;
        let mut neurons = 0.0;
        let mut synapses = 0.0;
        let mut age = 0.0;
        let mut hunger = 0.0;
        let mut mass = 0.0;
        let mut hungry = 0;
        let mut generation = 0;
        for o in &self.organisms {
            energy += o.energy;
            neurons += o.brain.neuron_count() as f32;
            synapses += o.brain.synapse_count() as f32;
            age += o.age;
            let h = o.hunger();
            hunger += h;
            mass += o.mass();
            if h > 0.45 {
                hungry += 1;
            }
            generation = generation.max(o.genome.generation);
        }
        let k = n as f32;
        Census {
            alive: n,
            food: self.food_count(),
            mean_energy: energy / k,
            max_generation: generation,
            mean_neurons: neurons / k,
            mean_synapses: synapses / k,
            mean_age: age / k,
            mean_hunger: hunger / k,
            mean_mass: mass / k,
            hungry,
            time: self.time,
            births: self.births,
            deaths: self.deaths,
            feeders: self.feeder_count(),
            food_sense: self.food_sense_radius(),
        }
    }

    pub fn network(&self, id: u64) -> Option<Net> {
        let o = self.organisms.iter().find(|o| o.id == id)?;
        let kind = o.brain.kinds();
        let labels = o.brain.labels();
        let act = (0..kind.len()).map(|i| o.brain.activation(i)).collect();
        let wires = o
            .brain
            .wires()
            .into_iter()
            .map(|(from, to, weight)| Wire { from, to, weight })
            .collect();
        Some(Net {
            kind,
            act,
            wires,
            labels,
        })
    }

    /// Refresh activations on a cached [`Net`] when topology is unchanged.
    pub fn refresh_net_acts(&self, id: u64, net: &mut Net) -> bool {
        let Some(o) = self.organisms.iter().find(|o| o.id == id) else {
            return false;
        };
        if net.act.len() != o.brain.neuron_count() {
            return false;
        }
        for (i, slot) in net.act.iter_mut().enumerate() {
            *slot = o.brain.activation(i);
        }
        true
    }

    /// Spawn up to `n` organisms at random dish points. Returns how many were created.
    pub fn spawn_boot_organisms(&mut self, n: usize) -> usize {
        let room = MAX_POP.saturating_sub(self.organisms.len());
        let n = n.min(room);
        let mut made = 0;
        for _ in 0..n {
            let p = self.random_point(0.82);
            self.spawn_at(p);
            made += 1;
        }
        made
    }

    /// Drop up to `n` starter food items. `kind_offset` cycles food kinds.
    pub fn spawn_boot_food(&mut self, n: usize, kind_offset: usize) -> usize {
        let room = FOOD_CAP.saturating_sub(self.food_count());
        let n = n.min(room);
        let mut made = 0;
        for i in 0..n {
            let p = self.random_dish_point();
            self.drop_food_kind(p, FoodKind::from_index(kind_offset + i));
            made += 1;
        }
        made
    }

    pub fn stats(&self, id: u64) -> Option<Stats> {
        let o = self.organisms.iter().find(|o| o.id == id)?;
        Some(Stats {
            id: o.id,
            hue: o.genome.morph.hue,
            energy: o.energy,
            hunger: o.hunger(),
            pain: o.damage.clamp(0.0, 1.0),
            novelty: o.novelty_excess.clamp(0.0, 1.0),
            repro: o.repro_need(),
            pulse: o.pulse,
            heart: o.heart(),
            tonic: o.tonic,
            age: o.age,
            generation: o.genome.generation,
            neurons: o.brain.neuron_count(),
            synapses: o.brain.synapse_count(),
            nodes: o.genome.morph.nodes,
            mass: o.genome.morph.mass,
            body: o.genome.morph.radius,
            hetero: o.genome.morph.hetero,
            setpoint: o.genome.morph.setpoint,
            learn: o.genome.morph.learn,
            hunger_w: o.genome.morph.hunger_w,
            pain_w: o.genome.morph.pain_w,
            novelty_w: o.genome.morph.novelty_w,
            birth_w: o.genome.morph.birth_w,
            maturity: o.genome.morph.maturity,
            root_nodes: o.genome.root_morph.nodes,
            root_mass: o.genome.root_morph.mass,
            root_body: o.genome.root_morph.radius,
            root_hue: o.genome.root_morph.hue,
            root_hetero: o.genome.root_morph.hetero,
            root_setpoint: o.genome.root_morph.setpoint,
            root_learn: o.genome.root_morph.learn,
            root_hunger_w: o.genome.root_morph.hunger_w,
            root_pain_w: o.genome.root_morph.pain_w,
            root_novelty_w: o.genome.root_morph.novelty_w,
            root_birth_w: o.genome.root_morph.birth_w,
            root_maturity: o.genome.root_morph.maturity,
            gene_drift: o.genome.drift_from_root(),
            food_taste: o.food_taste,
        })
    }

    pub fn pick(&self, p: Vec2) -> Option<u64> {
        self.organisms
            .iter()
            .filter_map(|o| {
                let dish_pos = self
                    .dish_index(o.dish_id)
                    .map(|i| self.dishes[i].pos)
                    .unwrap_or(Vec2::ZERO);
                let pad = o.node_radius() * 2.4;
                let best = o
                    .nodes
                    .iter()
                    .map(|n| (*n + dish_pos - p).length())
                    .fold(f32::INFINITY, f32::min);
                let reach = o.radius() * 1.35 + pad;
                if best <= reach {
                    Some((o.id, best))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
    }

    pub fn drop_food(&mut self, pos: Vec2) {
        self.drop_food_kind(pos, FoodKind::Green);
    }

    pub fn drop_food_kind(&mut self, pos: Vec2, kind: FoodKind) {
        let dish = self.primary_mut();
        if dish.foods.len() >= FOOD_CAP {
            return;
        }
        let pos = dish.clamp_local(pos, 0.06);
        dish.foods.push(Food::fresh(pos, kind));
    }

    pub fn spawn_at(&mut self, pos: Vec2) -> u64 {
        let dish_id = self.primary().id;
        self.spawn_in_dish(dish_id, pos).expect("primary dish")
    }

    /// Spawn one organism into a specific dish (local coordinates).
    pub fn spawn_in_dish(&mut self, dish_id: u32, local: Vec2) -> Option<u64> {
        let di = self.dish_index(dish_id)?;
        let pos = self.dishes[di].clamp_local(local, 0.1);
        let genome = Genome::random(&mut self.rng);
        let axis = random_axis(&mut self.rng);
        let id = self.alloc_id();
        let mut org = Organism::born(id, genome, pos, axis, START_ENERGY);
        org.dish_id = dish_id;
        self.organisms.push(org);
        self.births += 1;
        Some(id)
    }

    pub fn dish_organism_count(&self, dish_id: u32) -> usize {
        self.organisms
            .iter()
            .filter(|o| o.dish_id == dish_id)
            .count()
    }

    pub fn dish_is_empty(&self, dish_id: u32) -> bool {
        self.dish_organism_count(dish_id) == 0
    }

    /// Seed an empty (or cleared) dish with starting life and food.
    pub fn seed_dish(
        &mut self,
        dish_id: u32,
        population: usize,
        start_food: usize,
        half_x: f32,
        half_y: f32,
    ) -> bool {
        let Some(di) = self.dish_index(dish_id) else {
            return false;
        };
        self.dishes[di].set_bounds(half_x, half_y);
        // Clear residual food/organisms in this dish before seeding.
        self.organisms.retain(|o| o.dish_id != dish_id);
        self.dishes[di].foods.clear();
        let pop = population.min(MAX_POP);
        let meals = start_food.min(FOOD_CAP);
        let births_before = self.births;
        for _ in 0..pop {
            let p = self.random_point_in(dish_id, 0.82).unwrap_or(Vec2::ZERO);
            let _ = self.spawn_in_dish(dish_id, p);
        }
        for i in 0..meals {
            if let Some(p) = self.random_point_in(dish_id, 0.9) {
                if let Some(di) = self.dish_index(dish_id) {
                    let pos = self.dishes[di].clamp_local(p, 0.06);
                    self.dishes[di]
                        .foods
                        .push(Food::fresh(pos, FoodKind::from_index(i)));
                }
            }
        }
        self.births = births_before;
        true
    }

    pub fn fill_field_rgba(&self, width: usize, height: usize, out: &mut [u8]) {
        self.primary().fields.fill_rgba(width, height, out);
    }

    pub fn step(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, DT_CAP);
        if dt == 0.0 {
            return;
        }
        self.tick_feeders(dt);
        self.tick_food(dt);
        let dish_ids: Vec<u32> = self.dishes.iter().map(|d| d.id).collect();
        for did in dish_ids {
            self.separate_dish(did);
        }
        self.think_all(dt);
        for i in 0..self.organisms.len() {
            let did = self.organisms[i].dish_id;
            let Some(di) = self.dish_index(did) else {
                continue;
            };
            let (hx, hy, vis) = {
                let d = &self.dishes[di];
                (d.half_x, d.half_y, d.viscosity)
            };
            self.organisms[i].integrate(dt, hx, hy, vis);
        }
        self.exchange(dt);
        self.apply_edge_zones(dt);
        for org in &mut self.organisms {
            org.grow(dt);
        }
        self.births();
        for org in &mut self.organisms {
            org.reward(dt);
        }
        self.deaths();
        for dish in &mut self.dishes {
            dish.tick_fields(dt);
        }
        self.transfer_tubes();
        self.fade_sparks(dt);
        self.fade_flashes(dt);
        self.time += dt;
        self.steps += 1;
        if self.steps >= GENERATION_STEPS {
            self.steps = 0;
            self.select_generation();
        }
    }

    fn select_generation(&mut self) {
        if self.survival == Survival::Live {
            return;
        }
        let zone = self.survival;
        let time = self.time;
        let dishes = &self.dishes;
        self.organisms.retain(|org| {
            let Some(di) = dishes.iter().position(|d| d.id == org.dish_id) else {
                return false;
            };
            let d = &dishes[di];
            survives(zone, org.centroid(), d.half_x, d.half_y, time)
        });
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn random_point(&mut self, extent: f32) -> Vec2 {
        self.random_point_in(self.primary().id, extent)
            .unwrap_or(Vec2::ZERO)
    }

    fn random_point_in(&mut self, dish_id: u32, extent: f32) -> Option<Vec2> {
        let di = self.dish_index(dish_id)?;
        let dish = &self.dishes[di];
        let x = (dish.half_x * extent).clamp(0.05, (dish.half_x - 0.05).max(0.05));
        let y = (dish.half_y * extent).clamp(0.05, (dish.half_y - 0.05).max(0.05));
        Some(Vec2::new(
            self.rng.gen_range(-x..x),
            self.rng.gen_range(-y..y),
        ))
    }

    /// Uniform point across the whole dish (with a small rim inset).
    fn random_dish_point(&mut self) -> Vec2 {
        self.random_point_in(self.primary().id, 0.93)
            .unwrap_or(Vec2::ZERO)
    }

    fn clamp_inside(&self, p: Vec2, inset: f32) -> Vec2 {
        self.primary().clamp_local(p, inset)
    }

    fn tick_feeders(&mut self, dt: f32) {
        for di in 0..self.dishes.len() {
            let mut pumps: Vec<(usize, FoodKind, usize)> = Vec::new();
            {
                let dish = &mut self.dishes[di];
                if dish.feeders.is_empty() {
                    continue;
                }
                for (i, feeder) in dish.feeders.iter_mut().enumerate() {
                    if !feeder.enabled || feeder.rate <= 0.0 {
                        continue;
                    }
                    let interval = (1.0 / feeder.rate).max(0.02);
                    feeder.timer += dt;
                    let mut n = 0usize;
                    while feeder.timer >= interval {
                        feeder.timer -= interval;
                        n += 1;
                        if n > 20 {
                            feeder.timer = 0.0;
                            break;
                        }
                    }
                    if n > 0 {
                        pumps.push((i, feeder.kind, n));
                    }
                }
            }
            for (idx, kind, count) in pumps {
                let (feeder_pos, radius) = {
                    let f = &self.dishes[di].feeders[idx];
                    (f.pos, f.radius)
                };
                for _ in 0..count {
                    if self.dishes[di].foods.len() >= FOOD_CAP {
                        break;
                    }
                    let angle = self.rng.gen_range(0.0..std::f32::consts::TAU);
                    let r = self.rng.gen_range(0.0..radius.max(0.02));
                    let offset = Vec2::new(angle.cos() * r, angle.sin() * r);
                    let pos = self.dishes[di].clamp_local(feeder_pos + offset, 0.04);
                    self.dishes[di].foods.push(Food::fresh(pos, kind));
                }
            }
        }
    }

    fn tick_food(&mut self, dt: f32) {
        const BLOOM_SECS: f32 = 1.25;
        const FADE_SECS: f32 = 0.9;
        for dish in &mut self.dishes {
            for food in &mut dish.foods {
                if let Some(fade) = food.fade.as_mut() {
                    *fade = (*fade + dt / FADE_SECS).min(1.0);
                } else if food.bloom < 1.0 {
                    food.bloom = (food.bloom + dt / BLOOM_SECS).min(1.0);
                }
            }
            dish.foods
                .retain(|f| f.fade.map(|u| u < 0.999).unwrap_or(true));
        }
    }

    fn separate_dish(&mut self, dish_id: u32) {
        let members: Vec<usize> = self
            .organisms
            .iter()
            .enumerate()
            .filter(|(_, o)| o.dish_id == dish_id)
            .map(|(i, _)| i)
            .collect();
        let n = members.len();
        if n < 2 {
            return;
        }
        let Some(di) = self.dish_index(dish_id) else {
            return;
        };
        let (hx, hy) = (self.dishes[di].half_x, self.dishes[di].half_y);

        self.scratch_centers.clear();
        self.scratch_radii.clear();
        for &i in &members {
            self.scratch_centers.push(self.organisms[i].centroid());
            self.scratch_radii.push(self.organisms[i].radius());
        }
        self.scratch_push.clear();
        self.scratch_push.resize(n, Vec2::ZERO);
        self.scratch_touch.clear();
        self.scratch_touch.resize(n, [0.0; 4]);

        let cell = self
            .scratch_radii
            .iter()
            .copied()
            .fold(0.12_f32, f32::max)
            * 2.2;
        self.spatial
            .clear_and_build(hx, hy, cell, &self.scratch_centers);

        let centers = &self.scratch_centers;
        let radii = &self.scratch_radii;
        let mut push = std::mem::take(&mut self.scratch_push);
        let mut touch = std::mem::take(&mut self.scratch_touch);
        self.spatial.for_each_pair(|a, b| {
            let delta = centers[b] - centers[a];
            let dist = delta.length().max(1e-4);
            let min = radii[a] + radii[b];
            if dist >= min {
                return;
            }
            let nrm = delta * (1.0 / dist);
            let pen = (min - dist) * 0.5;
            push[a] -= nrm * pen;
            push[b] += nrm * pen;
            add_touch(&self.organisms[members[a]], nrm, pen, &mut touch[a]);
            add_touch(
                &self.organisms[members[b]],
                nrm * -1.0,
                pen,
                &mut touch[b],
            );
        });
        self.scratch_push = push;
        self.scratch_touch = touch;

        for (local, &oi) in members.iter().enumerate() {
            self.organisms[oi].shift(self.scratch_push[local]);
            self.organisms[oi].touch = self.scratch_touch[local];
        }
    }

    fn think_all(&mut self, dt: f32) {
        let near: Vec<Near> = self
            .organisms
            .iter()
            .map(|o| Near {
                pos: o.centroid(),
                hue: o.genome.morph.hue,
                dish_id: o.dish_id,
            })
            .collect();
        let crowd = crowd(&self.organisms);
        let n = self.organisms.len();
        for i in 0..n {
            let dish_id = self.organisms[i].dish_id;
            let Some(di) = self.dish_index(dish_id) else {
                continue;
            };
            let inputs = {
                let org = &self.organisms[i];
                let head = org.head();
                let tail = org.tail();
                let axis = org.axis();
                let left = axis.rotate(0.6);
                let right = axis.rotate(-0.6);
                let kin = [
                    cone_kin(head, left, org.genome.morph.hue, dish_id, &near),
                    cone_kin(head, axis, org.genome.morph.hue, dish_id, &near),
                    cone_kin(head, right, org.genome.morph.hue, dish_id, &near),
                ];
                let (density, grad_fwd, grad_side, similar) = crowd[i];
                let taste = self.organisms[i].food_taste;
                let foods = &self.dishes[di].foods;
                let (smell, aim, kind_smells) = food_pull(foods, &self.food_kinds, head, &taste);
                let dish = &self.dishes[di];
                let side = axis.perp();
                let inputs = lay_inputs(
                    org,
                    &Senses {
                        half_x: dish.half_x,
                        half_y: dish.half_y,
                        food_kinds: kind_smells,
                        food_tail: sense_food(foods, &self.food_kinds, tail, &taste),
                        food_fwd: aim.dot(axis),
                        food_side: aim.dot(side),
                        pheromone: dish.fields.sample(Channel::Signal, head),
                        kin,
                        similar,
                        density,
                        grad_fwd,
                        grad_side,
                    },
                );
                let _ = smell;
                inputs
            };
            self.organisms[i].think(&inputs, dt);
        }
    }

    fn exchange(&mut self, dt: f32) {
        self.consume_food();
        let n = self.organisms.len();
        for i in 0..n {
            let head = self.organisms[i].head();
            let dish_id = self.organisms[i].dish_id;
            let emit = self.organisms[i].actuators.signal * 0.4 * dt;
            if emit > 0.0 {
                if let Some(di) = self.dish_index(dish_id) {
                    self.dishes[di]
                        .fields
                        .add_blob(Channel::Signal, head, 0.055, emit);
                }
            }
            self.organisms[i].metabolize(dt);
        }
        let dish_ids: Vec<u32> = self.dishes.iter().map(|d| d.id).collect();
        for did in dish_ids {
            self.bites_dish(did, dt);
        }
    }

    /// The detection circle only reports food. The particle is eaten whole when the head touches it.
    fn consume_food(&mut self) {
        let mut taken: Vec<(usize, usize, usize)> = Vec::new(); // org, dish_idx, food_idx
        for (oi, org) in self.organisms.iter().enumerate() {
            if org.actuators.mouth < MOUTH_OPEN {
                continue;
            }
            let Some(di) = self.dish_index(org.dish_id) else {
                continue;
            };
            let head = org.head();
            let reach = FOOD_BITE.max(org.radius() * 0.65);
            for (fi, food) in self.dishes[di].foods.iter().enumerate() {
                if !food.alive()
                    || taken
                        .iter()
                        .any(|&(_, d, used)| d == di && used == fi)
                {
                    continue;
                }
                if (food.pos - head).length() <= reach {
                    taken.push((oi, di, fi));
                    break;
                }
            }
        }
        for (oi, di, fi) in taken {
            let kind = self.dishes[di].foods[fi].kind;
            let spec = self.food_kinds[kind.index()];
            let org = &mut self.organisms[oi];
            let digest = 0.55 + 0.45 * org.actuators.enzyme;
            let yield_e = spec.energy * EAT_YIELD * digest * org.genome.morph.hetero;
            org.gain(yield_e);
            if spec.harm > 0.0 {
                org.hurt(spec.harm, spec.harm * 0.22);
            }
            org.taste_food(kind.index(), spec.energy, spec.harm);
            self.dishes[di].foods[fi].fade = Some(0.0);
        }
    }

    fn apply_edge_zones(&mut self, dt: f32) {
        for org in &mut self.organisms {
            let Some(di) = self.dishes.iter().position(|d| d.id == org.dish_id) else {
                continue;
            };
            let hx = self.dishes[di].half_x;
            let hy = self.dishes[di].half_y;
            let edges = self.dishes[di].edges;
            let p = org.centroid();
            let dists = [
                p.x - (-hx), // left
                hx - p.x,    // right
                p.y - (-hy), // bottom
                hy - p.y,    // top
            ];
            for (i, &d) in dists.iter().enumerate() {
                let zone = edges[i];
                if zone.effect == EdgeEffect::None || zone.reach <= 1e-4 || d > zone.reach {
                    continue;
                }
                let u = (1.0 - d / zone.reach).clamp(0.0, 1.0);
                let s = u * u * dt;
                match zone.effect {
                    EdgeEffect::None => {}
                    EdgeEffect::Dopamine => {
                        org.env_reward += s * 1.1;
                    }
                    EdgeEffect::Hurt => {
                        org.hurt(s * 0.55, s * 0.22);
                    }
                    EdgeEffect::Heal => {
                        org.damage = (org.damage - s * 0.4).max(0.0);
                        org.gain(s * 0.08);
                    }
                    EdgeEffect::Energy => {
                        org.gain(s * 0.28);
                    }
                }
            }
        }
    }

    fn bites_dish(&mut self, dish_id: u32, dt: f32) {
        let members: Vec<usize> = self
            .organisms
            .iter()
            .enumerate()
            .filter(|(_, o)| o.dish_id == dish_id)
            .map(|(i, _)| i)
            .collect();
        let n = members.len();
        if n < 2 {
            return;
        }
        let Some(di) = self.dish_index(dish_id) else {
            return;
        };
        let (hx, hy) = (self.dishes[di].half_x, self.dishes[di].half_y);
        self.scratch_centers.clear();
        self.scratch_radii.clear();
        for &i in &members {
            self.scratch_centers.push(self.organisms[i].centroid());
            self.scratch_radii.push(self.organisms[i].radius());
        }
        let cell = self
            .scratch_radii
            .iter()
            .copied()
            .fold(0.12_f32, f32::max)
            * 2.2;
        self.spatial
            .clear_and_build(hx, hy, cell, &self.scratch_centers);
        let centers = self.scratch_centers.clone();
        let radii = self.scratch_radii.clone();
        let mut pairs = Vec::new();
        self.spatial.for_each_pair(|a, b| {
            let delta = centers[b] - centers[a];
            let dist = delta.length().max(1e-4);
            let reach = radii[a] + radii[b];
            if dist > reach {
                return;
            }
            let nrm = delta * (1.0 / dist);
            let overlap = ((reach - dist) / reach).clamp(0.0, 1.0);
            pairs.push((members[a], members[b], nrm, overlap));
        });
        for (i, j, nrm, overlap) in pairs {
            self.try_bite(i, j, nrm, overlap, dt);
            self.try_bite(j, i, nrm * -1.0, overlap, dt);
        }
    }

    /// Move organisms that touch a tube mouth into the linked dish.
    fn transfer_tubes(&mut self) {
        if self.tubes.is_empty() {
            return;
        }
        let tubes = self.tubes.clone();
        for tube in tubes {
            self.try_tube_transfer(&tube, true);
            self.try_tube_transfer(&tube, false);
        }
    }

    fn try_tube_transfer(&mut self, tube: &Tube, a_to_b: bool) {
        let (from_id, from_side, from_along, to_id, to_side, to_along) = if a_to_b {
            (
                tube.a_dish,
                tube.a_side,
                tube.a_along,
                tube.b_dish,
                tube.b_side,
                tube.b_along,
            )
        } else {
            (
                tube.b_dish,
                tube.b_side,
                tube.b_along,
                tube.a_dish,
                tube.a_side,
                tube.a_along,
            )
        };
        let Some(from_i) = self.dish_index(from_id) else {
            return;
        };
        let Some(to_i) = self.dish_index(to_id) else {
            return;
        };
        let mouth = self.dishes[from_i].rim_pos(from_side, from_along);
        let exit = self.dishes[to_i].rim_pos(to_side, to_along);
        let inward = PetriDish::port_inward(to_side);
        let radius = tube.radius;
        let mut movers = Vec::new();
        for (oi, org) in self.organisms.iter().enumerate() {
            if org.dish_id != from_id {
                continue;
            }
            if (org.head() - mouth).length() <= radius {
                movers.push(oi);
            }
        }
        for oi in movers {
            let (hx, hy) = (self.dishes[to_i].half_x, self.dishes[to_i].half_y);
            let org = &mut self.organisms[oi];
            let head = org.head();
            let target_head = exit + inward * (radius + 0.04);
            let shift = target_head - head;
            for n in &mut org.nodes {
                *n += shift;
            }
            for v in &mut org.vels {
                *v = inward * 0.35;
            }
            org.dish_id = to_id;
            for n in &mut org.nodes {
                *n = Vec2::new(
                    n.x.clamp(-hx + 0.03, hx - 0.03),
                    n.y.clamp(-hy + 0.03, hy - 0.03),
                );
            }
        }
    }

    fn try_bite(&mut self, attacker: usize, victim: usize, toward: Vec2, overlap: f32, dt: f32) {
        // Only intentional attack (kill-forward), never accidental food-mouth collisions.
        let attack = self.organisms[attacker].actuators.attack;
        if attack < 0.45 {
            return;
        }
        let facing = self.organisms[attacker].axis().dot(toward);
        if facing < 0.35 {
            return;
        }
        let mass_adv = (self.organisms[attacker].mass() / self.organisms[victim].mass().max(0.05))
            .clamp(0.35, 2.2);
        let raw = attack * overlap * facing * BITE_RATE * mass_adv * dt;
        let bite = raw.min(self.organisms[victim].energy.max(0.0) * 0.5);
        if bite <= 0.004 {
            return;
        }
        let pos = (self.organisms[victim].centroid() + self.organisms[attacker].head()) * 0.5;
        self.organisms[victim].hurt(bite * 1.35, bite);
        self.organisms[attacker].gain(bite * BITE_EFFICIENCY);
        // Particles + mild flash — only for a real strike.
        self.burst_spray(pos, toward * -1.0, 0.02, 1, 10, 0.07, 0.42);
        self.flashes.push(Flash {
            pos,
            life: 0.18,
            max_life: 0.18,
        });
        if self.flashes.len() > 24 {
            let extra = self.flashes.len() - 24;
            self.flashes.drain(0..extra);
        }
    }

    fn births(&mut self) {
        let n = self.organisms.len();
        if n == 0 || n >= MAX_POP {
            return;
        }
        let mut used = vec![false; n];
        let mut plans = Vec::new();
        for i in 0..n {
            if used[i] || !self.organisms[i].wants_child() {
                continue;
            }
            let mate = (0..n).find(|&j| {
                j != i
                    && !used[j]
                    && self.organisms[j].dish_id == self.organisms[i].dish_id
                    && self.organisms[j].wants_child()
                    && (self.organisms[i].centroid() - self.organisms[j].centroid()).length()
                        < self.organisms[i].radius() + self.organisms[j].radius() + 0.05
                    && self.organisms[i].genome.distance(&self.organisms[j].genome) < 0.92
            });
            if let Some(j) = mate {
                used[j] = true;
            }
            used[i] = true;
            plans.push((i, mate));
            if n + plans.len() >= MAX_POP {
                break;
            }
        }
        for (i, mate) in plans {
            self.spawn_child(i, mate);
        }
    }

    fn spawn_child(&mut self, i: usize, mate: Option<usize>) {
        let mate = mate.filter(|&j| self.organisms[j].energy >= REPRO_THRESHOLD);
        let dish_id = self.organisms[i].dish_id;
        let base = self.organisms[i].genome.clone();
        let mut child_genome = if let Some(j) = mate {
            let other = self.organisms[j].genome.clone();
            base.crossover(&other, &mut self.rng)
        } else {
            base
        };
        child_genome.mutate(&mut self.rng);
        child_genome.generation = match mate {
            Some(j) => {
                self.organisms[i]
                    .genome
                    .generation
                    .max(self.organisms[j].genome.generation)
                    + 1
            }
            None => self.organisms[i].genome.generation + 1,
        };
        let axis = self.organisms[i].axis();
        let ahead = self.organisms[i].radius() + child_genome.morph.radius + 0.03;
        let mut pos = self.organisms[i].centroid() + axis * ahead;
        let (hx, hy) = self
            .dish_index(dish_id)
            .map(|di| (self.dishes[di].half_x, self.dishes[di].half_y))
            .unwrap_or((DISH, DISH));
        if pos.x.abs() > hx - 0.1 || pos.y.abs() > hy - 0.1 {
            pos = self.organisms[i].centroid() - axis * ahead;
        }
        pos = if let Some(di) = self.dish_index(dish_id) {
            self.dishes[di].clamp_local(pos, 0.08)
        } else {
            self.clamp_inside(pos, 0.08)
        };
        if mate.is_some() {
            let j = mate.unwrap();
            self.organisms[i].spend(0.36);
            self.organisms[j].spend(0.36);
            self.organisms[j].repro_cd = REPRO_COOLDOWN;
            self.organisms[j].birth_pulse = 1.0;
        } else {
            self.organisms[i].spend(CHILD_ENERGY + REPRO_OVERHEAD);
        }
        self.organisms[i].repro_cd = REPRO_COOLDOWN;
        self.organisms[i].birth_pulse = 1.0;
        self.organisms[i].shift(axis * -0.015);
        let energy = if mate.is_some() { 0.62 } else { CHILD_ENERGY };
        let id = self.alloc_id();
        let mut child = Organism::born(id, child_genome, pos, axis, energy);
        child.dish_id = dish_id;
        self.organisms.push(child);
        self.births += 1;
    }

    fn deaths(&mut self) {
        let mut i = 0;
        while i < self.organisms.len() {
            if self.organisms[i].dead() {
                let org = self.organisms.swap_remove(i);
                let pos = org.centroid();
                let hue = org.genome.morph.hue;
                let dish_id = org.dish_id;
                if let Some(di) = self.dish_index(dish_id) {
                    if self.dishes[di].foods.len() < FOOD_CAP {
                        self.dishes[di]
                            .foods
                            .push(Food::fresh(pos, FoodKind::Green));
                    }
                }
                self.death_burst(pos, hue);
                self.deaths += 1;
            } else {
                i += 1;
            }
        }
    }

    fn death_burst(&mut self, pos: Vec2, hue: f32) {
        // Soft cloud
        self.burst(pos, hue, 28, 0.055, 0.95, 0);
        // Chunks flying outward
        self.burst(pos, hue, 18, 0.11, 1.35, 2);
        // A few red flecks
        self.burst(pos, 0.02, 10, 0.08, 0.7, 1);
    }

    fn burst(&mut self, pos: Vec2, hue: f32, count: usize, speed: f32, life: f32, style: u8) {
        for _ in 0..count {
            let ang = self.rng.gen_range(0.0..std::f32::consts::TAU);
            let sp = speed * self.rng.gen_range(0.35..1.15);
            let size = match style {
                1 => self.rng.gen_range(0.7..1.8),
                2 => self.rng.gen_range(1.2..2.8),
                _ => self.rng.gen_range(0.55..1.4),
            };
            self.sparks.push(Spark {
                pos,
                vel: Vec2::new(ang.cos(), ang.sin()) * sp,
                life,
                max_life: life,
                hue,
                size,
                style,
            });
        }
        self.trim_sparks();
    }

    fn burst_spray(
        &mut self,
        pos: Vec2,
        dir: Vec2,
        hue: f32,
        style: u8,
        count: usize,
        speed: f32,
        life: f32,
    ) {
        let base = if dir.length() > 1e-4 {
            dir.normalized()
        } else {
            Vec2::new(1.0, 0.0)
        };
        let ang0 = base.y.atan2(base.x);
        for _ in 0..count {
            let ang = ang0 + self.rng.gen_range(-0.85..0.85);
            let sp = speed * self.rng.gen_range(0.45..1.25);
            self.sparks.push(Spark {
                pos,
                vel: Vec2::new(ang.cos(), ang.sin()) * sp,
                life: life * self.rng.gen_range(0.7..1.15),
                max_life: life,
                hue,
                size: self.rng.gen_range(0.8..2.0),
                style,
            });
        }
        self.trim_sparks();
    }

    fn trim_sparks(&mut self) {
        if self.sparks.len() > 720 {
            let extra = self.sparks.len() - 720;
            self.sparks.drain(0..extra);
        }
    }

    fn fade_sparks(&mut self, dt: f32) {
        for spark in &mut self.sparks {
            spark.pos += spark.vel * dt;
            let drag = match spark.style {
                2 => 0.94,
                1 => 0.88,
                _ => 0.9,
            };
            spark.vel = spark.vel * drag;
            spark.life -= dt;
        }
        self.sparks.retain(|s| s.life > 0.0);
    }

    fn fade_flashes(&mut self, dt: f32) {
        for flash in &mut self.flashes {
            flash.life -= dt;
        }
        self.flashes.retain(|f| f.life > 0.0);
    }
}

struct Near {
    pos: Vec2,
    hue: f32,
    dish_id: u32,
}

fn add_touch(org: &Organism, normal: Vec2, amount: f32, bins: &mut [f32; 4]) {
    let axis = org.axis();
    let side = axis.perp();
    let ang = normal.dot(side).atan2(normal.dot(axis));
    let u = (ang + std::f32::consts::PI) / std::f32::consts::TAU;
    let s = ((u * 4.0).floor() as usize) % 4;
    bins[s] = (bins[s] + amount * 22.0).min(1.5);
}

fn sense_food(
    foods: &[Food],
    kinds: &[FoodSpec; FOOD_KIND_COUNT],
    p: Vec2,
    taste: &[f32; 3],
) -> f32 {
    food_pull(foods, kinds, p, taste).0
}

/// Per-kind smell clouds. Aim is weighted by taste memory: unknown/pleasant → approach,
/// known poison → flee. Energy/harm themselves stay hidden until a meal is eaten.
fn food_pull(
    foods: &[Food],
    kinds: &[FoodSpec; FOOD_KIND_COUNT],
    p: Vec2,
    taste: &[f32; 3],
) -> (f32, Vec2, [f32; 3]) {
    let mut kind_smell = [0.0f32; 3];
    let mut pull = Vec2::ZERO;
    for food in foods {
        if !food.alive() {
            continue;
        }
        let scale = food.smell_scale();
        if scale < 0.04 {
            continue;
        }
        let spec = kinds[food.kind.index()];
        let reach = spec.sense * scale + FOOD_SENSOR_REACH;
        let delta = food.pos - p;
        let dist = delta.length();
        if dist >= reach {
            continue;
        }
        let proximity = if dist < 1e-5 {
            1.0
        } else {
            1.0 - dist / reach
        };
        let ki = food.kind.index();
        kind_smell[ki] = kind_smell[ki].max(proximity);
        if dist < 1e-5 {
            continue;
        }
        let valence = taste[ki].clamp(-1.0, 1.0);
        let dir = delta * (1.0 / dist);
        pull += dir * (proximity * valence);
    }
    let smell = kind_smell.iter().copied().fold(0.0_f32, f32::max);
    let aim_len = pull.length();
    let aim = if aim_len < 1e-4 {
        Vec2::ZERO
    } else {
        pull * (aim_len.clamp(0.0, 1.0) / aim_len)
    };
    (smell.clamp(0.0, 1.0), aim, kind_smell)
}

fn crowd(orgs: &[Organism]) -> Vec<(f32, f32, f32, f32)> {
    let pose: Vec<(Vec2, Vec2, u32)> = orgs
        .iter()
        .map(|o| (o.centroid(), o.axis(), o.dish_id))
        .collect();
    let mut out = Vec::with_capacity(orgs.len());
    for i in 0..orgs.len() {
        let (pos, axis, dish_id) = pose[i];
        let side = axis.perp();
        let mut near_n = 0.0f32;
        let mut ahead = 0.0f32;
        let mut behind = 0.0f32;
        let mut left_n = 0.0f32;
        let mut right_n = 0.0f32;
        let mut best_sim = 0.0f32;
        let mut best_d = f32::MAX;
        for j in 0..orgs.len() {
            if i == j || pose[j].2 != dish_id {
                continue;
            }
            let delta = pose[j].0 - pos;
            let dist = delta.length().max(1e-4);
            let dir = delta * (1.0 / dist);
            if dist < 0.32 {
                near_n += 1.0;
                let f = dir.dot(axis);
                let s = dir.dot(side);
                if f > 0.25 {
                    ahead += 1.0;
                } else if f < -0.25 {
                    behind += 1.0;
                }
                if s > 0.25 {
                    left_n += 1.0;
                } else if s < -0.25 {
                    right_n += 1.0;
                }
            }
            if dist < 0.45 && dist < best_d && dir.dot(axis) > 0.45 {
                best_d = dist;
                let d = gene_distance(orgs, i, j);
                best_sim = (1.0 - d).clamp(0.0, 1.0);
            }
        }
        out.push((
            (near_n / 6.0).clamp(0.0, 1.0),
            ((ahead - behind) / 4.0).clamp(-1.0, 1.0),
            ((left_n - right_n) / 4.0).clamp(-1.0, 1.0),
            best_sim,
        ));
    }
    out
}

fn gene_distance(orgs: &[Organism], i: usize, j: usize) -> f32 {
    let (a, b) = if i < j { (i, j) } else { (j, i) };
    let (left, right) = orgs.split_at(b);
    left[a].genome.distance(&right[0].genome)
}

fn survives(zone: Survival, p: Vec2, half_x: f32, half_y: f32, time: f32) -> bool {
    let nx = if half_x > 1e-4 { p.x / half_x } else { 0.0 };
    let ny = if half_y > 1e-4 { p.y / half_y } else { 0.0 };
    match zone {
        Survival::Live => true,
        Survival::RightSide => nx > 0.15,
        Survival::Edges => nx.abs() > 0.55 || ny.abs() > 0.55,
        Survival::Center => nx * nx + ny * ny < 0.36,
        Survival::Shifting => {
            let phase = (time * 0.15).sin();
            if phase >= 0.0 {
                nx > 0.1
            } else {
                ny.abs() > 0.45
            }
        }
    }
}

fn cone_kin(origin: Vec2, dir: Vec2, hue: f32, dish_id: u32, near: &[Near]) -> f32 {
    let mut best: f32 = 0.0;
    for other in near {
        if other.dish_id != dish_id {
            continue;
        }
        let to = other.pos - origin;
        let dist = to.length();
        if dist < 0.02 || dist > 0.45 {
            continue;
        }
        let align = to.normalized().dot(dir);
        if align < 0.62 {
            continue;
        }
        let s = hue_similarity(hue, other.hue) * align / (1.0 + dist * 5.0);
        best = best.max(s);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Brain;
    use crate::organism::hunger_of;

    #[test]
    fn dish_runs_without_blowing_up() {
        let mut world = World::new(11);
        for _ in 0..(100 * 60) {
            world.step(1.0 / 60.0);
        }
        assert!(world.food_count() < 10_000);
        for org in &world.organisms {
            assert!(org.energy.is_finite() && org.pulse.is_finite());
        }
    }

    #[test]
    fn offspring_does_not_inherit_learned_weights() {
        let mut world = World::new(3);
        world.organisms.truncate(1);
        {
            let org = &mut world.organisms[0];
            org.energy = 1.7;
            org.age = 30.0;
            org.repro_cd = 0.0;
            org.actuators.reproduce = 1.0;
            assert!(org.brain.has_weights());
            let innate = org.brain.first_weight();
            org.brain
                .set_first_weight(if innate >= 0.0 { -3.5 } else { 3.5 });
        }
        let learned = world.organisms[0].brain.first_weight();
        world.births();
        assert_eq!(world.organisms.len(), 2);
        let child = &world.organisms[1];
        let fresh = Brain::from_genome(&child.genome);
        assert!(child.brain.max_diff(&fresh) < 1e-4);
        let parent_born = Brain::from_genome(&world.organisms[0].genome);
        assert!((parent_born.first_weight() - learned).abs() > 1.0);
        assert!((world.organisms[0].brain.first_weight() - learned).abs() < 1e-4);
    }

    #[test]
    fn survival_zones_keep_the_matching_side() {
        assert!(survives(
            Survival::RightSide,
            Vec2::new(0.4, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(!survives(
            Survival::RightSide,
            Vec2::new(-0.4, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(survives(
            Survival::Center,
            Vec2::new(0.1, 0.1),
            1.0,
            1.0,
            0.0
        ));
        assert!(!survives(
            Survival::Center,
            Vec2::new(0.9, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(survives(
            Survival::Edges,
            Vec2::new(0.8, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(!survives(
            Survival::Edges,
            Vec2::new(0.0, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(survives(
            Survival::Shifting,
            Vec2::new(0.5, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert!(!survives(
            Survival::Shifting,
            Vec2::new(-0.5, 0.0),
            1.0,
            1.0,
            0.0
        ));
        assert_eq!(POP_SPARSE, 1000);
        assert_eq!(POP_DENSE, 3000);
    }

    #[test]
    fn feeding_can_make_a_positive_pulse() {
        let mut world = World::new(5);
        world.organisms.truncate(1);
        world.organisms[0].energy = 0.45;
        world.organisms[0].prev_hunger = hunger_of(0.45, world.organisms[0].genome.morph.setpoint);
        world.organisms[0].gain(0.5);
        world.organisms[0].reward(1.0 / 60.0);
        assert!(
            world.organisms[0].pulse > 0.0,
            "a meal while hungry should feel better than expected, pulse {}",
            world.organisms[0].pulse
        );
    }

    #[test]
    fn one_click_is_one_particle_and_the_circle_only_detects() {
        let mut world = World::new(2);
        world.organisms.clear();
        world.foods_mut().clear();
        world.spawn_at(Vec2::new(-0.85, 0.0));
        world.drop_food(Vec2::ZERO);
        world.drop_food(Vec2::new(0.4, 0.2));
        for food in world.foods_mut() {
            food.bloom = 1.0;
        }
        assert_eq!(world.foods().len(), 2);
        let curious = [0.28, 0.28, 0.28];
        assert!(sense_food(world.foods(), &world.food_kinds, Vec2::ZERO, &curious) > 0.95);
        assert_eq!(
            sense_food(
                world.foods(),
                &world.food_kinds,
                Vec2::new(
                    -(world.food_spec(FoodKind::Green).sense + FOOD_SENSOR_REACH + 0.05),
                    0.85
                ),
                &curious,
            ),
            0.0
        );
        world.organisms[0].actuators.mouth = 1.0;
        world.consume_food();
        assert_eq!(
            world.foods().iter().filter(|f| f.alive()).count(),
            2,
            "mouth outside the dot must not eat"
        );
        let head = world.organisms[0].head();
        world.organisms[0].shift(Vec2::ZERO - head);
        let energy = world.organisms[0].energy;
        world.consume_food();
        assert_eq!(world.foods().iter().filter(|f| f.alive()).count(), 1);
        assert!(world.organisms[0].energy > energy);
    }

    #[test]
    fn tasting_poison_teaches_aversion_to_that_smell() {
        let mut world = World::new(2);
        world.organisms.clear();
        world.foods_mut().clear();
        world.spawn_at(Vec2::new(-0.5, 0.0));
        let curious = [0.28, 0.28, 0.28];
        world.drop_food_kind(Vec2::ZERO, FoodKind::Toxic);
        world.foods_mut()[0].bloom = 1.0;
        let (_, aim_before, _) =
            food_pull(world.foods(), &world.food_kinds, Vec2::new(-0.15, 0.0), &curious);
        assert!(
            aim_before.x > 0.05,
            "curious agents approach unknown toxic smell"
        );
        world.organisms[0].taste_food(FoodKind::Toxic.index(), 0.28, 0.48);
        assert!(world.organisms[0].food_taste[2] < -0.4);
        let taste = world.organisms[0].food_taste;
        let (_, aim_after, _) =
            food_pull(world.foods(), &world.food_kinds, Vec2::new(-0.15, 0.0), &taste);
        assert!(
            aim_after.x < -0.05,
            "after tasting poison they flee that smell"
        );
    }

    #[test]
    fn hungry_agent_opens_mouth_on_smell() {
        let mut world = World::new(3);
        world.organisms.truncate(1);
        world.foods_mut().clear();
        world.organisms[0].energy = 0.35;
        let head = world.organisms[0].head();
        world.drop_food(head);
        world.foods_mut()[0].bloom = 1.0;
        world.step(1.0 / 60.0);
        assert!(
            world.organisms[0].actuators.mouth >= MOUTH_OPEN,
            "mouth {} should open when smelling food (taste≈0.28)",
            world.organisms[0].actuators.mouth
        );
        // Keep mouth open and place particle on the head after the step moved things.
        world.organisms[0].actuators.mouth = 1.0;
        let head = world.organisms[0].head();
        world.foods_mut()[0].pos = head;
        world.foods_mut()[0].fade = None;
        let before = world.foods().iter().filter(|f| f.alive()).count();
        world.consume_food();
        let after = world.foods().iter().filter(|f| f.alive()).count();
        assert!(after < before, "open mouth on the particle should eat it");
    }

    #[test]
    fn tube_moves_organism_between_dishes() {
        let mut world = World::new_with(9, 1, 0);
        world.organisms.truncate(1);
        let a = world.primary().id;
        let b = world.add_dish(Vec2::new(3.0, 0.0), 1.0, 1.0);
        assert!(world.add_tube(a, 1, 0.5, b, 0, 0.5));
        // Place head on the right-side mouth of dish A.
        let mouth = world.primary().rim_pos(1, 0.5);
        let head = world.organisms[0].head();
        world.organisms[0].shift(mouth - head);
        world.organisms[0].dish_id = a;
        world.transfer_tubes();
        assert_eq!(world.organisms[0].dish_id, b);
    }

    #[test]
    fn feeder_free_placement_and_dispersion() {
        let mut world = World::new(42);
        world.foods_mut().clear();
        assert_eq!(world.food_count(), 0);

        // Place a feeder at (0.2, -0.3)
        let pos = Vec2::new(0.2, -0.3);
        let idx = world.add_feeder(pos, FoodKind::Amber);
        assert_eq!(idx, 0);
        assert_eq!(world.feeders().len(), 1);
        let feeder = &world.feeders()[0];
        assert!(!feeder.enabled, "Newly placed feeder must start disabled");
        assert_eq!(feeder.kind, FoodKind::Amber);
        assert_eq!(feeder.pos, pos);

        // Ticking while disabled does not spawn food
        world.tick_feeders(2.0);
        assert_eq!(world.food_count(), 0);

        // Enable feeder and configure rate & radius
        world.set_feeder_enabled(0, true);
        world.set_feeder_rate(0, 5.0); // 5 food/s -> every 0.2s
        world.set_feeder_radius(0, 0.25);
        world.tick_feeders(0.25);

        assert!(world.food_count() > 0, "Enabled feeder should spawn food");
        let spawned = &world.foods()[0];
        assert_eq!(spawned.kind, FoodKind::Amber);
        let dist = (spawned.pos - pos).length();
        assert!(
            dist <= 0.25 + 1e-4,
            "Spawned food distance {} must be within feeder radius 0.25",
            dist
        );

        // Pick feeder
        assert_eq!(world.pick_feeder(Vec2::new(0.21, -0.29), 0.1), Some(0));
        assert_eq!(world.pick_feeder(Vec2::new(0.8, 0.8), 0.1), None);

        // Delete feeder
        assert!(world.remove_feeder(0));
        assert_eq!(world.feeders().len(), 0);
    }

    #[test]
    fn mutate_organism_and_feeder_count() {
        let mut world = World::new_with(77, 2, 0);
        assert_eq!(world.active_feeder_count(), 0);

        world.add_feeder(Vec2::new(0.1, 0.1), FoodKind::Green);
        assert_eq!(world.active_feeder_count(), 0);

        world.set_feeder_enabled(0, true);
        assert_eq!(world.active_feeder_count(), 1);

        let id = world.organisms[0].id;
        let orig_drift = world.organisms[0].genome.drift_from_root();
        assert!(world.mutate_organism(id));
        let new_drift = world.organisms[0].genome.drift_from_root();
        assert!(new_drift >= orig_drift);
        assert_eq!(world.organisms[0].nodes.len(), world.organisms[0].genome.morph.nodes as usize);
        assert!(!world.mutate_organism(999_999));
    }
}

