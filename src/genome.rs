use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::math::{gaussian, wrap_unit};
use crate::tune::{
    BIT_FLIP, GENOME_LENGTH, GENOME_LENGTH_MAX, INNER_NEURONS, NODE_MUTATION, NODES_BIRTH_MAX,
    NODES_BIRTH_MIN, NODES_MAX, NODES_MIN, WEIGHT_DIVISOR,
};

/// Fixed sensory ports. Values fed into the net are clamped to `[0, 1]`.
/// Includes food-kind smells plus forward/side food aim so foraging can evolve in the net
/// (no hardcoded chemotaxis).
pub const SENSOR_BASE: usize = 26;
/// Fixed action ports. Muscle ports follow, one per body spring.
pub const ACTION_BASE: usize = 15;

pub const ACT_MOVE_X: usize = 0;
pub const ACT_MOVE_Y: usize = 1;
pub const ACT_MOVE_FORWARD: usize = 2;
pub const ACT_MOVE_RANDOM: usize = 3;
pub const ACT_MOVE_EAST: usize = 4;
pub const ACT_MOVE_WEST: usize = 5;
pub const ACT_MOVE_NORTH: usize = 6;
pub const ACT_MOVE_SOUTH: usize = 7;
pub const ACT_EMIT_PHEROMONE: usize = 8;
pub const ACT_SET_RESPONSIVENESS: usize = 9;
pub const ACT_SET_OSCILLATOR: usize = 10;
pub const ACT_KILL_FORWARD: usize = 11;
pub const ACT_ENZYME: usize = 12;
pub const ACT_REPRODUCE: usize = 13;
pub const ACT_GROWTH: usize = 14;
/// First of three head-smell channels (green, amber, toxic).
pub const SENSE_FOOD: usize = 11;
pub const SENSE_FOOD_KINDS: usize = 3;
/// Taste-weighted food direction in body frame (after kind smells + Δ).
pub const SENSE_FOOD_FWD: usize = 15;
pub const SENSE_FOOD_SIDE: usize = 16;

/// One connection, packed into 32 bits.
///
/// ```text
/// 31        source type   0 = sensor, 1 = inner
/// 30..24    source id     modulo the count of that type
/// 23        sink type     0 = inner,  1 = action
/// 22..16    sink id       modulo the count of that type
/// 15..0     weight        i16, later divided by 8000
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gene {
    pub data: u32,
}

impl Gene {
    pub fn source_type(self) -> u32 {
        (self.data >> 31) & 1
    }

    pub fn source_id(self) -> u32 {
        (self.data >> 24) & 0x7f
    }

    pub fn sink_type(self) -> u32 {
        (self.data >> 23) & 1
    }

    pub fn sink_id(self) -> u32 {
        (self.data >> 16) & 0x7f
    }

    pub fn weight_raw(self) -> i16 {
        self.data as i16
    }

