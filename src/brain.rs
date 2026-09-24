use serde::{Deserialize, Serialize};

use crate::genome::Genome;

/// Feed-forward net with a one-step memory on inner neurons.
/// Inner neurons and wires that cannot reach an action are pruned.
#[derive(Clone, Serialize, Deserialize)]
pub struct Brain {
    sensors: usize,
    inners: usize,
    actions: usize,
    from: Vec<usize>,
    to: Vec<usize>,
    weights: Vec<f32>,
    inner_y: Vec<f32>,
    last_in: Vec<f32>,
    last_out: Vec<f32>,
    elig: Vec<f32>,
    /// Scratch for one-step memory read during `step` (not saved).
    #[serde(skip)]
    prev_inner: Vec<f32>,
    /// Persistent accumulators — avoid heap allocs in the hot loop.
    #[serde(skip)]
    acc_i: Vec<f32>,
    #[serde(skip)]
    acc_a: Vec<f32>,
    /// Display order: connected sensors, useful inners, driven actions.
    show_kind: Vec<u8>,
    show_src: Vec<usize>,
    show_of: Vec<Option<usize>>,
}

impl Brain {
    pub fn from_genome(genome: &Genome) -> Self {
        let sensors = genome.morph.sensor_count();
        let inners = (genome.inner_neurons as usize).clamp(1, 127);
        let actions = genome.morph.action_count();
        let mut from = Vec::new();
        let mut to = Vec::new();
        let mut weights = Vec::new();
        for gene in &genome.genes {
            let src = if gene.source_type() == 0 {
                gene.source_id() as usize % sensors
            } else {
                sensors + gene.source_id() as usize % inners
            };
            let dst = if gene.sink_type() == 0 {
                sensors + gene.sink_id() as usize % inners
            } else {
                sensors + inners + gene.sink_id() as usize % actions
            };
            if src == dst {
                continue;
            }
            from.push(src);
            to.push(dst);
            weights.push(gene.weight());
        }
        let mut useful = vec![false; inners];
        let mut grew = true;
        while grew {
            grew = false;
            for (&src, &dst) in from.iter().zip(to.iter()) {
                let sink_ok = dst >= sensors + inners
                    || ((sensors..sensors + inners).contains(&dst) && useful[dst - sensors]);
                if sink_ok && (sensors..sensors + inners).contains(&src) && !useful[src - sensors] {
                    useful[src - sensors] = true;
                    grew = true;
                }
            }
        }
        let mut keep = Vec::new();
        for i in 0..from.len() {
            let dst = to[i];
            let sink_ok = dst >= sensors + inners
                || ((sensors..sensors + inners).contains(&dst) && useful[dst - sensors]);
            if sink_ok {
                keep.push(i);
            }
        }
        from = keep.iter().map(|&i| from[i]).collect();
        to = keep.iter().map(|&i| to[i]).collect();
        weights = keep.iter().map(|&i| weights[i]).collect();

        let mut show_of = vec![None; sensors + inners + actions];
        let mut show_kind = Vec::new();
        let mut show_src = Vec::new();
        let mut seen = vec![false; sensors + inners + actions];
        for &idx in from.iter().chain(to.iter()) {
            if seen[idx] {
                continue;
            }
            seen[idx] = true;
        }
        for i in 0..sensors {
            if !seen[i] {
                continue;
            }
            show_of[i] = Some(show_kind.len());
            show_kind.push(0);
            show_src.push(i);
        }
        for i in 0..inners {
            let idx = sensors + i;
            if !seen[idx] {
                continue;
            }
            show_of[idx] = Some(show_kind.len());
            show_kind.push(1);
            show_src.push(idx);
        }
        for i in 0..actions {
            let idx = sensors + inners + i;
            if !seen[idx] {
                continue;
            }
            show_of[idx] = Some(show_kind.len());
            show_kind.push(2);
            show_src.push(idx);
        }

        Self {
            sensors,
            inners,
            actions,
            from,
            to,
            weights,
            inner_y: vec![0.0; inners],
            last_in: vec![0.0; sensors],
            last_out: vec![0.0; actions],
            elig: Vec::new(),
            prev_inner: vec![0.0; inners],
            acc_i: vec![0.0; inners],
            acc_a: vec![0.0; actions],
            show_kind,
            show_src,
            show_of,
        }
    }

    pub fn finish(&mut self) {
        self.elig.resize(self.weights.len(), 0.0);
        self.ensure_scratch();
    }

    fn ensure_scratch(&mut self) {
        if self.prev_inner.len() != self.inners {
            self.prev_inner.resize(self.inners, 0.0);
        }
        if self.acc_i.len() != self.inners {
            self.acc_i.resize(self.inners, 0.0);
        }
        if self.acc_a.len() != self.actions {
            self.acc_a.resize(self.actions, 0.0);
        }
        if self.last_out.len() != self.actions {
            self.last_out.resize(self.actions, 0.0);
        }
    }

