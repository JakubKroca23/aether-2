//! Procedural audio — mystical organic sci-fi ambience and UI/world SFX.

use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};

const SR: u32 = 22_050;

/// Skip procedural WAV synth at startup (was ~38s ambience + SFX → multi-second freeze).
const LOAD_AUDIO: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Ui,
    Confirm,
    Cancel,
    Transit,
    Feeder,
    Eat,
    Birth,
    Death,
}

#[derive(Clone, Copy, Debug)]
pub struct Mix {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            // Temporary: silence until audio mix is tuned.
            master: 0.0,
            music: 0.55,
            sfx: 0.7,
        }
    }
}

pub struct AudioHub {
    mix: Mix,
    music: Sound,
    music_on: bool,
    ui: Sound,
    confirm: Sound,
    cancel: Sound,
    transit: Sound,
    feeder: Sound,
    eat: Sound,
    birth: Sound,
    death: Sound,
}

impl AudioHub {
    pub async fn boot() -> Self {
        if !LOAD_AUDIO {
            // Tiny silent placeholders — keeps the API, avoids startup hitch.
            let quiet = encode_wav(&[0.0f32; 256]);
            return Self {
                mix: Mix::default(),
                music: load_wav(&quiet).await,
                music_on: false,
                ui: load_wav(&quiet).await,
                confirm: load_wav(&quiet).await,
                cancel: load_wav(&quiet).await,
                transit: load_wav(&quiet).await,
                feeder: load_wav(&quiet).await,
                eat: load_wav(&quiet).await,
                birth: load_wav(&quiet).await,
                death: load_wav(&quiet).await,
            };
        }
        let hub = Self {
            mix: Mix::default(),
            music: load_wav(&render_ambience(38.0)).await,
            music_on: false,
            ui: load_wav(&render_ui_tick()).await,
            confirm: load_wav(&render_confirm()).await,
            cancel: load_wav(&render_cancel()).await,
            transit: load_wav(&render_transit()).await,
            feeder: load_wav(&render_feeder()).await,
            eat: load_wav(&render_eat()).await,
            birth: load_wav(&render_birth()).await,
            death: load_wav(&render_death()).await,
        };
        hub
    }

    pub fn mix(&self) -> Mix {
        self.mix
    }

    pub fn set_mix(&mut self, mix: Mix) {
        self.mix = Mix {
            master: mix.master.clamp(0.0, 1.0),
            music: mix.music.clamp(0.0, 1.0),
            sfx: mix.sfx.clamp(0.0, 1.0),
        };
        self.apply_music_volume();
    }

    pub fn nudge_master(&mut self, d: f32) {
        let mut m = self.mix;
        m.master = (m.master + d).clamp(0.0, 1.0);
        self.set_mix(m);
    }

    pub fn nudge_music(&mut self, d: f32) {
        let mut m = self.mix;
        m.music = (m.music + d).clamp(0.0, 1.0);
        self.set_mix(m);
    }

    pub fn nudge_sfx(&mut self, d: f32) {
        let mut m = self.mix;
        m.sfx = (m.sfx + d).clamp(0.0, 1.0);
        self.set_mix(m);
    }

    fn music_gain(&self) -> f32 {
        (self.mix.master * self.mix.music * 0.72).clamp(0.0, 1.0)
    }

    fn sfx_gain(&self) -> f32 {
        (self.mix.master * self.mix.sfx).clamp(0.0, 1.0)
    }

    fn apply_music_volume(&self) {
        set_sound_volume(&self.music, self.music_gain());
    }

    pub fn ensure_music(&mut self, want: bool) {
        if want && !self.music_on && self.music_gain() > 0.01 {
            play_sound(
                &self.music,
                PlaySoundParams {
                    looped: true,
                    volume: self.music_gain(),
                },
            );
            self.music_on = true;
        } else if (!want || self.music_gain() <= 0.01) && self.music_on {
            stop_sound(&self.music);
            self.music_on = false;
        } else if self.music_on {
            self.apply_music_volume();
        }
    }

