use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::brain::Brain;
use crate::genome::{
    Genome, ACT_EMIT_PHEROMONE, ACT_ENZYME, ACT_GROWTH, ACT_KILL_FORWARD, ACT_MOVE_EAST,
    ACT_MOVE_FORWARD, ACT_MOVE_NORTH, ACT_MOVE_RANDOM, ACT_MOVE_SOUTH, ACT_MOVE_WEST, ACT_MOVE_X,
    ACT_MOVE_Y, ACT_REPRODUCE, ACT_SET_OSCILLATOR, ACT_SET_RESPONSIVENESS, SENSE_FOOD,
    SENSE_FOOD_FWD, SENSE_FOOD_KINDS, SENSE_FOOD_SIDE,
};
use crate::math::Vec2;
use crate::tune::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actuators {
    pub thrust: f32,
    pub turn: f32,
    pub mouth: f32,
    /// Intent to bite other organisms (kill-forward). Separate from food mouth.
    pub attack: f32,
    pub enzyme: f32,
    pub signal: f32,
    pub reproduce: f32,
    pub growth: f32,
    pub muscles: Vec<f32>,
    pub drive: Vec2,
}

impl Default for Actuators {
    fn default() -> Self {
        Self {
            thrust: 0.0,
            turn: 0.0,
            mouth: 0.0,
            attack: 0.0,
            enzyme: 0.0,
            signal: 0.0,
            reproduce: 0.0,
            growth: 0.0,
            muscles: Vec::new(),
            drive: Vec2::ZERO,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Organism {
    pub id: u64,
    /// Which petri dish this body lives in (local coordinates).
    #[serde(default)]
    pub dish_id: u32,
    pub genome: Genome,
    pub brain: Brain,
    pub nodes: Vec<Vec2>,
    pub vels: Vec<Vec2>,
    pub rest: f32,
    pub scale: f32,
    pub energy: f32,
    pub damage: f32,
    pub age: f32,
    pub tonic: f32,
    pub expected: f32,
    pub pulse: f32,
    pub beat: f32,
    pub trace: Vec<f32>,
    pub surprise_expected: f32,
    pub novelty_excess: f32,
    pub prev_hunger: f32,
    pub prev_pain: f32,
    pub birth_pulse: f32,
    pub env_reward: f32,
    pub repro_cd: f32,
    pub actuators: Actuators,
    pub touch: [f32; 4],
    pub osc_phase: f32,
    pub osc_period: f32,
    pub responsiveness: f32,
    pub wander: f32,
    pub cruise: Vec2,
    /// Learned valence per food kind in `[-1, 1]`. Starts mildly curious; poison teaches aversion.
    pub food_taste: [f32; 3],
}

impl Organism {
    pub fn born(id: u64, genome: Genome, pos: Vec2, axis: Vec2, energy: f32) -> Self {
        let brain = Brain::from_genome(&genome);
        let nodes_n = genome.morph.nodes as usize;
        let rest = if nodes_n > 1 {
            genome.morph.radius * 2.2 / (nodes_n - 1) as f32
        } else {
            genome.morph.radius
        };
        let axis = axis.normalized();
        let mut nodes = Vec::with_capacity(nodes_n);
        for i in 0..nodes_n {
            let t = if nodes_n == 1 {
                0.0
            } else {
                i as f32 / (nodes_n - 1) as f32 - 0.5
            };
            nodes.push(pos + axis * (t * rest * (nodes_n - 1) as f32));
        }
        let hunger = hunger_of(energy, genome.morph.setpoint);
        let mut org = Self {
            id,
            dish_id: 0,
            genome,
            brain,
            nodes,
            vels: vec![Vec2::ZERO; nodes_n],
            rest,
            scale: 1.0,
            energy,
            damage: 0.0,
            age: 0.0,
            tonic: 0.0,
            expected: 0.0,
            pulse: 0.0,
            beat: 0.0,
            trace: Vec::new(),
            surprise_expected: 0.0,
            novelty_excess: 0.0,
            prev_hunger: hunger,
            prev_pain: 0.0,
            birth_pulse: 0.0,
            env_reward: 0.0,
            repro_cd: 1.5,
            actuators: Actuators::default(),
            touch: [0.0; 4],
            osc_phase: (id as f32 * 0.17).fract(),
            osc_period: OSCILLATOR_STEPS,
            responsiveness: 1.0,
            wander: (id as f32 * 0.37).fract(),
            cruise: Vec2::ZERO,
            food_taste: [0.28, 0.28, 0.28],
        };
        org.brain.finish();
        org
    }

    /// Mutates this organism's genome and rebuilds brain/nodes to match new morphology.
    pub fn mutate(&mut self, rng: &mut impl Rng) {
        self.genome.mutate_noticeable(rng);
        self.brain = Brain::from_genome(&self.genome);
        self.brain.finish();

        let target_nodes = self.genome.morph.nodes as usize;
        let axis = self.axis();
        let center = self.centroid();
        let rest = if target_nodes > 1 {
            self.genome.morph.radius * 2.2 / (target_nodes - 1) as f32
        } else {
            self.genome.morph.radius
        };
        self.rest = rest;

        if target_nodes != self.nodes.len() {
            let mut new_nodes = Vec::with_capacity(target_nodes);
            for i in 0..target_nodes {
                let t = if target_nodes == 1 {
                    0.0
                } else {
                    i as f32 / (target_nodes - 1) as f32 - 0.5
                };
                new_nodes.push(center + axis * (t * rest * (target_nodes - 1) as f32));
            }
            self.nodes = new_nodes;
            self.vels = vec![Vec2::ZERO; target_nodes];
        }

        self.pulse = 1.0;
        self.birth_pulse = 0.8;
    }

    pub fn centroid(&self) -> Vec2 {
        let mut c = Vec2::ZERO;
        for n in &self.nodes {
            c += *n;
        }
        c * (1.0 / self.nodes.len() as f32)
    }

    pub fn axis(&self) -> Vec2 {
        if self.nodes.len() < 2 {
            return Vec2::new(1.0, 0.0);
        }
        (self.nodes[0] - self.nodes[self.nodes.len() - 1]).normalized()
    }

    pub fn radius(&self) -> f32 {
        let span = self.rest * self.scale * self.nodes.len().saturating_sub(1) as f32;
        let node = self.genome.morph.radius * self.scale * 0.42;
        span * 0.5 + node
    }

    pub fn node_radius(&self) -> f32 {
        self.genome.morph.radius * self.scale * 0.42
    }

    pub fn mass(&self) -> f32 {
        self.genome.morph.mass * self.scale * self.scale
    }

    pub fn head(&self) -> Vec2 {
        self.nodes
            .first()
            .copied()
            .unwrap_or_else(|| self.centroid())
    }

    pub fn tail(&self) -> Vec2 {
        self.nodes
            .last()
            .copied()
            .unwrap_or_else(|| self.centroid())
    }

    pub fn dead(&self) -> bool {
        self.energy <= 0.02 || self.damage >= 1.15
    }

    pub fn hunger(&self) -> f32 {
        hunger_of(self.energy, self.genome.morph.setpoint)
    }

    pub fn repro_need(&self) -> f32 {
        let age = ((self.age - self.genome.morph.maturity) / 5.0).clamp(0.0, 1.0);
        let surplus = ((self.energy - self.genome.morph.setpoint) / 0.45).clamp(0.0, 1.0);
        age * surplus
    }

    pub fn wants_child(&self) -> bool {
        self.actuators.reproduce > 0.62
            && self.energy >= REPRO_THRESHOLD
            && self.age >= self.genome.morph.maturity
            && self.repro_cd <= 0.0
    }

    pub fn think(&mut self, inputs: &[f32], dt: f32) {
        self.sense_novelty(inputs, dt);
        self.brain.step(inputs, self.responsiveness);
        let mut outs = [0.0f32; 32];
        let n = self.brain.outputs().len().min(outs.len());
        outs[..n].copy_from_slice(&self.brain.outputs()[..n]);
        let at = |i: usize| outs.get(i).copied().unwrap_or(0.0);
        let pos = |y: f32| y.clamp(0.0, 1.0);
        let axis = self.axis();
        let side = axis.perp();
        let smell = (0..SENSE_FOOD_KINDS)
            .map(|i| inputs.get(SENSE_FOOD + i).copied().unwrap_or(0.0))
            .fold(0.0_f32, f32::max)
            .clamp(0.0, 1.0);
        let mut best_kind = 0usize;
        let mut best_kind_smell = -1.0f32;
        for i in 0..SENSE_FOOD_KINDS {
            let s = inputs.get(SENSE_FOOD + i).copied().unwrap_or(0.0);
            if s > best_kind_smell {
                best_kind_smell = s;
                best_kind = i;
            }
        }
        let taste = self.food_taste[best_kind].clamp(-1.0, 1.0);
        let hunger = self.hunger().clamp(0.0, 1.4);
        // Survival restlessness: hunger speeds wander and adds undirected search when starving.
        self.wander =
            (self.wander + dt * (0.8 + at(ACT_MOVE_RANDOM).max(0.0) + hunger * 0.55)).fract();
        let ang = self.wander * std::f32::consts::TAU;
        let wander = Vec2::new(ang.cos(), ang.sin()) * at(ACT_MOVE_RANDOM).max(0.0);
        let east = at(ACT_MOVE_EAST).max(0.0) - at(ACT_MOVE_WEST).max(0.0);
        let north = at(ACT_MOVE_NORTH).max(0.0) - at(ACT_MOVE_SOUTH).max(0.0);
        let mut drive = Vec2::new(at(ACT_MOVE_X) + east, at(ACT_MOVE_Y) + north)
            + axis * at(ACT_MOVE_FORWARD)
            + wander;
        // Species-survival floor: hungry + pleasant smell nearby → commit toward food.
        // Well-fed / no scent: motion is entirely from the net.
        let food_fwd = (inputs.get(SENSE_FOOD_FWD).copied().unwrap_or(0.5) - 0.5) * 2.0;
        let food_side = (inputs.get(SENSE_FOOD_SIDE).copied().unwrap_or(0.5) - 0.5) * 2.0;
        let local_food = axis * food_fwd + side * food_side;
        let urgency = (hunger * smell).clamp(0.0, 1.0);
        if urgency > 0.08 && taste > 0.05 {
            let flen = local_food.length();
            let toward = if flen > 1e-4 {
                local_food * (1.0 / flen)
            } else {
                Vec2::ZERO
            };
            let blend = (0.45 + 0.5 * urgency).clamp(0.0, 0.94);
            drive = drive * (1.0 - blend) + toward * (blend * (1.1 + 0.5 * smell));
            drive += Vec2::new(ang.cos(), ang.sin()) * (1.0 - smell) * urgency * 0.3;
        }
        if drive.length() > 1.25 {
            drive = drive.normalized() * 1.25;
        }
        let vigor = (self.responsiveness * (1.0 + 0.35 * self.tonic)).clamp(0.55, 1.8);
        let aimed = drive * vigor * 1.55;
        let follow = 1.0 - (-dt / 0.22).exp();
        self.cruise += (aimed - self.cruise) * follow;
        let springs = self.nodes.len().saturating_sub(1);
        let mut muscles = vec![0.0; springs];
        for i in 0..springs {
            muscles[i] = pos(at(ACT_GROWTH + 1 + i));
        }
        let kill = if KILL_FORWARD {
            pos(at(ACT_KILL_FORWARD))
        } else {
            0.0
        };
        // Sole hardcoded survival drive: open mouth when hungry near acceptable food.
        // Steering, flee, attack, enzyme, repro — all from the net.
        let bite_want = if taste > 0.05 {
            let openness = 0.55 + 0.45 * taste.clamp(0.0, 1.0);
            (smell * openness * (0.75 + 0.35 * hunger.min(1.0))).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.actuators = Actuators {
            thrust: self.cruise.dot(axis).clamp(-1.6, 1.6),
            turn: self.cruise.dot(side).clamp(-1.6, 1.6),
            mouth: bite_want,
            attack: kill,
            enzyme: pos(at(ACT_ENZYME)),
            signal: pos(at(ACT_EMIT_PHEROMONE)),
            reproduce: pos(at(ACT_REPRODUCE)),
            growth: pos(at(ACT_GROWTH)),
            muscles,
            drive: self.cruise,
        };
        self.responsiveness = (1.0 + at(ACT_SET_RESPONSIVENESS) * 0.45).clamp(0.55, 1.7);
        self.osc_period = (OSCILLATOR_STEPS + at(ACT_SET_OSCILLATOR) * 15.0).clamp(8.0, 48.0);
        self.osc_phase = (self.osc_phase + dt * 60.0 / self.osc_period.max(4.0)).fract();
        self.fade_food_taste(dt);
    }

    /// Slowly drift taste memory toward mild curiosity so aversion can be unlearned.
    fn fade_food_taste(&mut self, dt: f32) {
        let a = 1.0 - (-dt / 140.0).exp();
        for t in &mut self.food_taste {
            *t += (0.28 - *t) * a;
        }
    }

    /// Update kind valence after a meal. Poison teaches aversion; rich food teaches preference.
    pub fn taste_food(&mut self, kind_index: usize, energy: f32, harm: f32) {
        let i = kind_index % 3;
        let outcome = if harm > 0.04 {
            (-0.9 - harm * 0.55).max(-1.0)
        } else {
            (0.25 + energy * 0.6).min(1.0)
        };
        self.food_taste[i] = (self.food_taste[i] * 0.4 + outcome * 0.6).clamp(-1.0, 1.0);
    }

    pub fn integrate(&mut self, dt: f32, half_x: f32, half_y: f32, viscosity: f32) {
        let n = self.nodes.len();
        let axis = self.axis();
        let side = axis.perp();
        let node_mass = (self.mass() / n as f32).max(0.05);
        let mut force = [Vec2::ZERO; NODES_MAX as usize];
        let rest_now = self.rest * self.scale;
        for i in 0..n.saturating_sub(1) {
            let tension = self.actuators.muscles.get(i).copied().unwrap_or(0.0);
            let rest = rest_now * (1.0 - MUSCLE_SHORTEN * tension);
            let delta = self.nodes[i + 1] - self.nodes[i];
            let len = delta.length().max(1e-4);
            let dir = delta * (1.0 / len);
            let f = SPRING_K * (len - rest);
            force[i] += dir * f;
            force[i + 1] -= dir * f;
        }
        let thrust = self.actuators.drive * THRUST;
        let turn = self.actuators.turn * THRUST * 0.85;
        for i in 0..n {
            let head_w = 1.0 - i as f32 / n as f32;
            force[i] += thrust * (0.35 + 0.65 * head_w);
        }
        if n >= 2 {
            force[0] += side * turn;
            force[n - 1] -= side * (turn * 0.65);
        }
        let damp = (-DRAG * viscosity.clamp(0.2, 5.0) * dt).exp();
        for i in 0..n {
            // Soft push off the walls so they do not glue to the rim.
            let soft = 0.14;
            let limit_x = half_x - 0.02;
            let limit_y = half_y - 0.02;
            let px = self.nodes[i].x;
            let py = self.nodes[i].y;
            if px > limit_x - soft {
                force[i].x -= ((px - (limit_x - soft)) / soft).clamp(0.0, 1.0) * 2.4;
            } else if px < -limit_x + soft {
                force[i].x += ((-limit_x + soft - px) / soft).clamp(0.0, 1.0) * 2.4;
            }
            if py > limit_y - soft {
                force[i].y -= ((py - (limit_y - soft)) / soft).clamp(0.0, 1.0) * 2.4;
            } else if py < -limit_y + soft {
                force[i].y += ((-limit_y + soft - py) / soft).clamp(0.0, 1.0) * 2.4;
            }
            self.vels[i] += force[i] * (dt / node_mass);
            self.vels[i] = self.vels[i] * damp;
            self.nodes[i] += self.vels[i] * dt;
            if self.nodes[i].x.abs() > limit_x {
                let sign = self.nodes[i].x.signum();
                self.nodes[i].x = sign * limit_x;
                if self.vels[i].x * sign > 0.0 {
                    self.vels[i].x *= -0.35;
                }
            }
            if self.nodes[i].y.abs() > limit_y {
                let sign = self.nodes[i].y.signum();
                self.nodes[i].y = sign * limit_y;
                if self.vels[i].y * sign > 0.0 {
                    self.vels[i].y *= -0.35;
                }
            }
        }
    }

    pub fn shift(&mut self, delta: Vec2) {
        for n in &mut self.nodes {
            *n += delta;
        }
    }

    pub fn metabolize(&mut self, dt: f32) {
        let synapses = self.brain.synapse_count() as f32;
        let neurons = self.brain.neuron_count() as f32;
        let age_tax = ((self.age - AGE_TAX_START) / AGE_TAX_START).clamp(0.0, 1.0) * 0.004;
        let basal =
            self.mass() * BASAL_MASS + neurons * BASAL_NEURON + synapses * BASAL_SYNAPSE + age_tax;
        let motion = (self.actuators.thrust.abs() + self.actuators.turn.abs() * 0.6) * MOVE_COST;
        let signal = self.actuators.signal * SIGNAL_COST;
        let burn = basal + motion + signal;
        self.energy -= burn * dt;
        if self.damage > 0.0 {
            let heal = HEAL_RATE * dt * (0.35 + self.energy.min(1.0) * 0.4);
            self.damage = (self.damage - heal).max(0.0);
            self.energy -= HEAL_COST * dt;
        }
        self.energy = self.energy.min(MAX_ENERGY);
        self.age += dt;
        self.repro_cd = (self.repro_cd - dt).max(0.0);
        self.beat += dt
            * (1.1 + self.hunger() * 0.9 + self.actuators.thrust.abs() * 1.6 + self.damage * 0.8);
    }

    /// Visible heartbeat. Faster when hungry, thrusting, or hurt; weaker when empty or damaged.
    pub fn heart(&self) -> f32 {
        let amp = (0.22 + 0.78 * (self.energy / MAX_ENERGY).clamp(0.0, 1.0))
            * (1.0 - self.damage.clamp(0.0, 1.0) * 0.55);
        (self.beat * std::f32::consts::TAU).sin() * amp
    }

    pub fn grow(&mut self, dt: f32) {
        let room = self.genome.morph.setpoint + 0.18;
        if self.actuators.growth > 0.58 && self.energy > room && self.scale < 1.2 {
            self.energy -= 0.035 * dt;
            self.scale = (self.scale + 0.012 * dt).min(1.2);
        }
    }

    pub fn reward(&mut self, dt: f32) {
        let hunger = self.hunger();
        let pain = self.damage.clamp(0.0, 1.0);
        let morph = &self.genome.morph;
        let novelty = (self.novelty_excess - 0.04).max(0.0);
        let r = morph.hunger_w * (self.prev_hunger - hunger)
            + morph.pain_w * (self.prev_pain - pain)
            + morph.novelty_w * novelty * NOVELTY_SCALE
            + morph.birth_w * self.birth_pulse * (0.35 + self.repro_need())
            + self.env_reward;
        let delta = (r - self.expected).clamp(-1.0, 1.0);
        self.pulse = delta;
        let expect_a = 1.0 - (-dt / EXPECT_TAU).exp();
        let tonic_a = 1.0 - (-dt / TONIC_TAU).exp();
        self.expected += (r - self.expected) * expect_a;
        self.tonic = (self.tonic + (delta - self.tonic) * tonic_a).clamp(-1.0, 1.0);
        self.prev_hunger = hunger;
        self.prev_pain = pain;
        self.birth_pulse = 0.0;
        self.env_reward = 0.0;
        let eta = morph.learn * (-self.age / PLASTIC_TIME).exp();
        self.brain.learn(delta, eta, dt);
    }

    pub fn gain(&mut self, energy: f32) {
        self.energy = (self.energy + energy).min(MAX_ENERGY);
    }

    pub fn spend(&mut self, energy: f32) {
        self.energy -= energy;
    }

    pub fn hurt(&mut self, damage: f32, energy_loss: f32) {
        self.damage = (self.damage + damage).min(2.0);
        self.energy -= energy_loss;
    }

    fn sense_novelty(&mut self, inputs: &[f32], dt: f32) {
        if self.trace.len() != inputs.len() {
            self.trace = inputs.to_vec();
            self.novelty_excess = 0.0;
            return;
        }
        let mut surprise = 0.0;
        for (s, t) in inputs.iter().zip(self.trace.iter()) {
            surprise += (s - t).abs();
        }
        surprise /= inputs.len().max(1) as f32;
        let a = 1.0 - (-dt / 0.45).exp();
        for (s, t) in inputs.iter().zip(self.trace.iter_mut()) {
            *t += (*s - *t) * a;
        }
        self.surprise_expected += (surprise - self.surprise_expected) * a;
        self.novelty_excess = surprise - self.surprise_expected;
    }
}

pub fn hunger_of(energy: f32, setpoint: f32) -> f32 {
    (1.0 - energy / setpoint.max(0.2)).clamp(0.0, 1.6)
}

pub struct Senses {
    pub half_x: f32,
    pub half_y: f32,
    pub food_kinds: [f32; 3],
    pub food_tail: f32,
    /// Taste-weighted food aim dotted with body axis / side (signed, later mapped to `[0,1]`).
    pub food_fwd: f32,
    pub food_side: f32,
    pub pheromone: f32,
    pub kin: [f32; 3],
    pub similar: f32,
    pub density: f32,
    pub grad_fwd: f32,
    pub grad_side: f32,
}

pub fn lay_inputs(org: &Organism, sense: &Senses) -> Vec<f32> {
    let unit = |x: f32| x.clamp(0.0, 1.0);
    let signed = |x: f32| (x.clamp(-1.0, 1.0) * 0.5 + 0.5).clamp(0.0, 1.0);
    let pos = org.centroid();
    let hx = sense.half_x.max(1e-3);
    let hy = sense.half_y.max(1e-3);
    let mut inputs = Vec::with_capacity(org.genome.morph.sensor_count());
    inputs.push(unit(pos.x / hx * 0.5 + 0.5));
    inputs.push(unit(pos.y / hy * 0.5 + 0.5));
    inputs.push(unit(sense.similar));
    inputs.push(unit((hx - pos.x.abs()) / hx));
    inputs.push(unit((hy - pos.y.abs()) / hy));
    inputs.push(unit(sense.pheromone / 0.05));
    inputs.push(unit(sense.density));
    inputs.push(signed(sense.grad_fwd));
    inputs.push(signed(sense.grad_side));
    let span = GENERATION_STEPS as f32 / 60.0;
    inputs.push(unit(org.age / span));
    inputs.push(unit(
        0.5 + 0.5 * (org.osc_phase * std::f32::consts::TAU).sin(),
    ));
    for &smell in &sense.food_kinds {
        inputs.push(unit(smell));
    }
    let food_head = sense
        .food_kinds
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    inputs.push(signed(food_head - sense.food_tail));
    inputs.push(signed(sense.food_fwd));
    inputs.push(signed(sense.food_side));
    inputs.push(unit(sense.kin[0]));
    inputs.push(unit(sense.kin[1]));
    inputs.push(unit(sense.kin[2]));
    for t in org.touch {
        inputs.push(unit(t));
    }
    inputs.push(unit(org.energy / MAX_ENERGY));
    inputs.push(unit(org.damage));
    let rest = (org.rest * org.scale).max(1e-4);
    for i in 0..org.nodes.len().saturating_sub(1) {
        let len = (org.nodes[i + 1] - org.nodes[i]).length();
        inputs.push(unit((len / rest).clamp(0.0, 2.0) * 0.5));
    }
    debug_assert_eq!(inputs.len(), org.genome.morph.sensor_count());
    inputs
}

pub fn random_axis(rng: &mut impl Rng) -> Vec2 {
    let ang = rng.gen_range(0.0..std::f32::consts::TAU);
    Vec2::new(ang.cos(), ang.sin())
}