    pub fn neuron_count(&self) -> usize {
        self.show_kind.len()
    }

    pub fn synapse_count(&self) -> usize {
        self.weights.len()
    }

    pub fn kinds(&self) -> Vec<u8> {
        self.show_kind.clone()
    }

    pub fn labels(&self) -> Vec<String> {
        self.show_kind
            .iter()
            .zip(self.show_src.iter())
            .map(|(&kind, &src)| match kind {
                0 => sensor_label(src).to_string(),
                2 => action_label(src.saturating_sub(self.sensors + self.inners)).to_string(),
                _ => format!("H{}", src.saturating_sub(self.sensors) + 1),
            })
            .collect()
    }

    pub fn activation(&self, index: usize) -> f32 {
        let Some(&src) = self.show_src.get(index) else {
            return 0.0;
        };
        self.value_at(src)
    }

    pub fn wires(&self) -> Vec<(usize, usize, f32)> {
        self.from
            .iter()
            .zip(self.to.iter())
            .zip(self.weights.iter())
            .filter_map(|((from, to), weight)| {
                let from = self.show_of.get(*from).copied().flatten()?;
                let to = self.show_of.get(*to).copied().flatten()?;
                Some((from, to, *weight))
            })
            .collect()
    }

    fn value_at(&self, idx: usize) -> f32 {
        if idx < self.sensors {
            self.last_in.get(idx).copied().unwrap_or(0.0)
        } else if idx < self.sensors + self.inners {
            self.inner_y.get(idx - self.sensors).copied().unwrap_or(0.0)
        } else {
            self.last_out
                .get(idx - self.sensors - self.inners)
                .copied()
                .unwrap_or(0.0)
        }
    }

    /// Sensors stay in `[0, 1]`. Inner and action neurons are `tanh` of the weighted sum.
    pub fn step(&mut self, inputs: &[f32], responsiveness: f32) {
        let gain = responsiveness.clamp(0.15, 2.5);
        self.ensure_scratch();
        self.last_in.clear();
        self.last_in
            .extend(inputs.iter().take(self.sensors).map(|v| v.clamp(0.0, 1.0)));
        self.last_in.resize(self.sensors, 0.0);

        self.prev_inner.copy_from_slice(&self.inner_y);
        self.acc_i.fill(0.0);
        // Pass 1: sensors + previous inners → new inners only.
        for s in 0..self.weights.len() {
            let src = self.from[s];
            let signal = if src < self.sensors {
                self.last_in[src]
            } else if src < self.sensors + self.inners {
                self.prev_inner[src - self.sensors]
            } else {
                continue;
            };
            let dst = self.to[s];
            if (self.sensors..self.sensors + self.inners).contains(&dst) {
                self.acc_i[dst - self.sensors] += self.weights[s] * signal;
            }
        }
        for i in 0..self.inners {
            self.inner_y[i] = (self.acc_i[i] * gain).tanh();
        }
        // Pass 2: sensors + fresh inners → actions.
        self.acc_a.fill(0.0);
        for s in 0..self.weights.len() {
            let src = self.from[s];
            let signal = if src < self.sensors {
                self.last_in[src]
            } else if src < self.sensors + self.inners {
                self.inner_y[src - self.sensors]
            } else {
                continue;
            };
            let dst = self.to[s];
            if dst >= self.sensors + self.inners {
                let a = dst - self.sensors - self.inners;
                if a < self.actions {
                    self.acc_a[a] += self.weights[s] * signal;
                }
            }
        }
        for a in 0..self.actions {
            self.last_out[a] = (self.acc_a[a] * gain).tanh();
        }
    }

    pub fn output(&self, index: usize) -> f32 {
        self.last_out.get(index).copied().unwrap_or(0.0)
    }

    pub fn outputs(&self) -> &[f32] {
        &self.last_out
    }

    #[cfg(test)]
    pub(crate) fn set_first_weight(&mut self, value: f32) {
        self.weights[0] = value;
    }

    #[cfg(test)]
    pub(crate) fn first_weight(&self) -> f32 {
        self.weights[0]
    }