    pub fn weight(self) -> f32 {
        self.weight_raw() as f32 / WEIGHT_DIVISOR
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Morph {
    pub nodes: u8,
    pub radius: f32,
    pub mass: f32,
    pub hue: f32,
    pub hetero: f32,
    pub setpoint: f32,
    pub learn: f32,
    pub hunger_w: f32,
    pub pain_w: f32,
    pub novelty_w: f32,
    pub birth_w: f32,
    pub maturity: f32,
}

impl Morph {
    pub fn sensor_count(&self) -> usize {
        SENSOR_BASE + self.nodes as usize - 1
    }

    pub fn action_count(&self) -> usize {
        ACTION_BASE + self.nodes as usize - 1
    }

    fn mix(a: f32, b: f32, rng: &mut impl Rng) -> f32 {
        if rng.gen_bool(0.5) {
            a
        } else {
            b
        }
    }

    fn crossed(&self, other: &Morph, rng: &mut impl Rng) -> Morph {
        Morph {
            nodes: if rng.gen_bool(0.5) {
                self.nodes
            } else {
                other.nodes
            },
            radius: Self::mix(self.radius, other.radius, rng),
            mass: Self::mix(self.mass, other.mass, rng),
            hue: if rng.gen_bool(0.5) {
                self.hue
            } else {
                other.hue
            },
            hetero: Self::mix(self.hetero, other.hetero, rng),
            setpoint: Self::mix(self.setpoint, other.setpoint, rng),
            learn: Self::mix(self.learn, other.learn, rng),
            hunger_w: Self::mix(self.hunger_w, other.hunger_w, rng),
            pain_w: Self::mix(self.pain_w, other.pain_w, rng),
            novelty_w: Self::mix(self.novelty_w, other.novelty_w, rng),
            birth_w: Self::mix(self.birth_w, other.birth_w, rng),
            maturity: Self::mix(self.maturity, other.maturity, rng),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genome {
    pub morph: Morph,
    /// Founder morphology from generation 0 of this lineage (never mutated).
    pub root_morph: Morph,
    pub genes: Vec<Gene>,
    /// Founder gene pack from generation 0 (for drift vs ancestor).
    pub root_genes: Vec<Gene>,
    /// Inner neurons available to genes. Clamped to 1..=127.
    pub inner_neurons: u8,
    pub generation: u32,
}

impl Genome {
    pub fn random(rng: &mut impl Rng) -> Self {
        let nodes = rng.gen_range(NODES_BIRTH_MIN..=NODES_BIRTH_MAX);
        let morph = Morph {
            nodes,
            radius: rng.gen_range(0.045..0.075),
            mass: rng.gen_range(0.55..0.95),
            hue: rng.gen::<f32>(),
            hetero: rng.gen_range(0.45..1.05),
            setpoint: rng.gen_range(0.72..1.05),
            learn: rng.gen_range(0.08..0.28),
            hunger_w: rng.gen_range(0.7..1.3),
            pain_w: rng.gen_range(0.7..1.3),
            novelty_w: rng.gen_range(0.15..0.7),
            birth_w: rng.gen_range(0.5..1.2),
            maturity: rng.gen_range(6.0..12.0),
        };
        let mut genome = Self {
            morph: morph.clone(),
            root_morph: morph,
            genes: Vec::new(),
            root_genes: Vec::new(),
            inner_neurons: INNER_NEURONS,
            generation: 0,
        };
        genome.resize_genes(GENOME_LENGTH, rng);
        genome.root_genes = genome.genes.clone();
        genome
    }

    pub fn resize_genes(&mut self, len: usize, rng: &mut impl Rng) {
        let len = len.clamp(2, GENOME_LENGTH_MAX);
        if self.genes.len() > len {
            self.genes.truncate(len);
        }
        while self.genes.len() < len {
            self.genes.push(Gene { data: rng.gen() });
        }
    }

    pub fn distance(&self, other: &Genome) -> f32 {
        Self::gene_distance(&self.genes, &other.genes)
    }

    pub fn drift_from_root(&self) -> f32 {
        Self::gene_distance(&self.genes, &self.root_genes)
    }

    fn gene_distance(a: &[Gene], b: &[Gene]) -> f32 {
        let n = a.len().min(b.len()).max(1);
        let mut weight = 0.0;
        let mut bits = 0u32;
        for i in 0..n {
            weight += (a[i].weight() - b[i].weight()).abs();
            bits += (a[i].data ^ b[i].data).count_ones();
        }
        let weight = weight / n as f32 / 8.0;
        let hamming = bits as f32 / (n as f32 * 32.0);
        0.65 * hamming + 0.35 * weight
    }

    /// One cut: genes before it come from this parent, the rest from the other.
    pub fn crossover(&self, other: &Genome, rng: &mut impl Rng) -> Genome {
        let mut child = self.clone();
        child.morph = self.morph.crossed(&other.morph, rng);
        // Keep the older lineage's founder snapshot.
        if other.generation > self.generation {
            child.root_morph = other.root_morph.clone();
            child.root_genes = other.root_genes.clone();
        } else {
            child.root_morph = self.root_morph.clone();
            child.root_genes = self.root_genes.clone();
        }
        child.inner_neurons = if rng.gen_bool(0.5) {
            self.inner_neurons
        } else {
            other.inner_neurons
        };
        let n = child.genes.len().min(other.genes.len());
        let cut = rng.gen_range(0..=n);
        for i in cut..n {
            child.genes[i] = other.genes[i];
        }
        child.generation = self.generation.max(other.generation);
        child
    }

    /// Each bit of each gene flips with probability 1/1000.
    pub fn mutate(&mut self, rng: &mut impl Rng) {
        for gene in &mut self.genes {
            let mut data = gene.data;
            for bit in 0..32 {
                if rng.gen_bool(BIT_FLIP) {
                    data ^= 1 << bit;
                }
            }
            gene.data = data;
        }
        if rng.gen::<f32>() < 0.02 {
            let step: i16 = if rng.gen_bool(0.5) { 1 } else { -1 };
            self.inner_neurons = (self.inner_neurons as i16 + step).clamp(1, 127) as u8;
        }
        self.morph.radius = (self.morph.radius + gaussian(rng) * 0.003).clamp(0.04, 0.09);
        self.morph.mass = (self.morph.mass + gaussian(rng) * 0.04).clamp(0.4, 1.4);
        self.morph.hue = wrap_unit(self.morph.hue + gaussian(rng) * 0.02);
        self.morph.hetero = (self.morph.hetero + gaussian(rng) * 0.04).clamp(0.25, 1.3);
        self.morph.setpoint = (self.morph.setpoint + gaussian(rng) * 0.03).clamp(0.55, 1.3);
        self.morph.learn = (self.morph.learn + gaussian(rng) * 0.02).clamp(0.03, 0.4);
        self.morph.hunger_w = (self.morph.hunger_w + gaussian(rng) * 0.05).clamp(0.2, 2.0);
        self.morph.pain_w = (self.morph.pain_w + gaussian(rng) * 0.05).clamp(0.2, 2.0);
        self.morph.novelty_w = (self.morph.novelty_w + gaussian(rng) * 0.04).clamp(0.0, 1.2);
        self.morph.birth_w = (self.morph.birth_w + gaussian(rng) * 0.04).clamp(0.1, 1.8);
        self.morph.maturity = (self.morph.maturity + gaussian(rng) * 0.3).clamp(4.0, 18.0);
        if rng.gen::<f32>() < NODE_MUTATION {
            let step = if rng.gen_bool(0.5) { 1 } else { -1 };
            self.morph.nodes = (self.morph.nodes as i32 + step).clamp(NODES_MIN as i32, NODES_MAX as i32) as u8;
        }
        // Rare gene-length drift — more wiring room for longer bodies.
        if rng.gen::<f32>() < 0.015 {
            let step: i32 = if rng.gen_bool(0.55) { 2 } else { -2 };
            let next = (self.genes.len() as i32 + step).clamp(8, GENOME_LENGTH_MAX as i32) as usize;
            // Keep power-of-two-ish growth without forcing exact powers.
            if next > self.genes.len() {
                while self.genes.len() < next {
                    self.genes.push(Gene { data: rng.gen() });
                }
            } else {
                self.genes.truncate(next);
            }
        }
    }

    /// Explicit user-triggered mutation: always flips several gene bits and nudges morphology.
    pub fn mutate_noticeable(&mut self, rng: &mut impl Rng) {
        if !self.genes.is_empty() {
            let flips = rng.gen_range(2..=5);
            for _ in 0..flips {
                let g = rng.gen_range(0..self.genes.len());
                let b = rng.gen_range(0..32);
                self.genes[g].data ^= 1 << b;
            }
        }
        self.morph.hue = wrap_unit(self.morph.hue + gaussian(rng) * 0.06 + 0.03);
        self.morph.radius = (self.morph.radius + gaussian(rng) * 0.006).clamp(0.04, 0.09);
        self.morph.mass = (self.morph.mass + gaussian(rng) * 0.08).clamp(0.4, 1.4);
        self.morph.hetero = (self.morph.hetero + gaussian(rng) * 0.06).clamp(0.25, 1.3);
        self.morph.setpoint = (self.morph.setpoint + gaussian(rng) * 0.05).clamp(0.55, 1.3);
        self.morph.learn = (self.morph.learn + gaussian(rng) * 0.04).clamp(0.03, 0.4);
        if rng.gen_bool(0.25) {
            let step: i32 = if rng.gen_bool(0.5) { 1 } else { -1 };
            self.morph.nodes = (self.morph.nodes as i32 + step).clamp(NODES_MIN as i32, NODES_MAX as i32) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gene_fields_and_weight_scale() {
        let mut data = 0u32;
        data |= 1 << 31;
        data |= 5 << 24;
        data |= 1 << 23;
        data |= 3 << 16;
        data |= (-8000i16 as u16) as u32;
        let gene = Gene { data };
        assert_eq!(gene.source_type(), 1);
        assert_eq!(gene.source_id(), 5);
        assert_eq!(gene.sink_type(), 1);
        assert_eq!(gene.sink_id(), 3);
        assert!((gene.weight() + 1.0).abs() < 1e-5);
        assert!((Gene { data: 32767 }.weight() - 32767.0 / 8000.0).abs() < 1e-5);
    }
}
