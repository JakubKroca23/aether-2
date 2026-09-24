//! Custom GLSL materials for world visuals (water, soft glow, bloom).

use macroquad::prelude::*;
use macroquad::window::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
varying lowp vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    uv = texcoord;
}"#;

const WATER_FRAG: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform float time;
uniform float aspect;
uniform vec2 light_uv;
uniform float light_rad;
uniform float light_aspect;
uniform float dish_count;
uniform vec4 dish0;
uniform vec4 dish1;
uniform vec4 dish2;
uniform vec4 dish3;
uniform float dish_corner;
uniform vec2 cam_offset;
uniform float cam_zoom;

void main() {
    vec2 local = (uv - 0.5) * 2.0;
    vec2 world;
    if (cam_zoom > 0.01) {
        world = vec2(local.x * max(aspect, 0.25), -local.y) / cam_zoom + cam_offset;
    } else {
        world = vec2(local.x * max(aspect, 0.25), -local.y);
    }

    // Jemný radiální gradient hloubky
    float edge = max(abs(local.x), abs(local.y));
    float rim = smoothstep(0.85, 1.15, edge) * 0.35;
    float depth = rim * rim;

    vec3 deep = vec3(0.024, 0.018, 0.055);
    vec3 mid = vec3(0.062, 0.048, 0.135);
    vec3 col = mix(deep, mid, (1.0 - depth) * 0.7);

    // Rychlé proudění tekutiny (rychlé analytické vlny místo těžkého procedurálního šumu)
    vec2 w1 = world * 1.5;
    float t1 = time * 0.35;
    float wave1 = sin(w1.x + t1) * cos(w1.y * 0.8 + t1 * 0.7);
    float wave2 = sin(w1.x * 0.7 - w1.y * 1.1 - t1 * 0.5);
    float fluid = wave1 * 0.5 + wave2 * 0.5;

    // Decentní modrofialová bioluminiscence v tekutině
    vec3 violet_flow = vec3(0.075, 0.048, 0.160);
    col += violet_flow * (0.35 + 0.35 * fluid) * (1.0 - depth * 0.5);

    // Odlehčená optická kaustika tekutiny
    vec2 q = world * 2.2;
    float caust = sin(q.x * 2.5 + q.y * 1.8 + time * 0.75) * cos(q.x * 1.7 - q.y * 2.3 - time * 0.6);
    caust = caust * caust; // hladké zjasnění hřebenů vln
    col += vec3(0.48, 0.42, 0.92) * caust * 0.060 * (1.0 - depth * 0.6);

    // Kurzorem ovládané prosvětlení / průzračnost
    if (light_rad > 0.001) {
        vec2 duv = (uv - light_uv) * vec2(max(light_aspect, 0.25), 1.0);
        float d = length(duv) / max(light_rad, 1e-4);
        if (d < 3.0) {
            float clear_l = exp(-d * d * 1.6);
            vec3 clear_water = vec3(0.12, 0.09, 0.24);
            col = mix(col, clear_water, clear_l * 0.45);
        }
    }

    col = mix(col, mid * 0.85, rim * 0.4);
    float vig = 1.0 - 0.08 * (edge * edge * edge);
    col *= vig;

    gl_FragColor = vec4(col, 1.0);
}"#;

const GLOW_FRAG: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform vec4 glow_color;
uniform float power;
uniform float hole;
uniform float core;

void main() {
    vec2 p = uv - vec2(0.5);
    float t = length(p) / 0.5;
    float a = pow(clamp(1.0 - t, 0.0, 1.0), max(power, 0.2));
    if (hole >= 0.0) {
        float clear = smoothstep(hole, hole + 0.22, t);
        a *= clear * clear;
    }
    float c = core * pow(clamp(1.0 - t * 2.8, 0.0, 1.0), 1.6);
    vec3 rgb = glow_color.rgb + vec3(c);
    float alpha = (a * glow_color.a + c * 0.85) * (1.0 - step(1.02, t));
    if (alpha < 0.002) discard;
    gl_FragColor = vec4(rgb, alpha);
}"#;

const BLOOM_FRAG: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform vec2 texel;
uniform float intensity;