    #[cfg(test)]
    pub(crate) fn has_weights(&self) -> bool {
        !self.weights.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn max_diff(&self, other: &Brain) -> f32 {
        self.weights
            .iter()
            .zip(other.weights.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }

    pub fn learn(&mut self, delta: f32, eta: f32, dt: f32) {
        use crate::tune::{DW_PER_SEC, ELIG_TAU};
        if !delta.is_finite() || eta <= 0.0 || self.weights.is_empty() {
            return;
        }
        if self.elig.len() != self.weights.len() {
            self.elig.resize(self.weights.len(), 0.0);
        }
        let decay = (-dt / ELIG_TAU).exp();
        let delta = delta.clamp(-1.0, 1.0);
        let cap = DW_PER_SEC * dt;
        for s in 0..self.weights.len() {
            let pre = self.value_at(self.from[s]);
            let post = self.value_at(self.to[s]);
            self.elig[s] = self.elig[s] * decay + pre * post;
            let dw = (eta * delta * self.elig[s]).clamp(-cap, cap);
            self.weights[s] = (self.weights[s] + dw).clamp(-4.0, 4.0);
        }
    }
}

fn sensor_label(i: usize) -> &'static str {
    match i {
        0 => "pozice X",
        1 => "pozice Y",
        2 => "podobní",
        3 => "okraj X",
        4 => "okraj Y",
        5 => "feromon",
        6 => "hustota",
        7 => "grad vpřed",
        8 => "grad stranou",
        9 => "věk",
        10 => "oscilátor",
        11 => "vůně zelené",
        12 => "vůně zlaté",
        13 => "vůně jedu",
        14 => "vůně Δ",
        15 => "kin L",
        16 => "kin C",
        17 => "kin P",
        18 => "dotek L",
        19 => "dotek P",
        20 => "dotek D",
        21 => "dotek H",
        22 => "energie",
        23 => "bolest",
        _ => "sval/smysl",
    }
}

fn action_label(i: usize) -> &'static str {
    match i {
        0 => "pohyb X",
        1 => "pohyb Y",
        2 => "vpřed",
        3 => "náhodný",
        4 => "východ",
        5 => "západ",
        6 => "sever",
        7 => "jih",
        8 => "feromon",
        9 => "citlivost",
        10 => "tempo osc",
        11 => "útok",
        12 => "enzym",
        13 => "potomek",
        14 => "růst",
        _ => "sval",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Gene, Genome};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn wired(weight: i16, sink_action: bool) -> Genome {
        let mut rng = StdRng::seed_from_u64(1);
        let mut genome = Genome::random(&mut rng);
        let mut data = 0u32;
        data |= 0 << 31;
        data |= 0 << 24;
        data |= u32::from(sink_action) << 23;
        data |= 0 << 16;
        data |= (weight as u16) as u32;
        genome.genes = vec![Gene { data }];
        genome.inner_neurons = 2;
        genome
    }

    #[test]
    fn birth_weights_match_innate_not_learned() {
        let genome = wired(8000, true);
        let mut lived = Brain::from_genome(&genome);
        lived.finish();
        assert!(lived.has_weights());
        let innate = lived.first_weight();
        assert!((innate - 1.0).abs() < 1e-5);
        lived.set_first_weight(innate + 1.4);
        let born = Brain::from_genome(&genome);
        assert!((born.first_weight() - innate).abs() < 1e-5);
        assert!((born.first_weight() - lived.first_weight()).abs() > 1.0);
    }

    #[test]
    fn positive_pulse_moves_coactive_synapse() {
        let genome = wired(8000, true);
        let mut brain = Brain::from_genome(&genome);
        brain.finish();
        let inputs = vec![1.0; genome.morph.sensor_count()];
        for _ in 0..8 {
            brain.step(&inputs, 1.0);
        }
        let before = brain.first_weight();
        brain.learn(1.0, 0.8, 1.0 / 60.0);
        assert!(
            (brain.first_weight() - before).abs() > 1e-6,
            "dopamine should nudge a synapse that just fired"
        );
    }

    #[test]
    fn dead_end_inner_wire_is_pruned() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut genome = Genome::random(&mut rng);
        let mut data = 0u32;
        data |= 1 << 31;
        data |= 1 << 16;
        data |= 8000u32;
        genome.genes = vec![Gene { data }];
        genome.inner_neurons = 3;
        let brain = Brain::from_genome(&genome);
        assert_eq!(brain.synapse_count(), 0);
    }

    #[test]
    fn action_output_is_tanh() {
        let genome = wired(8000, true);
        let mut brain = Brain::from_genome(&genome);
        brain.step(&vec![1.0; genome.morph.sensor_count()], 1.0);
        let expect = 1.0f32.tanh();
        assert!((brain.output(0) - expect).abs() < 1e-4);
    }

    #[test]
    fn sensor_input_is_clamped() {
        let genome = wired(8000, true);
        let mut brain = Brain::from_genome(&genome);
        let mut inputs = vec![0.0; genome.morph.sensor_count()];
        inputs[0] = 4.0;
        brain.step(&inputs, 1.0);
        assert!((brain.activation(0) - 1.0).abs() < 1e-5);
    }
}