    pub fn play(&self, kind: Sfx) {
        let g = self.sfx_gain();
        if g < 0.01 {
            return;
        }
        let sound = match kind {
            Sfx::Ui => &self.ui,
            Sfx::Confirm => &self.confirm,
            Sfx::Cancel => &self.cancel,
            Sfx::Transit => &self.transit,
            Sfx::Feeder => &self.feeder,
            Sfx::Eat => &self.eat,
            Sfx::Birth => &self.birth,
            Sfx::Death => &self.death,
        };
        let vol = match kind {
            Sfx::Ui => 0.35,
            Sfx::Confirm => 0.55,
            Sfx::Cancel => 0.4,
            Sfx::Transit => 0.65,
            Sfx::Feeder => 0.5,
            Sfx::Eat => 0.45,
            Sfx::Birth => 0.5,
            Sfx::Death => 0.55,
        } * g;
        play_sound(
            sound,
            PlaySoundParams {
                looped: false,
                volume: vol.clamp(0.0, 1.0),
            },
        );
    }
}

async fn load_wav(bytes: &[u8]) -> Sound {
    load_sound_from_bytes(bytes)
        .await
        .expect("procedural wav load")
}

fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let n = samples.len() as u32;
    let data_bytes = n * 2;
    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SR.to_le_bytes());
    out.extend_from_slice(&(SR * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

fn hash_noise(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn soft_noise(t: f32, seed: u32) -> f32 {
    let f = t * SR as f32;
    let i = f.floor() as u32;
    let u = f - f.floor();
    let a = hash_noise(i.wrapping_add(seed));
    let b = hash_noise(i.wrapping_add(seed).wrapping_add(1));
    let u = u * u * (3.0 - 2.0 * u);
    a + (b - a) * u
}

fn sine(freq: f32, t: f32) -> f32 {
    (std::f32::consts::TAU * freq * t).sin()
}

fn tri(freq: f32, t: f32) -> f32 {
    let p = frac(freq * t);
    if p < 0.5 {
        4.0 * p - 1.0
    } else {
        3.0 - 4.0 * p
    }
}

fn env_ads(t: f32, a: f32, d: f32, s: f32, len: f32) -> f32 {
    if t < 0.0 || t > len {
        return 0.0;
    }
    if t < a {
        return (t / a.max(1e-4)).clamp(0.0, 1.0);
    }
    if t < a + d {
        let u = (t - a) / d.max(1e-4);
        return 1.0 + (s - 1.0) * u;
    }
    let release = (len - t).min(0.35) / 0.35;
    s * release.clamp(0.0, 1.0)
}

fn env_perc(t: f32, attack: f32, release: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    if t < attack {
        return t / attack.max(1e-4);
    }
    ((1.0 - (t - attack) / release.max(1e-4)).max(0.0)).powf(1.6)
}

/// Long mystical pad: deep drones, soft organ shimmer, organic breath noise.
fn render_ambience(secs: f32) -> Vec<u8> {
    let n = (secs * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    let roots = [55.0, 82.5, 110.0, 164.8]; // A1-ish mystical stack
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let breath = 0.55 + 0.45 * (t * 0.07).sin() * (t * 0.031).sin();
        let mut s = 0.0;

        // Sub drone
        s += sine(roots[0], t) * 0.22;
        s += sine(roots[0] * 1.002, t + 0.17) * 0.12;

        // Warm fifth / organ
        s += sine(roots[1], t) * 0.14 * breath;
        s += tri(roots[2], t) * 0.05 * breath;
        s += sine(roots[2] * 0.997, t) * 0.08;

        // High glassy shimmer (detuned)
        let shimmer = 0.5 + 0.5 * (t * 0.11).sin();
        s += sine(roots[3], t) * 0.035 * shimmer;
        s += sine(roots[3] * 1.498, t) * 0.02 * shimmer;
        s += sine(330.0 + 8.0 * (t * 0.2).sin(), t) * 0.015;

        // Organic filtered noise bed
        let noise = soft_noise(t, 17) * 0.045 + soft_noise(t * 0.5, 91) * 0.03;
        s += noise * (0.35 + 0.65 * breath);

        // Slow mystical pulse
        let pulse = (0.5 + 0.5 * (t * std::f32::consts::TAU / 7.5).sin()).powf(2.4);
        s += sine(220.0, t) * 0.03 * pulse;
        s += sine(146.8, t) * 0.04 * pulse;

        // Soft harmonic swell every ~12s
        let swell_t = (t % 12.0) / 12.0;
        let swell = (swell_t * std::f32::consts::PI).sin().powf(2.0);
        s += sine(440.0 * 0.75, t) * 0.025 * swell;

        // Gentle edge fade for seamless loop
        let edge = (t / 1.2).min(1.0).min(((secs - t) / 1.2).max(0.0));
        buf[i] = (s * 0.72 * edge).tanh() * 0.9;
    }
    // Crossfade ends for cleaner loop
    let fade = (SR as usize / 2).min(n / 4);
    for i in 0..fade {
        let w = i as f32 / fade as f32;
        let a = buf[i];
        let b = buf[n - fade + i];
        let m = a * w + b * (1.0 - w);
        buf[i] = m;
        buf[n - fade + i] = m;
    }
    encode_wav(&buf)
}

fn render_ui_tick() -> Vec<u8> {
    let n = (0.09 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_perc(t, 0.004, 0.08);
        let s = sine(740.0 + t * 180.0, t) * 0.35 + sine(1480.0, t) * 0.12;
        let click = soft_noise(t, 3) * 0.08 * env_perc(t, 0.001, 0.02);
        buf[i] = (s * e + click).tanh() * 0.7;
    }
    encode_wav(&buf)
}

fn render_confirm() -> Vec<u8> {
    let n = (0.45 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e1 = env_perc(t, 0.01, 0.28);
        let e2 = env_perc(t - 0.06, 0.01, 0.32);
        let s = sine(392.0, t) * 0.28 * e1
            + sine(523.25, t) * 0.22 * e2
            + sine(784.0, t) * 0.1 * e2
            + soft_noise(t, 5) * 0.04 * e1;
        buf[i] = s.tanh() * 0.75;
    }
    encode_wav(&buf)
}

fn render_cancel() -> Vec<u8> {
    let n = (0.22 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_perc(t, 0.006, 0.2);
        let s = sine(320.0 - t * 220.0, t) * 0.3 + sine(180.0, t) * 0.12;
        buf[i] = (s * e).tanh() * 0.65;
    }
    encode_wav(&buf)
}

fn render_transit() -> Vec<u8> {
    let n = (1.5 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_ads(t, 0.18, 0.4, 0.55, 1.5);
        let sweep = 90.0 + 260.0 * (t / 1.5);
        let s = sine(sweep, t) * 0.22
            + sine(sweep * 1.5, t) * 0.1
            + sine(55.0, t) * 0.18
            + soft_noise(t, 11) * 0.07 * (0.4 + 0.6 * (t * 2.0).sin().abs());
        buf[i] = (s * e).tanh() * 0.8;
    }
    encode_wav(&buf)
}

fn render_feeder() -> Vec<u8> {
    let n = (0.55 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_perc(t, 0.02, 0.45);
        let bubble = sine(140.0 + 40.0 * (t * 18.0).sin(), t) * 0.25;
        let drip = sine(520.0 - t * 180.0, t) * 0.15 * env_perc(t, 0.01, 0.2);
        let wet = soft_noise(t, 21) * 0.1 * e;
        buf[i] = ((bubble + drip + wet) * e).tanh() * 0.7;
    }
    encode_wav(&buf)
}

fn render_eat() -> Vec<u8> {
    let n = (0.28 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_perc(t, 0.008, 0.22);
        let s = sine(210.0 + t * 90.0, t) * 0.2
            + soft_noise(t, 7) * 0.16 * env_perc(t, 0.002, 0.08)
            + sine(90.0, t) * 0.12;
        buf[i] = (s * e).tanh() * 0.7;
    }
    encode_wav(&buf)
}

fn render_birth() -> Vec<u8> {
    let n = (0.7 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_ads(t, 0.04, 0.2, 0.4, 0.7);
        let s = sine(440.0, t) * 0.12
            + sine(554.37, t) * 0.1
            + sine(659.25, t) * 0.08 * env_perc(t - 0.08, 0.02, 0.4)
            + soft_noise(t, 13) * 0.03;
        buf[i] = (s * e).tanh() * 0.7;
    }
    encode_wav(&buf)
}

fn render_death() -> Vec<u8> {
    let n = (0.85 * SR as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let e = env_ads(t, 0.02, 0.25, 0.35, 0.85);
        let s = sine(180.0 - t * 90.0, t) * 0.22
            + sine(90.0, t) * 0.16
            + soft_noise(t, 29) * 0.08 * (1.0 - t / 0.85)
            + sine(55.0, t) * 0.1;
        buf[i] = (s * e).tanh() * 0.75;
    }
    encode_wav(&buf)
}
