pub const DISH: f32 = 1.0;
pub const GRID: usize = 128;
pub const MAX_POP: usize = 96;
/// Soft cap on petri dishes sharing one lab table.
pub const MAX_DISHES: usize = 8;

pub const DT_CAP: f32 = 1.0 / 30.0;

pub const BASAL_MASS: f32 = 0.009;
pub const BASAL_NEURON: f32 = 0.00026;
pub const BASAL_SYNAPSE: f32 = 0.000045;

pub const EAT_YIELD: f32 = 0.82;
/// How far from the head the food sensors reach. Same length as the drawn sense arcs.
pub const FOOD_SENSOR_REACH: f32 = 0.45;
pub const FOOD_BITE: f32 = 0.05;
pub const MOUTH_OPEN: f32 = 0.4;
/// Soft ceiling so auto-spawn cannot pack the dish.
pub const FOOD_CAP: usize = 48;
pub const FOOD_KIND_COUNT: usize = 3;
pub const MOVE_COST: f32 = 0.016;
pub const SIGNAL_COST: f32 = 0.012;
pub const HEAL_RATE: f32 = 0.012;
pub const HEAL_COST: f32 = 0.01;

pub const MAX_ENERGY: f32 = 2.0;
pub const START_ENERGY: f32 = 1.05;
pub const CHILD_ENERGY: f32 = 0.58;
pub const REPRO_THRESHOLD: f32 = 1.12;
pub const REPRO_OVERHEAD: f32 = 0.06;
pub const REPRO_COOLDOWN: f32 = 5.0;
pub const BITE_RATE: f32 = 0.2;
pub const BITE_EFFICIENCY: f32 = 0.72;

pub const THRUST: f32 = 0.36;
pub const DRAG: f32 = 7.8;
pub const SPRING_K: f32 = 28.0;
pub const MUSCLE_SHORTEN: f32 = 0.42;

pub const ELIG_TAU: f32 = 0.7;
pub const EXPECT_TAU: f32 = 0.55;
pub const TONIC_TAU: f32 = 1.3;
pub const PLASTIC_TIME: f32 = 42.0;
pub const DW_PER_SEC: f32 = 0.28;
pub const NOVELTY_SCALE: f32 = 0.12;

pub const AGE_TAX_START: f32 = 150.0;

/// One gene is 32 bits. Length may be 2, 4, 8, 16, 32, … up to 1000.
pub const GENOME_LENGTH: usize = 128;
pub const GENOME_LENGTH_MAX: usize = 1000;
/// Inner neurons addressed by a gene are taken modulo this count (1..=127).
pub const INNER_NEURONS: u8 = 16;
/// Point mutation: each bit flips with probability 1/1000 when a genome is copied.
pub const BIT_FLIP: f64 = 0.001;
/// Signed 16-bit weight / this divisor ≈ [-4, 4].
pub const WEIGHT_DIVISOR: f32 = 8000.0;
/// Steps in one generation. The age sensor reaches 1 at the end of it.
pub const GENERATION_STEPS: u32 = 280;
pub const OSCILLATOR_STEPS: f32 = 25.0;
/// Kill-forward can be switched off without removing the action neuron.
pub const KILL_FORWARD: bool = true;

/// Body-chain length (segments = nodes). Wider range lets morphology evolve.
pub const NODES_MIN: u8 = 2;
pub const NODES_MAX: u8 = 12;
pub const NODES_BIRTH_MIN: u8 = 3;
pub const NODES_BIRTH_MAX: u8 = 7;
/// Chance per birth that `morph.nodes` steps by ±1.
pub const NODE_MUTATION: f32 = 0.07;

/// Reference population sizes for a generational grid run. The live dish stays smaller.
#[allow(dead_code)]
pub const POP_SPARSE: usize = 1000;
#[allow(dead_code)]
pub const POP_DENSE: usize = 3000;