float lum(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

vec3 bright(vec3 c) {
    float l = lum(c);
    return c * smoothstep(0.42, 0.85, l);
}

void main() {
    vec3 scene = texture2D(Texture, uv).rgb;

    vec3 b = vec3(0.0);
    float wsum = 0.0;
    // Wide soft bloom kernel
    for (int y = -2; y <= 2; y++) {
        for (int x = -2; x <= 2; x++) {
            float w = 1.0 / (1.0 + float(x * x + y * y));
            vec2 o = vec2(float(x), float(y)) * texel * 2.2;
            b += bright(texture2D(Texture, uv + o).rgb) * w;
            wsum += w;
        }
    }
    b /= max(wsum, 1e-3);

    vec3 outc = scene + b * intensity;
    // Mild tonemap
    outc = outc / (outc + vec3(0.85)) * 1.12;
    gl_FragColor = vec4(outc, 1.0);
}"#;

fn alpha_blend() -> PipelineParams {
    PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    }
}

fn load_glsl(fragment: &str, uniforms: Vec<UniformDesc>) -> Option<Material> {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment,
        },
        MaterialParams {
            uniforms,
            pipeline_params: alpha_blend(),
            ..Default::default()
        },
    )
    .ok()
}

pub struct Gfx {
    water: Material,
    glow: Material,
    bloom: Material,
    scene: RenderTarget,
    scene_w: u32,
    scene_h: u32,
    in_world: bool,
}

impl Gfx {
    pub fn try_new() -> Option<Self> {
        let water = load_glsl(
            WATER_FRAG,
            vec![
                UniformDesc::new("time", UniformType::Float1),
                UniformDesc::new("aspect", UniformType::Float1),
                UniformDesc::new("light_uv", UniformType::Float2),
                UniformDesc::new("light_rad", UniformType::Float1),
                UniformDesc::new("light_aspect", UniformType::Float1),
                UniformDesc::new("dish_count", UniformType::Float1),
                UniformDesc::new("dish0", UniformType::Float4),
                UniformDesc::new("dish1", UniformType::Float4),
                UniformDesc::new("dish2", UniformType::Float4),
                UniformDesc::new("dish3", UniformType::Float4),
                UniformDesc::new("dish_corner", UniformType::Float1),
                UniformDesc::new("cam_offset", UniformType::Float2),
                UniformDesc::new("cam_zoom", UniformType::Float1),
            ],
        )?;
        let glow = load_glsl(
            GLOW_FRAG,
            vec![
                UniformDesc::new("glow_color", UniformType::Float4),
                UniformDesc::new("power", UniformType::Float1),
                UniformDesc::new("hole", UniformType::Float1),
                UniformDesc::new("core", UniformType::Float1),
            ],
        )?;
        let bloom = load_glsl(
            BLOOM_FRAG,
            vec![
                UniformDesc::new("texel", UniformType::Float2),
                UniformDesc::new("intensity", UniformType::Float1),
            ],
        )?;

        let scene_w = screen_width().max(1.0) as u32;
        let scene_h = screen_height().max(1.0) as u32;
        let scene = render_target(scene_w, scene_h);
        scene.texture.set_filter(FilterMode::Linear);

        Some(Self {
            water,
            glow,
            bloom,
            scene,
            scene_w,
            scene_h,
            in_world: false,
        })
    }

    fn ensure_size(&mut self, sw: f32, sh: f32) {
        let w = sw.max(1.0) as u32;
        let h = sh.max(1.0) as u32;
        if w != self.scene_w || h != self.scene_h {
            self.scene = render_target(w, h);
            self.scene.texture.set_filter(FilterMode::Linear);
            self.scene_w = w;
            self.scene_h = h;
        }
    }

    /// Begin world pass into the scene render target (screen-space coords).
    pub fn begin_world(&mut self, sw: f32, sh: f32) {
        self.ensure_size(sw, sh);
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, sw, sh));
        cam.render_target = Some(self.scene.clone());
        set_camera(&cam);
        self.in_world = true;
    }

    /// Composite bloom to the default framebuffer and leave default material active.
    pub fn present(&mut self, sw: f32, sh: f32) {
        if self.in_world {
            set_default_camera();
            self.in_world = false;
        }
        clear_background(Color::new(0.02, 0.045, 0.055, 1.0));
        gl_use_material(&self.bloom);
        self.bloom
            .set_uniform("texel", vec2(1.0 / sw.max(1.0), 1.0 / sh.max(1.0)));
        self.bloom.set_uniform("intensity", 0.55f32);
        draw_texture_ex(
            &self.scene.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(sw, sh)),
                // Render targets are stored bottom-up in GL; flip when presenting.
                flip_y: true,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }

    pub fn draw_water(
        &self,
        min_x: f32,
        min_y: f32,
        dw: f32,
        dh: f32,
        time: f32,
        aspect: f32,
        light: Option<((f32, f32), f32)>,
        dishes: &[[f32; 4]],
        dish_corner: f32,
        cam_offset: (f32, f32),
        cam_zoom: f32,
    ) {
        gl_use_material(&self.water);
        self.water.set_uniform("time", time);
        self.water.set_uniform("aspect", aspect.max(0.25));
        let (luv, lrad, laspect) = if let Some(((lx, ly), radius_px)) = light {
            let u = ((lx - min_x) / dw.max(1.0)).clamp(0.0, 1.0);
            let v = ((ly - min_y) / dh.max(1.0)).clamp(0.0, 1.0);
            let rad = (radius_px / dh.max(1.0)).clamp(0.02, 1.5);
            (vec2(u, v), rad, (dw / dh.max(1.0)).max(0.25))
        } else {
            (vec2(-1.0, -1.0), 0.0f32, 1.0f32)
        };
        self.water.set_uniform("light_uv", luv);
        self.water.set_uniform("light_rad", lrad);
        self.water.set_uniform("light_aspect", laspect);

        let count = dishes.len().min(4) as f32;
        self.water.set_uniform("dish_count", count);
        let dummy = vec4(0.0, 0.0, 0.0, 0.0);
        let d0 = dishes.get(0).map(|r| vec4(r[0], r[1], r[2], r[3])).unwrap_or(dummy);
        let d1 = dishes.get(1).map(|r| vec4(r[0], r[1], r[2], r[3])).unwrap_or(dummy);
        let d2 = dishes.get(2).map(|r| vec4(r[0], r[1], r[2], r[3])).unwrap_or(dummy);
        let d3 = dishes.get(3).map(|r| vec4(r[0], r[1], r[2], r[3])).unwrap_or(dummy);
        self.water.set_uniform("dish0", d0);
        self.water.set_uniform("dish1", d1);
        self.water.set_uniform("dish2", d2);
        self.water.set_uniform("dish3", d3);
        self.water.set_uniform("dish_corner", dish_corner);
        self.water.set_uniform("cam_offset", vec2(cam_offset.0, cam_offset.1));
        self.water.set_uniform("cam_zoom", cam_zoom);

        draw_rectangle(min_x, min_y, dw, dh, WHITE);
        gl_use_default_material();
    }

    /// Soft radial glow. `hole` < 0 disables center-clear; `core` adds hot center.
    pub fn soft_blob(
        &self,
        cx: f32,
        cy: f32,
        outer: f32,
        color: Color,
        power: f32,
        hole: f32,
        core: f32,
    ) {
        if outer < 0.5 || color.a < 0.002 {
            return;
        }
        self.begin_glow();
        self.soft_blob_cont(cx, cy, outer, color, power, hole, core);
        self.end_glow();
    }

    /// Bind the glow material once, then emit many [`soft_blob_cont`] draws.
    pub fn begin_glow(&self) {
        gl_use_material(&self.glow);
    }

    pub fn end_glow(&self) {
        gl_use_default_material();
    }

    /// Like [`soft_blob`], but assumes [`begin_glow`] is already active.
    pub fn soft_blob_cont(
        &self,
        cx: f32,
        cy: f32,
        outer: f32,
        color: Color,
        power: f32,
        hole: f32,
        core: f32,
    ) {
        if outer < 0.5 || color.a < 0.002 {
            return;
        }
        let size = outer * 2.0;
        self.glow
            .set_uniform("glow_color", vec4(color.r, color.g, color.b, color.a));
        self.glow.set_uniform("power", power);
        self.glow.set_uniform("hole", hole);
        self.glow.set_uniform("core", core);
        draw_rectangle(cx - outer, cy - outer, size, size, WHITE);
    }
}
