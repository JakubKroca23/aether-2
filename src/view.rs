use aether::{
    delete_save, list_saves, load_simulation, open_default, save_simulation, Appearance,
    EdgeEffect, EdgeZone, Feeder, Flash, FoodKind, FoodSpec, Net, PetriDish, SaveMeta, Spark, Stats,
    Vec2, World,
};
use macroquad::prelude::*;

use crate::audio::{AudioHub, Mix, Sfx};
use crate::gfx::Gfx;

const FONT_PATH: &str = "/usr/share/fonts/opentype/fira/FiraSans-Medium.otf";
const LOGO_FONT_PATH: &str = "/usr/share/fonts/opentype/fira/FiraSans-Heavy.otf";
/// Bottom tool dock stays screen-fixed; dish chrome is in world units below.
const TOOL: f32 = 84.0;

const W_LIFE_BTN_W: f32 = 0.52;
const W_LIFE_BTN_H: f32 = 0.11;
const W_DISH_CORNER: f32 = 0.18;
/// Shared flat chrome panel fill (header + census).
const CHROME_FILL: Color = Color::new(0.02, 0.05, 0.07, 0.92);
const CHROME_EDGE: Color = Color::new(0.28, 0.9, 0.82, 0.4);

struct Frame {
    sw: f32,
    sh: f32,
}

fn frame_of(sw: f32, sh: f32) -> Frame {
    Frame { sw, sh }
}

#[derive(Clone, Copy)]
struct Cam {
    center: Vec2,
    zoom: f32,
}

impl Cam {
    fn identity() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

struct Mote {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    hue: f32,
}

struct Fade {
    id: u64,
    hover: f32,
    pin: f32,
}

#[derive(Default)]
struct ToolHovers {
    spawn: f32,
    pause: f32,
    settings: f32,
    saves: f32,
    food: f32,
    dish: f32,
    info: f32,
    speed: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Running,
}

struct ScreenTransit {
    to: Phase,
    t: f32,
    flipped: bool,
    /// If set, spawn a fresh world at the midpoint of the transition.
    start_seed: Option<u64>,
}

#[derive(Clone)]
struct BootConfig {
    population: usize,
    start_food: usize,
    /// Dish half-extents in world units (independent of window).
    dish_half_x: f32,
    dish_half_y: f32,
    food_kinds: [FoodSpec; 3],
    viscosity: f32,
    edges: [EdgeZone; 4],
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            population: 0,
            start_food: 0,
            dish_half_x: 1.0,
            dish_half_y: 1.0,
            food_kinds: FoodSpec::defaults(),
            viscosity: 1.0,
            edges: [EdgeZone::default(); 4],
        }
    }
}

impl BootConfig {
    fn from_world(world: &World) -> Self {
        let (hx, hy) = world.bounds();
        Self {
            population: world.alive().min(96),
            start_food: world.food_count().min(48),
            dish_half_x: hx.clamp(0.45, 4.0),
            dish_half_y: hy.clamp(0.45, 4.0),
            food_kinds: *world.food_kinds(),
            viscosity: world.viscosity(),
            edges: world.edges(),
        }
    }

    fn apply(&self, world: &mut World) {
        for kind in FoodKind::ALL {
            world.set_food_spec(kind, self.food_kinds[kind.index()]);
        }
        world.set_viscosity(self.viscosity);
        world.set_edges(self.edges);
        world.set_bounds(self.dish_half_x, self.dish_half_y);
    }
}

pub async fn run() {
    let shot = std::env::args().any(|a| a == "--shot");
    let mut seed = if shot {
        7
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
    };
    let mut boot = BootConfig::default();
    let mut world = if shot {
        World::new_with(seed, 16, 6)
    } else {
        World::new_with(seed, 0, 0)
    };
    boot.apply(&mut world);
    let font = load_ttf_font(FONT_PATH).await.ok();
    let logo_font = load_ttf_font(LOGO_FONT_PATH).await.ok();
    let mut gfx = Gfx::try_new();
    if gfx.is_none() {
        eprintln!("aether: custom shaders unavailable, using CPU draw fallback");
    }
    let mut audio = AudioHub::boot().await;
    let mut bloom_on = true;
    set_fullscreen(false);

    let mut phase = if shot { Phase::Running } else { Phase::Title };
    let mut title_msg: Option<(String, f32)> = None;
    let mut title_settings_open = false;
    let mut paused = false;
    let mut pinned: Option<u64> = None;
    let mut feed_tool = false;
    let mut dish_tool = false;
    let mut feed_kind = FoodKind::Green;
    let mut selected_feeder: Option<usize> = None;
    let mut food_panel_open = false;
    let mut food_edit: u8 = 0;
    let mut life_setup_open = false;
    let mut life_setup_dish: Option<u32> = None;
    let mut life_pop: usize = 8;
    let mut life_food: usize = 6;
    let mut life_hx: f32 = 1.0;
    let mut life_hy: f32 = 1.0;
    let mut life_btn_hover = 0.0f32;
    let mut settings_open = false;
    let mut settings_section: u8 = 0;
    let mut saves_open = false;
    let mut save_list: Vec<SaveMeta> = Vec::new();
    let mut save_name = String::new();
    let mut save_selected: Option<i64> = None;
    let mut save_name_focus = false;
    let mut save_status: Option<(String, f32)> = None;
    let save_db = open_default().ok();
    if let Some(conn) = save_db.as_ref() {
        if let Ok(rows) = list_saves(conn) {
            save_list = rows;
        }
    }
    let mut fullscreen = false;
    let mut speed = 1u32;
    let mut speed_open = false;
    let mut home = Cam::identity();
    let mut cam = Cam::identity();
    // Coasting pan velocity in world units / sec (fling inertia).
    let mut pan_coast = Vec2::ZERO;
    // Coasting log-zoom velocity `d(ln zoom)/dt`.
    let mut zoom_coast = 0.0_f32;
    let mut dish_hover_t = 0.0_f32;
    let mut pointer: Option<((f32, f32), (f32, f32), bool, MouseButton)> = None;
    let mut detail_hit: Option<(f32, f32, f32, f32)> = None;
    let mut net_hit: Option<(f32, f32, f32, f32)> = None;
    let mut net_detail_btn: Option<(f32, f32, f32, f32)> = None;
    let mut net_3d_open = false;
    let mut net_3d_yaw = 0.55_f32;
    let mut net_3d_pitch = 0.38_f32;
    let mut net_3d_dist = 5.2_f32;
    let mut net_3d_drag: Option<(f32, f32)> = None;
    let mut motes: Vec<Mote> = Vec::new();
    let mut fades: Vec<Fade> = Vec::new();
    let mut tool_hovers = ToolHovers::default();
    let mut shown: Option<Stats> = None;
    let mut panel_t = 0.0f32;
    let mut acc = 0.0f32;
    let mut following = false;
    let mut follow_id: Option<u64> = None;
    let mut inspect_zoom = 3.0f32;
    let mut detail_tab = DetailTab::Info;
    let mut detail_hits: Option<DetailHits> = None;
    let mut ambience_cursor = (640.0_f32, 320.0_f32);
    let mut ambience_cursor_vel = (0.0_f32, 0.0_f32);
    let mut ambience_roamers = TitleRoamer::spawn(1280.0, 800.0);
    let mut screen_transit: Option<ScreenTransit> = None;
    let mut hover_new = 0.0_f32;
    let mut hover_load = 0.0_f32;
    let mut hover_settings = 0.0_f32;
    let mut hover_quit = 0.0_f32;
    let mut prev_births = world.census().births;
    let mut prev_deaths = world.census().deaths;
    let mut prev_food = world.census().food;

    loop {
        let sw = screen_width().max(1.0);
        let sh = screen_height().max(1.0);
        let frame = frame_of(sw, sh);
        let mouse = mouse_position();
        let pressed = is_mouse_button_pressed(MouseButton::Left);

        if let Some((_, t)) = save_status.as_mut() {
            *t -= get_frame_time().min(0.05);
            if *t <= 0.0 {
                save_status = None;
            }
        }
        if let Some((_, t)) = title_msg.as_mut() {
            *t -= get_frame_time().min(0.05);
            if *t <= 0.0 {
                title_msg = None;
            }
        }

        let dt_ui = get_frame_time().min(0.05);
        let input_locked = screen_transit.is_some();
        let (mut transit_scale, mut transit_alpha, mut transit_blur) =
            (1.0_f32, 1.0_f32, 0.0_f32);
        if let Some(tr) = screen_transit.as_mut() {
            tr.t = (tr.t + dt_ui / 1.15).min(1.0);
            let (s, a, b) = if tr.to == Phase::Running {
                transit_visual_new_game(tr.t)
            } else {
                transit_visual(tr.t)
            };
            transit_scale = s;
            transit_alpha = a;
            transit_blur = b;
            // Spawn while the veil is already dark — hitch is hidden, flip stays instant.
            if tr.start_seed.is_some() && tr.t >= 0.38 {
                if let Some(seed) = tr.start_seed.take() {
                    world = World::new_with(seed, boot.population, boot.start_food);
                    let aspect = (frame.sw / frame.sh.max(1.0)).clamp(0.25, 4.0);
                    world.set_dish_bounds(aspect, 1.0);
                    boot.apply(&mut world);
                }
            }
            if !tr.flipped && tr.t >= 0.5 {
                tr.flipped = true;
                if tr.to == Phase::Running {
                    reset_run_ui(
                        &mut paused,
                        &mut pinned,
                        &mut feed_tool,
                        &mut dish_tool,
                        &mut feed_kind,
                        &mut selected_feeder,
                        &mut settings_open,
                        &mut settings_section,
                        &mut food_panel_open,
                        &mut shown,
                        &mut panel_t,
                        &mut detail_hit,
                        &mut net_hit,
                        &mut motes,
                        &mut fades,
                        &mut home,
                        &mut cam,
                        &mut following,
                        &mut follow_id,
                        &mut inspect_zoom,
                        &mut pan_coast,
                        &mut zoom_coast,
                        &frame,
                        world.bounds().0,
                        world.bounds().1,
                    );
                    dish_hover_t = 0.0;
                    acc = 0.0;
                }
                if tr.to == Phase::Title {
                    if let Some(conn) = save_db.as_ref() {
                        if let Ok(rows) = list_saves(conn) {
                            save_list = rows;
                        }
                    }
                    saves_open = false;
                    save_name_focus = false;
                    hover_new = 0.0;
                    hover_load = 0.0;
                }
                phase = tr.to;
            }
            if tr.t >= 1.0 {
                screen_transit = None;
                transit_scale = 1.0;
                transit_alpha = 1.0;
                transit_blur = 0.0;
            }
        }

        if phase == Phase::Title {
            audio.ensure_music(true);
            let ui = title_layout(&frame);
            let saves_ui = if saves_open {
                Some(saves_panel_layout(&frame, save_list.len(), false))
            } else {
                None
            };
            let on_saves = saves_ui
                .as_ref()
                .is_some_and(|s| hit_rect(mouse, s.panel));
            let on_title_settings = title_settings_open
                && hit_rect(mouse, ui.settings_panel);

            // Soft organic cursor follow — drives logo / UI parallax.
            let prev = ambience_cursor;
            ambience_cursor.0 = damp(ambience_cursor.0, mouse.0, dt_ui, 0.28);
            ambience_cursor.1 = damp(ambience_cursor.1, mouse.1, dt_ui, 0.28);
            ambience_cursor_vel.0 = damp(
                ambience_cursor_vel.0,
                (ambience_cursor.0 - prev.0) / dt_ui.max(1e-3),
                dt_ui,
                0.2,
            );
            ambience_cursor_vel.1 = damp(
                ambience_cursor_vel.1,
                (ambience_cursor.1 - prev.1) / dt_ui.max(1e-3),
                dt_ui,
                0.2,
            );

            if save_name_focus && saves_open {
                while let Some(ch) = get_char_pressed() {
                    if !ch.is_control() && save_name.chars().count() < 40 {
                        save_name.push(ch);
                    }
                }
                if is_key_pressed(KeyCode::Backspace) {
                    save_name.pop();
                }
            } else {
                while get_char_pressed().is_some() {}
            }

            if pressed && !input_locked {
                if saves_open {
                    if let Some(sui) = saves_ui.as_ref() {
                        if hit_rect(mouse, sui.close) || !on_saves {
                            saves_open = false;
                            save_name_focus = false;
                        } else {
                            for (i, row) in sui.rows.iter().enumerate().take(save_list.len()) {
                                if hit_rect(mouse, *row) {
                                    save_selected = Some(save_list[i].id);
                                    save_name = save_list[i].name.clone();
                                }
                            }
                            if hit_rect(mouse, sui.load) {
                                if let (Some(conn), Some(id)) =
                                    (save_db.as_ref(), save_selected)
                                {
                                    match load_simulation(conn, id) {
                                        Ok(loaded) => {
                                            world = loaded;
                                            boot = BootConfig::from_world(&world);
                                            reset_run_ui(
                                                &mut paused,
                                                &mut pinned,
                                                &mut feed_tool,
                                                &mut dish_tool,
                                                &mut feed_kind,
                                                &mut selected_feeder,
                                                &mut settings_open,
                                                &mut settings_section,
                                                &mut food_panel_open,
                                                &mut shown,
                                                &mut panel_t,
                                                &mut detail_hit,
                                                &mut net_hit,
                                                &mut motes,
                                                &mut fades,
                                                &mut home,
                                                &mut cam,
                                                &mut following,
                                                &mut follow_id,
                                                &mut inspect_zoom,
                                                &mut pan_coast,
                                                &mut zoom_coast,
                                                &frame,
                                                world.bounds().0,
                                                world.bounds().1,
                                            );
                                            dish_hover_t = 0.0;
                                            saves_open = false;
                                            save_name_focus = false;
                                            title_msg = None;
                                            audio.play(Sfx::Transit);
                                            begin_screen_transit(
                                                &mut screen_transit,
                                                Phase::Running,
                                                None,
                                            );
                                        }
                                        Err(e) => {
                                            save_status = Some((e.to_string(), 3.0));
                                        }
                                    }
                                } else {
                                    save_status =
                                        Some(("Vyber uložení ze seznamu.".into(), 2.5));
                                }
                            } else if hit_rect(mouse, sui.delete) {
                                if let (Some(conn), Some(id)) =
                                    (save_db.as_ref(), save_selected)
                                {
                                    match delete_save(conn, id) {
                                        Ok(()) => {
                                            save_selected = None;
                                            if let Ok(rows) = list_saves(conn) {
                                                save_list = rows;
                                            }
                                            save_status = Some(("Smazáno.".into(), 2.0));
                                        }
                                        Err(e) => {
                                            save_status = Some((e.to_string(), 3.0));
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if title_settings_open && on_title_settings {
                    audio.play(Sfx::Ui);
                    if hit_rect(mouse, ui.fullscreen) {
                        fullscreen = !fullscreen;
                        set_fullscreen(fullscreen);
                    } else if hit_rect(mouse, ui.bloom) {
                        bloom_on = !bloom_on;
                    } else if hit_rect(mouse, ui.master_minus) {
                        audio.nudge_master(-0.05);
                    } else if hit_rect(mouse, ui.master_plus) {
                        audio.nudge_master(0.05);
                    } else if hit_rect(mouse, ui.music_minus) {
                        audio.nudge_music(-0.05);
                    } else if hit_rect(mouse, ui.music_plus) {
                        audio.nudge_music(0.05);
                    } else if hit_rect(mouse, ui.sfx_minus) {
                        audio.nudge_sfx(-0.05);
                    } else if hit_rect(mouse, ui.sfx_plus) {
                        audio.nudge_sfx(0.05);
                    }
                } else if title_settings_open && !on_title_settings {
                    title_settings_open = false;
                } else if hit_rect(mouse, ui.new_sim) {
                    title_msg = None;
                    title_settings_open = false;
                    seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(seed.wrapping_add(1));
                    boot = BootConfig::default();
                    audio.play(Sfx::Transit);
                    begin_screen_transit(&mut screen_transit, Phase::Running, Some(seed));
                } else if hit_rect(mouse, ui.load_sim) {
                    audio.play(Sfx::Ui);
                    title_settings_open = false;
                    if save_db.is_none() {
                        title_msg = Some(("Databáze není dostupná.".into(), 3.2));
                    } else if let Some(conn) = save_db.as_ref() {
                        if let Ok(rows) = list_saves(conn) {
                            save_list = rows;
                        }
                        if save_list.is_empty() {
                            title_msg = Some(("Zatím žádná uložení.".into(), 3.2));
    } else {
                            saves_open = true;
                            save_name_focus = false;
                            save_selected = save_list.first().map(|s| s.id);
                            if let Some(s) = save_list.first() {
                                save_name = s.name.clone();
                            }
                        }
                    }
                } else if hit_rect(mouse, ui.settings) {
                    audio.play(Sfx::Ui);
                    title_settings_open = !title_settings_open;
                    saves_open = false;
                } else if hit_rect(mouse, ui.quit) {
                    audio.play(Sfx::Cancel);
                    miniquad::window::quit();
                }
            }
            if is_key_pressed(KeyCode::Escape) && !input_locked {
                if saves_open {
                    saves_open = false;
                    save_name_focus = false;
                } else if title_settings_open {
                    title_settings_open = false;
                }
            }
            clear_background(Color::new(0.022, 0.07, 0.09, 1.0));
            for roamer in &mut ambience_roamers {
                roamer.step(
                    sw,
                    sh,
                    ambience_cursor,
                    ambience_cursor_vel,
                    LOBBY_ATTRACT_R,
                    dt_ui,
                );
            }
            hover_new = damp(
                hover_new,
                if !input_locked
                    && hit_rect(mouse, ui.new_sim)
                {
                    1.0
    } else {
                    0.0
                },
                dt_ui,
                0.11,
            );
            hover_load = damp(
                hover_load,
                if !input_locked
                    && save_db.is_some()
                    && !save_list.is_empty()
                    && hit_rect(mouse, ui.load_sim)
                {
                    1.0
                } else {
                    0.0
                },
                dt_ui,
                0.11,
            );
            hover_settings = damp(
                hover_settings,
                if !input_locked
                    && hit_rect(mouse, ui.settings)
                {
                    1.0
                } else {
                    0.0
                },
                dt_ui,
                0.11,
            );
            hover_quit = damp(
                hover_quit,
                if !input_locked
                    && hit_rect(mouse, ui.quit)
                {
                    1.0
                } else {
                    0.0
                },
                dt_ui,
                0.11,
            );
            let t_now = get_time() as f32;
            paint_lobby_backdrop(
                gfx.as_ref(),
                &frame,
                t_now,
                Some((ambience_cursor, LOBBY_ATTRACT_R)),
            );
            paint_title_ambience(
                gfx.as_ref(),
                &frame,
                ambience_cursor,
                ambience_cursor_vel,
                &ambience_roamers,
                t_now,
                transit_scale,
                transit_alpha,
                transit_blur,
            );
            let compose = logo_compose(phase, screen_transit.as_ref());
            let burst = logo_burst(screen_transit.as_ref());
            draw_title(
                gfx.as_ref(),
                &frame,
                &font,
                &logo_font,
                ambience_cursor,
                &ui,
                t_now,
                save_db.is_some() && !save_list.is_empty(),
                title_msg.as_ref().map(|(m, _)| m.as_str()),
                hover_new,
                hover_load,
                hover_settings,
                hover_quit,
                title_settings_open,
                fullscreen,
                bloom_on,
                audio.mix(),
                transit_scale,
                transit_alpha,
                transit_blur,
                compose,
                burst,
            );
            paint_transit_veil(gfx.as_ref(), &frame, screen_transit.as_ref().map(|t| t.t));
            if let Some(sui) = saves_ui.as_ref() {
                draw_saves_panel(
                    &frame,
                    &font,
                    mouse,
                    sui,
                    &save_list,
                    save_selected,
                    &save_name,
                    save_name_focus,
                    false,
                    save_status.as_ref().map(|(m, _)| m.as_str()),
                );
            }
            next_frame().await;
            continue;
        }


        let aspect = (frame.sw / frame.sh.max(1.0)).clamp(0.25, 4.0);
        world.set_dish_bounds(aspect, 1.0);
        if shot && world.time() < 18.0 {
            let (tx, ty) = world.table_bounds();
            reset_cam_to_dish(&mut home, &mut cam, &frame, tx, ty);
            while world.time() < 18.0 {
                world.step(1.0 / 60.0);
            }
        } else if !paused && !input_locked {
            let pace = speed.max(1);
            acc += get_frame_time().min(0.05) * pace as f32;
            let mut steps = 0;
            let cap = (pace as i32).clamp(4, 200) as i32;
            while acc >= 1.0 / 60.0 && steps < cap {
                world.step(1.0 / 60.0);
                acc -= 1.0 / 60.0;
                steps += 1;
            }
            if steps > 0 {
                let c = world.census();
                if c.births > prev_births {
                    audio.play(Sfx::Birth);
                }
                if c.deaths > prev_deaths {
                    audio.play(Sfx::Death);
                }
                if c.food < prev_food {
                    audio.play(Sfx::Eat);
                }
                prev_births = c.births;
                prev_deaths = c.deaths;
                prev_food = c.food;
            }
        }

        audio.ensure_music(true);
        if selected_feeder.is_some_and(|i| i >= world.feeders().len()) {
            selected_feeder = None;
        }

        let pressed = is_mouse_button_pressed(MouseButton::Left);
        let (bound_hx, bound_hy) = world.bounds();
        let (chrome_hx, chrome_hy, chrome_center) = (bound_hx, bound_hy, Vec2::ZERO);
        let bar = control_bar_layout(
            &frame,
            &cam,
            chrome_center,
            chrome_hx,
            chrome_hy,
            speed_open,
        );
        let (food_x, food_y, food_w, food_h) = food_button_rect(&frame);
        let on_food = hit(mouse, food_x, food_y, food_w, food_h);
        let (dish_x, dish_y, dish_w, dish_h) = dish_button_rect(&frame);
        let on_dish = hit(mouse, dish_x, dish_y, dish_w, dish_h);
        let (spawn_x, spawn_y, spawn_w, spawn_h) = spawn_button_rect(&frame);
        let on_spawn = hit(mouse, spawn_x, spawn_y, spawn_w, spawn_h);
        let on_pause = hit_rect(mouse, bar.pause);
        let on_saves_btn = hit_rect(mouse, bar.saves);
        let on_dish_settings = hit_rect(mouse, bar.settings);
        let (census_x, census_y, census_w, census_h) =
            census_rect(&frame, &cam, chrome_center, chrome_hx, chrome_hy, true);
        let on_census = hit(mouse, census_x, census_y, census_w, census_h);
        let life_ui = if life_setup_open {
            Some(life_setup_layout(&frame))
        } else {
            None
        };
        let on_life_setup = life_ui
            .as_ref()
            .is_some_and(|u| hit_rect(mouse, u.panel));
        let mut on_empty_life = false;
        let mut empty_life_target: Option<u32> = None;
        if !life_setup_open {
            for dish in world.dishes() {
                if !world.dish_is_empty(dish.id) {
                    continue;
                }
                let rect =
                    empty_life_button_rect(&frame, &cam, dish.pos, dish.half_x, dish.half_y);
                if hit_rect(mouse, rect) {
                    on_empty_life = true;
                    empty_life_target = Some(dish.id);
                    break;
                }
            }
        }
        let speed_ui = speed_menu(
            &frame,
            &cam,
            chrome_center,
            chrome_hx,
            chrome_hy,
            speed_open,
        );
        let on_speed = hit_rect(mouse, speed_ui.main)
            || (speed_open && speed_ui.options.iter().any(|r| hit_rect(mouse, *r)));
        let setup = settings_menu(&frame, bar.settings, settings_open, settings_section);
        let on_setup = settings_open && hit_rect(mouse, setup.panel);
        let saves_ui = if saves_open {
            Some(saves_panel_layout(&frame, save_list.len(), true))
        } else {
            None
        };
        let on_saves_panel = saves_ui
            .as_ref()
            .is_some_and(|s| hit_rect(mouse, s.panel));
        let feeder_anchor = selected_feeder
            .and_then(|idx| world.feeders().get(idx))
            .map(|f| world_to_screen(&frame, &cam, f.pos));
        let food_ui = food_panel(&frame, feeder_anchor, food_panel_open);
        let on_food_panel = food_panel_open && hit_rect(mouse, food_ui.panel);
        let on_detail = detail_hit.is_some_and(|r| hit_rect(mouse, r));
        let on_net = net_hit.is_some_and(|r| hit_rect(mouse, r));
        let on_net_detail = net_detail_btn.is_some_and(|r| hit_rect(mouse, r));
        let on_tools = on_food
            || on_dish
            || on_spawn
            || on_pause
            || on_saves_btn
            || on_dish_settings
            || on_census
            || on_speed
            || on_empty_life
            || on_life_setup
            || hit_rect(mouse, bar.bar);
        let mut consumed = input_locked;

        if net_3d_open {
            let close = net_3d_close_rect(&frame);
            if is_key_pressed(KeyCode::Escape) || (pressed && hit_rect(mouse, close)) {
                net_3d_open = false;
                net_3d_drag = None;
                consumed = true;
            } else {
                let (_wx, wy) = mouse_wheel();
                if wy.abs() > 0.0 {
                    net_3d_dist =
                        (net_3d_dist * (1.0 - wy.clamp(-3.0, 3.0) * 0.08)).clamp(2.4, 14.0);
                }
                if is_mouse_button_pressed(MouseButton::Left)
                    || is_mouse_button_pressed(MouseButton::Right)
                    || is_mouse_button_pressed(MouseButton::Middle)
                {
                    net_3d_drag = Some(mouse);
                }
                if let Some(prev) = net_3d_drag {
                    if is_mouse_button_down(MouseButton::Left)
                        || is_mouse_button_down(MouseButton::Right)
                        || is_mouse_button_down(MouseButton::Middle)
                    {
                        net_3d_yaw += (mouse.0 - prev.0) * 0.008;
                        net_3d_pitch =
                            (net_3d_pitch + (mouse.1 - prev.1) * 0.006).clamp(-1.2, 1.2);
                        net_3d_drag = Some(mouse);
                    } else {
                        net_3d_drag = None;
                    }
                }
                consumed = true;
            }
        } else if pressed && !consumed && on_net_detail {
            net_3d_open = true;
            net_3d_drag = None;
            consumed = true;
        }

        if save_name_focus && saves_open && !input_locked {
            while let Some(ch) = get_char_pressed() {
                if !ch.is_control() && save_name.chars().count() < 40 {
                    save_name.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                save_name.pop();
            }
        }

        if pressed && on_saves_btn {
            saves_open = !saves_open;
            audio.play(Sfx::Ui);
            if saves_open {
                settings_open = false;
                settings_section = 0;
                food_panel_open = false;
                speed_open = false;
                if let Some(conn) = save_db.as_ref() {
                    if let Ok(rows) = list_saves(conn) {
                        save_list = rows;
                    }
                }
                if save_name.is_empty() {
                    let mins = (world.time() / 60.0).floor() as u32;
                    let secs = (world.time() % 60.0).floor() as u32;
                    save_name = format!("běh {mins}:{secs:02}");
                }
                save_name_focus = true;
            } else {
                save_name_focus = false;
            }
            consumed = true;
        } else if pressed && saves_open && on_saves_panel {
            if let Some(sui) = saves_ui.as_ref() {
                if hit_rect(mouse, sui.name_box) {
                    save_name_focus = true;
                } else if hit_rect(mouse, sui.close) {
                    saves_open = false;
                    save_name_focus = false;
                } else {
                    save_name_focus = false;
                    for (i, row) in sui.rows.iter().enumerate().take(save_list.len()) {
                        if hit_rect(mouse, *row) {
                            save_selected = Some(save_list[i].id);
                            save_name = save_list[i].name.clone();
                        }
                    }
                    if hit_rect(mouse, sui.save) {
                        if let Some(conn) = save_db.as_ref() {
                            match save_simulation(conn, &save_name, &world) {
                                Ok(id) => {
                                    save_selected = Some(id);
                                    if let Ok(rows) = list_saves(conn) {
                                        save_list = rows;
                                    }
                                    save_status = Some(("Uloženo.".into(), 2.2));
                                }
                                Err(e) => {
                                    save_status = Some((e.to_string(), 3.0));
                                }
                            }
            } else {
                            save_status = Some(("Databáze není dostupná.".into(), 3.0));
                        }
                    } else if hit_rect(mouse, sui.load) {
                        if let (Some(conn), Some(id)) = (save_db.as_ref(), save_selected) {
                            match load_simulation(conn, id) {
                                Ok(loaded) => {
                                    world = loaded;
                                    boot = BootConfig::from_world(&world);
                                    reset_run_ui(
                                        &mut paused,
                                        &mut pinned,
                                        &mut feed_tool,
                                        &mut dish_tool,
                                        &mut feed_kind,
                                        &mut selected_feeder,
                                        &mut settings_open,
                                        &mut settings_section,
                                        &mut food_panel_open,
                                        &mut shown,
                                        &mut panel_t,
                                        &mut detail_hit,
                                        &mut net_hit,
                                        &mut motes,
                                        &mut fades,
                                        &mut home,
                                        &mut cam,
                                        &mut following,
                                        &mut follow_id,
                                        &mut inspect_zoom,
                                        &mut pan_coast,
                                        &mut zoom_coast,
                                        &frame,
                                        world.bounds().0,
                                        world.bounds().1,
                                    );
                                    dish_hover_t = 0.0;
                                    saves_open = false;
                                    save_name_focus = false;
                                    save_status = Some(("Načteno.".into(), 2.2));
                                }
                                Err(e) => {
                                    save_status = Some((e.to_string(), 3.0));
                                }
                            }
        } else {
                            save_status = Some(("Vyber uložení ze seznamu.".into(), 2.5));
                        }
                    } else if hit_rect(mouse, sui.delete) {
                        if let (Some(conn), Some(id)) = (save_db.as_ref(), save_selected) {
                            match delete_save(conn, id) {
                                Ok(()) => {
                                    save_selected = None;
                                    if let Ok(rows) = list_saves(conn) {
                                        save_list = rows;
                                    }
                                    save_status = Some(("Smazáno.".into(), 2.0));
                                }
                                Err(e) => {
                                    save_status = Some((e.to_string(), 3.0));
                                }
                            }
                        }
                    }
                }
            }
            consumed = true;
        } else if pressed && saves_open && !on_saves_panel && !on_saves_btn {
            saves_open = false;
            save_name_focus = false;
            consumed = true;
        } else if pressed && settings_open && on_setup {
            audio.play(Sfx::Ui);
            if hit_rect(mouse, setup.cat_obraz) {
                settings_section = if settings_section == 1 { 0 } else { 1 };
            } else if hit_rect(mouse, setup.cat_zvuk) {
                settings_section = if settings_section == 2 { 0 } else { 2 };
            } else if hit_rect(mouse, setup.cat_prostredi) {
                settings_section = if settings_section == 3 { 0 } else { 3 };
            } else if hit_rect(mouse, setup.fullscreen) {
                fullscreen = !fullscreen;
                set_fullscreen(fullscreen);
            } else if hit_rect(mouse, setup.bloom) {
                bloom_on = !bloom_on;
            } else if hit_rect(mouse, setup.master_minus) {
                audio.nudge_master(-0.05);
            } else if hit_rect(mouse, setup.master_plus) {
                audio.nudge_master(0.05);
            } else if hit_rect(mouse, setup.music_minus) {
                audio.nudge_music(-0.05);
            } else if hit_rect(mouse, setup.music_plus) {
                audio.nudge_music(0.05);
            } else if hit_rect(mouse, setup.sfx_minus) {
                audio.nudge_sfx(-0.05);
            } else if hit_rect(mouse, setup.sfx_plus) {
                audio.nudge_sfx(0.05);
            } else if hit_rect(mouse, setup.viscosity_minus) {
                world.set_viscosity(world.viscosity() - 0.1);
            } else if hit_rect(mouse, setup.viscosity_plus) {
                world.set_viscosity(world.viscosity() + 0.1);
            } else {
                for i in 0..4 {
                    if hit_rect(mouse, setup.edge_effect[i]) {
                        world.cycle_edge_effect(i);
                    } else if hit_rect(mouse, setup.edge_reach_minus[i]) {
                        world.set_edge_reach(i, world.edges()[i].reach - 0.05);
                    } else if hit_rect(mouse, setup.edge_reach_plus[i]) {
                        world.set_edge_reach(i, world.edges()[i].reach + 0.05);
                    }
                }
            }
            consumed = true;
        } else if pressed && settings_open && !on_setup && !on_dish_settings {
            settings_open = false;
            settings_section = 0;
            consumed = true;
        } else if pressed && food_panel_open && on_food_panel {
            audio.play(Sfx::Ui);
            let mut picked = food_edit;
            if hit_kind_tab(mouse, &food_ui.kind_icons, &mut picked) {
                food_edit = picked;
                feed_kind = FoodKind::from_index(picked as usize);
                if let Some(idx) = selected_feeder {
                    world.set_feeder_kind(idx, feed_kind);
                }
            } else if hit_rect(mouse, food_ui.close) {
                selected_feeder = None;
                food_panel_open = false;
            } else if hit_rect(mouse, food_ui.feeder_enable) {
                if let Some(idx) = selected_feeder {
                    let on = world
                        .feeders()
                        .get(idx)
                        .map(|f| !f.enabled)
                        .unwrap_or(true);
                    world.set_feeder_enabled(idx, on);
                }
            } else if hit_rect(mouse, food_ui.feeder_delete) {
                if let Some(idx) = selected_feeder {
                    world.remove_feeder(idx);
                    selected_feeder = None;
                    food_panel_open = false;
                }
            } else if hit_rect(mouse, food_ui.food_rate_minus) {
                if let Some(idx) = selected_feeder {
                    let r = world.feeders().get(idx).map(|f| f.rate).unwrap_or(1.0);
                    let step = if r > 1.05 { 0.5 } else { 0.1 };
                    world.set_feeder_rate(idx, (r - step).max(0.1));
                }
            } else if hit_rect(mouse, food_ui.food_rate_plus) {
                if let Some(idx) = selected_feeder {
                    let r = world.feeders().get(idx).map(|f| f.rate).unwrap_or(1.0);
                    let step = if r >= 1.0 { 0.5 } else { 0.1 };
                    world.set_feeder_rate(idx, (r + step).min(10.0));
                }
            } else if hit_rect(mouse, food_ui.food_radius_minus) {
                if let Some(idx) = selected_feeder {
                    let r = world.feeders().get(idx).map(|f| f.radius).unwrap_or(0.35);
                    world.set_feeder_radius(idx, (r - 0.05).max(0.10));
                }
            } else if hit_rect(mouse, food_ui.food_radius_plus) {
                if let Some(idx) = selected_feeder {
                    let r = world.feeders().get(idx).map(|f| f.radius).unwrap_or(0.35);
                    world.set_feeder_radius(idx, (r + 0.05).min(2.00));
                }
            } else if hit_rect(mouse, food_ui.food_sense_minus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_sense(kind, world.food_spec(kind).sense - 0.02);
            } else if hit_rect(mouse, food_ui.food_sense_plus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_sense(kind, world.food_spec(kind).sense + 0.02);
            } else if hit_rect(mouse, food_ui.food_energy_minus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_energy(kind, world.food_spec(kind).energy - 0.04);
            } else if hit_rect(mouse, food_ui.food_energy_plus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_energy(kind, world.food_spec(kind).energy + 0.04);
            } else if hit_rect(mouse, food_ui.food_harm_minus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_harm(kind, world.food_spec(kind).harm - 0.04);
            } else if hit_rect(mouse, food_ui.food_harm_plus) {
                let kind = FoodKind::from_index(food_edit as usize);
                world.set_food_harm(kind, world.food_spec(kind).harm + 0.04);
            }
            consumed = true;
        } else if pressed && food_panel_open && !on_food_panel && !on_food {
            food_panel_open = false;
            selected_feeder = None;
            consumed = true;
        }
        if pressed && !consumed {
            if let Some(h) = detail_hits.as_ref() {
                if hit_rect(mouse, h.info) {
                    detail_tab = DetailTab::Info;
                    consumed = true;
                } else if hit_rect(mouse, h.genom) {
                    detail_tab = DetailTab::Genom;
                    consumed = true;
                }
            }
        }
        if pressed && hit_rect(mouse, speed_ui.main) && !consumed {
            speed_open = !speed_open;
            consumed = true;
        } else if speed_open && pressed && !consumed {
            for (i, rect) in speed_ui.options.iter().enumerate() {
                if hit_rect(mouse, *rect) {
                    speed = SPEED_PRESETS[i].0;
                    speed_open = false;
                    consumed = true;
                    break;
                }
            }
            if !consumed && !on_speed {
                speed_open = false;
            }
        }

        if is_key_pressed(KeyCode::Space) || (pressed && on_pause && !consumed) {
            paused = !paused;
            audio.play(Sfx::Ui);
        }
        if is_key_pressed(KeyCode::Escape) && !input_locked {
            if speed_open {
                speed_open = false;
            } else if life_setup_open {
                life_setup_open = false;
                life_setup_dish = None;
            } else if saves_open {
                saves_open = false;
                save_name_focus = false;
            } else if food_panel_open {
                food_panel_open = false;
            } else if settings_open {
                settings_open = false;
                settings_section = 0;
            } else if pinned.is_some() || feed_tool || dish_tool {
                pinned = None;
                feed_tool = false;
                dish_tool = false;
            } else {
                boot = BootConfig::from_world(&world);
                audio.play(Sfx::Transit);
                begin_screen_transit(&mut screen_transit, Phase::Title, None);
            }
        }
        if pressed && on_dish_settings && !consumed {
            settings_open = !settings_open;
            audio.play(Sfx::Ui);
            if !settings_open {
                settings_section = 0;
            }
            food_panel_open = false;
            speed_open = false;
            saves_open = false;
            save_name_focus = false;
            consumed = true;
        } else if pressed && on_empty_life && !consumed {
            if let Some(id) = empty_life_target {
                life_setup_dish = Some(id);
                life_setup_open = true;
                if let Some(d) = world.dishes().iter().find(|d| d.id == id) {
                    life_hx = d.half_x;
                    life_hy = d.half_y;
                }
                life_pop = 8;
                life_food = 6;
                audio.play(Sfx::Ui);
                consumed = true;
            }
        } else if pressed && life_setup_open && !consumed {
            if let Some(ui) = life_ui.as_ref() {
                if hit_rect(mouse, ui.close) {
                    life_setup_open = false;
                    life_setup_dish = None;
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.pop_minus) {
                    life_pop = life_pop.saturating_sub(1);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.pop_plus) {
                    life_pop = (life_pop + 1).min(96);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.food_minus) {
                    life_food = life_food.saturating_sub(1);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.food_plus) {
                    life_food = (life_food + 1).min(48);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.width_minus) {
                    life_hx = (life_hx - 0.1).max(0.45);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.width_plus) {
                    life_hx = (life_hx + 0.1).min(4.0);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.height_minus) {
                    life_hy = (life_hy - 0.1).max(0.45);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.height_plus) {
                    life_hy = (life_hy + 0.1).min(4.0);
                    audio.play(Sfx::Ui);
                    consumed = true;
                } else if hit_rect(mouse, ui.start) {
                    if let Some(id) = life_setup_dish {
                        if world.seed_dish(id, life_pop, life_food, life_hx, life_hy) {
                            audio.play(Sfx::Confirm);
                            let (tx, ty) = world.table_bounds();
                            clamp_cam(&mut cam, &frame, tx, ty);
                            home = cam;
                            clear_cam_coast(&mut pan_coast, &mut zoom_coast);
                        }
                    }
                    life_setup_open = false;
                    life_setup_dish = None;
                    consumed = true;
                } else if !on_life_setup {
                    life_setup_open = false;
                    life_setup_dish = None;
                    consumed = true;
                } else {
                    consumed = true;
                }
            }
        }
        if pressed && on_food && !consumed {
            feed_tool = !feed_tool;
            audio.play(Sfx::Ui);
            if feed_tool {
                settings_open = false;
                settings_section = 0;
                speed_open = false;
                saves_open = false;
                save_name_focus = false;
                food_panel_open = false;
                selected_feeder = None;
                dish_tool = false;
            } else {
                food_panel_open = false;
            }
            consumed = true;
        }
        if pressed && on_dish && !consumed {
            dish_tool = !dish_tool;
            audio.play(Sfx::Ui);
            if dish_tool {
                feed_tool = false;
                food_panel_open = false;
                settings_open = false;
                settings_section = 0;
                speed_open = false;
                saves_open = false;
                save_name_focus = false;
                selected_feeder = None;
                pinned = None;
            }
            consumed = true;
        }
        if pressed && on_spawn && !consumed {
            audio.play(Sfx::Confirm);
            let a = world.time();
            let p = Vec2::new((a * 0.73).sin() * 0.4, (a * 0.51).cos() * 0.4);
            pinned = Some(world.spawn_at(p));
            selected_feeder = None;
            dish_tool = false;
        }

        let ui_block = on_tools
            || on_setup
            || on_food_panel
            || on_saves_panel
            || on_detail
            || on_net
            || on_net_detail
            || on_life_setup
            || life_setup_open
            || net_3d_open
            || consumed;
        let right_pressed = is_mouse_button_pressed(MouseButton::Right);
        if right_pressed {
            if food_panel_open && on_food_panel {
                let mut picked = 255u8;
                if hit_kind_tab(mouse, &food_ui.kind_icons, &mut picked) {
                    feed_tool = false;
                }
            } else if settings_open {
                settings_open = false;
                settings_section = 0;
            } else if food_panel_open {
                food_panel_open = false;
            } else {
                pointer = Some((mouse, mouse, false, MouseButton::Right));
            }
        } else if pressed && !ui_block {
            pointer = Some((mouse, mouse, false, MouseButton::Left));
        } else if is_mouse_button_pressed(MouseButton::Middle) && !ui_block {
            pointer = Some((mouse, mouse, false, MouseButton::Middle));
        }

        if let Some((origin, last, moved, button)) = pointer {
            if is_mouse_button_down(button) {
                let dist = (mouse.0 - origin.0).hypot(mouse.1 - origin.1);
                let moved = moved || dist > 3.0;
                if moved {
                    let dx = mouse.0 - last.0;
                    let dy = mouse.1 - last.1;
                    let scale = world_scale(&frame, &cam).max(1e-4);
                    let dwx = -dx / scale;
                    let dwy = dy / scale;
                    home.center.x += dwx;
                    home.center.y += dwy;
                    let (tbx, tby) = world.table_bounds();
                    clamp_cam(&mut home, &frame, tbx, tby);
                    // Track fling velocity (EMA) so release keeps a soft coast.
                    let dt_drag = get_frame_time().max(1e-4).min(0.05);
                    let instant = Vec2::new(dwx / dt_drag, dwy / dt_drag);
                    pan_coast = pan_coast * 0.42 + instant * 0.58;
                    let max_coast = 14.0 / scale.max(0.15);
                    let speed = pan_coast.length();
                    if speed > max_coast {
                        pan_coast = pan_coast * (max_coast / speed);
                    }
                    zoom_coast *= 0.85;
                }
                pointer = Some((origin, mouse, moved, button));
            }
        }
        if let Some((origin, _, moved, button)) = pointer {
            if is_mouse_button_released(button) {
                let moved = moved || (mouse.0 - origin.0).hypot(mouse.1 - origin.1) > 3.0;
                if !moved && button == MouseButton::Left {
                    if let Some(p) = screen_to_world(&frame, &cam, mouse.0, mouse.1) {
                        if dish_tool {
                            if world.try_add_dish(p).is_some() {
                                audio.play(Sfx::Confirm);
                                let (tx, ty) = world.table_bounds();
                                clamp_cam(&mut cam, &frame, tx, ty);
                                home = cam;
                                clear_cam_coast(&mut pan_coast, &mut zoom_coast);
                            } else {
                                audio.play(Sfx::Ui);
                            }
                            pinned = None;
                            selected_feeder = None;
                        } else if feed_tool {
                            if let Some(idx) = world.pick_feeder(p, 0.20) {
                                selected_feeder = Some(idx);
                                food_edit = world.feeders()[idx].kind.index() as u8;
                                feed_kind = world.feeders()[idx].kind;
                                food_panel_open = true;
                                audio.play(Sfx::Ui);
                            } else {
                                let idx = world.add_feeder(p, feed_kind);
                                selected_feeder = Some(idx);
                                food_panel_open = true;
                                feed_tool = false;
                                audio.play(Sfx::Feeder);
                            }
                            pinned = None;
                        } else if let Some(idx) = world.pick_feeder(p, 0.20) {
                            selected_feeder = Some(idx);
                            food_edit = world.feeders()[idx].kind.index() as u8;
                            feed_kind = world.feeders()[idx].kind;
                            food_panel_open = true;
                            pinned = None;
                            audio.play(Sfx::Ui);
                        } else {
                            pinned = world.pick(p);
                            selected_feeder = None;
                            food_panel_open = false;
                        }
                    }
                } else if !moved && button == MouseButton::Right {
                    pinned = None;
                    feed_tool = false;
                    dish_tool = false;
                    selected_feeder = None;
                    food_panel_open = false;
                }
                pointer = None;
            }
        }

        let fresh = pinned.and_then(|id| world.stats(id));
        if fresh.is_none() {
            pinned = None;
        }
        if let Some(s) = fresh.clone() {
            shown = Some(s);
        }

        let apps = world.appearances();
        spawn_motes(&mut motes, &apps);
        step_motes(&mut motes, 1.0 / 60.0);

        let dt = get_frame_time().min(0.05);
        tool_hovers.spawn = damp(tool_hovers.spawn, if on_spawn { 1.0 } else { 0.0 }, dt, 0.1);
        tool_hovers.pause = damp(tool_hovers.pause, if on_pause { 1.0 } else { 0.0 }, dt, 0.1);
        tool_hovers.settings = damp(
            tool_hovers.settings,
            if on_dish_settings || settings_open { 1.0 } else { 0.0 },
            dt,
            0.1,
        );
        tool_hovers.saves = damp(
            tool_hovers.saves,
            if on_saves_btn || saves_open { 1.0 } else { 0.0 },
            dt,
            0.1,
        );
        tool_hovers.info = 0.0;
        tool_hovers.speed = damp(
            tool_hovers.speed,
            if hit_rect(mouse, speed_ui.main) || speed_open {
                1.0
            } else {
                0.0
            },
            dt,
            0.1,
        );
        tool_hovers.food = damp(tool_hovers.food, if on_food { 1.0 } else { 0.0 }, dt, 0.1);
        life_btn_hover = damp(life_btn_hover, if on_empty_life { 1.0 } else { 0.0 }, dt, 0.12);
        tool_hovers.dish = damp(
            tool_hovers.dish,
            if on_dish || dish_tool { 1.0 } else { 0.0 },
            dt,
            0.1,
        );

        let dragging = pointer.is_some_and(|(_, _, moved, _)| moved);
        let (hx, hy) = world.bounds();
        let (table_hx, table_hy) = world.table_bounds();

        // Dish hover highlight on cutouts.
        let over_dish = if !dragging && !on_tools && !on_setup {
            pick_dish_id(&world, &frame, &cam, mouse)
        } else {
            None
        };
        dish_hover_t = damp(
            dish_hover_t,
            if over_dish.is_some() { 1.0 } else { 0.0 },
            dt,
            0.1,
        );

        // Wheel zoom — small steps + coast; kill coast that pushes past limits.
        let (_wx, wy) = mouse_wheel();
        if wy.abs() > 0.0
            && !ui_block
            && !settings_open
            && !saves_open
            && !food_panel_open
            && !speed_open
            && !life_setup_open
        {
            let steps = wy.clamp(-2.5, 2.5);
            // Finer steps than before.
            let factor = (1.0 + steps * 0.035).clamp(0.92, 1.09);
            // Reverse scroll cuts existing coast so max-zoom doesn't trap you.
            if steps.signum() != 0.0
                && zoom_coast.signum() != 0.0
                && steps.signum() != zoom_coast.signum()
            {
                zoom_coast *= 0.15;
            }
            zoom_coast += steps * 0.42;
            zoom_coast = zoom_coast.clamp(-5.0, 5.0);
            pan_coast = pan_coast * 0.92;
            if pinned.is_some() {
                inspect_zoom = (inspect_zoom * factor).clamp(0.08, 48.0);
                home.zoom = inspect_zoom;
            } else {
                home.zoom = (home.zoom * factor).clamp(0.04, 48.0);
            }
        }

        if let Some(id) = pinned {
            following = true;
            if follow_id != Some(id) {
                follow_id = Some(id);
                pan_coast = Vec2::ZERO;
                zoom_coast = 0.0;
                if let Some(app) = apps.iter().find(|a| a.id == id) {
                    inspect_zoom = (0.48 / app.radius.max(0.04)).clamp(2.0, 6.0);
                } else {
                    inspect_zoom = 3.0;
                }
                home.zoom = inspect_zoom;
            }
            if !dragging {
                if let Some(app) = apps.iter().find(|a| a.id == id) {
                    let mut cx = 0.0f32;
                    let mut cy = 0.0f32;
                    for n in app.world_nodes() {
                        cx += n.x;
                        cy += n.y;
                    }
                    let n = app.nodes.len().max(1) as f32;
                    home.center = Vec2::new(cx / n, cy / n);
                    home.zoom = inspect_zoom;
                    pan_coast = pan_coast * 0.8;
                }
            }
        } else if following {
            following = false;
            follow_id = None;
            home = cam;
            pan_coast = Vec2::ZERO;
            zoom_coast = 0.0;
        }

        // Integrate soft inertia into the camera target.
        if !dragging {
            home.center += pan_coast * dt;
            let pan_friction = (-dt / 0.48).exp();
            pan_coast = pan_coast * pan_friction;
            if pan_coast.length() < 1e-3 {
                pan_coast = Vec2::ZERO;
            }
        }
        if zoom_coast.abs() > 1e-5 {
            home.zoom *= (zoom_coast * dt).exp();
            let z_friction = (-dt / 0.34).exp();
            zoom_coast *= z_friction;
            if zoom_coast.abs() < 1e-4 {
                zoom_coast = 0.0;
            }
        }
        let z_lo = if pinned.is_some() { 0.08 } else { 0.04 };
        let z_hi = 48.0;
        // Don't keep coasting into a hard stop — otherwise zoom-out feels stuck.
        if home.zoom >= z_hi - 1e-3 && zoom_coast > 0.0 {
            zoom_coast = 0.0;
            home.zoom = z_hi;
        } else if home.zoom <= z_lo + 1e-4 && zoom_coast < 0.0 {
            zoom_coast = 0.0;
            home.zoom = z_lo;
        } else {
            home.zoom = home.zoom.clamp(z_lo, z_hi);
        }
        if pinned.is_some() {
            inspect_zoom = home.zoom;
        }
        clamp_cam(&mut home, &frame, table_hx, table_hy);

        // Soft follow — a touch snappier while dragging, floatier when coasting.
        let follow_tau = if dragging { 0.07 } else { 0.16 };
        cam.center = damp_vec(cam.center, home.center, dt, follow_tau);
        cam.zoom = damp(cam.zoom, home.zoom, dt, follow_tau + 0.03);
        clamp_cam(&mut cam, &frame, table_hx, table_hy);

        let hovered = if dragging || on_tools || on_setup || on_detail || on_net || dish_tool || feed_tool {
            None
        } else {
            screen_to_world(&frame, &cam, mouse.0, mouse.1).and_then(|p| world.pick(p))
        };
        step_fades(&mut fades, hovered, pinned, dt);

        clear_background(Color::new(0.015, 0.04, 0.05, 1.0));
        let use_shaders = bloom_on && gfx.is_some();
        if use_shaders {
            if let Some(g) = gfx.as_mut() {
                g.begin_world(sw, sh);
                clear_background(Color::new(0.015, 0.04, 0.05, 1.0));
            }
        }
        let paint_gfx = if use_shaders { gfx.as_ref() } else { None };
        // Table floor = lobby water, but pans/zooms with the camera, keeping green fog outside dishes.
        paint_table_backdrop(paint_gfx, &frame, &cam, table_hx, table_hy, world.time(), world.dishes());
        // Dishes = cutouts punched through the fluid.
        for dish in world.dishes() {
            let hover = if over_dish == Some(dish.id) {
                dish_hover_t
            } else {
                0.0
            };
            paint_dish_cutout(
                &frame,
                &cam,
                dish.pos,
                dish.half_x,
                dish.half_y,
                world.time(),
                hover,
            );
        }
        draw_edge_zones(&frame, &cam, hx, hy, world.edges());
        draw_feeders(
            &frame,
            &cam,
            &world,
            selected_feeder,
            feed_tool,
            feed_kind,
            mouse,
            on_tools || on_setup || on_food_panel || on_saves_panel || on_detail || on_net,
        );

        let sensor_reach = world.food_sensor_reach();
        if let Some(g) = paint_gfx {
            g.begin_glow();
            for (_dish_id, pos, food) in world.foods_table() {
                let spec = world.food_spec(food.kind);
                paint_food_glow(
                    g,
                    &frame,
                    &cam,
                    pos,
                    spec.sense,
                    spec.color,
                    food.bloom,
                    food.fade,
                );
            }
            g.end_glow();
        } else {
            for (_dish_id, pos, food) in world.foods_table() {
                let spec = world.food_spec(food.kind);
                draw_food(
                    &frame,
                    &cam,
                    pos,
                    spec.sense,
                    spec.color,
                    food.bloom,
                    food.fade,
                );
            }
        }
        for app in &apps {
            let (hover_t, pin_t) = fade_of(&fades, app.id);
            paint_organism(
                paint_gfx,
                &frame,
                &cam,
                &font,
                app,
                world.time(),
                hover_t.max(pin_t * 0.55),
            );
            paint_select_glow(paint_gfx, &frame, &cam, app, pin_t, world.time());
        }
        for spark in world.sparks() {
            paint_spark(paint_gfx, &frame, &cam, spark);
        }
        for flash in world.flashes() {
            paint_flash(paint_gfx, &frame, &cam, flash);
        }
        for mote in &motes {
            paint_mote(paint_gfx, &frame, &cam, mote);
        }
        if use_shaders {
            if let Some(g) = gfx.as_mut() {
                g.present(sw, sh);
            }
            // Sharp overlays after bloom (not baked into the RT).
            for app in &apps {
                draw_energy_bar(&frame, &cam, &font, app);
            }
        }
        // Hover rings after bloom so they stay crisp.
        for app in &apps {
            let (hover_t, _) = fade_of(&fades, app.id);
            draw_hover_ring(&frame, &cam, app, hover_t);
        }
        // Sense cones outside the bloom pass so they stay readable.
        for app in &apps {
            let (hover_t, pin_t) = fade_of(&fades, app.id);
            let sense_t = (0.55 + 0.45 * pin_t.max(hover_t * 0.55)).clamp(0.55, 1.0);
            draw_senses(&frame, &cam, app, sense_t, sensor_reach);
        }
        draw_control_bar_bg(&bar);
        draw_speed_menu(
            &frame,
            &font,
            mouse,
            speed,
            speed_open,
            &speed_ui,
            &bar,
            tool_hovers.speed,
        );
        draw_pause_button(&font, &bar, mouse, paused, tool_hovers.pause);
        draw_saves_button(&font, &bar, mouse, saves_open, tool_hovers.saves);
        draw_dish_settings_button(&font, &bar, mouse, settings_open, tool_hovers.settings);
        draw_census(
            &frame,
            &font,
            &world,
            &cam,
            chrome_center,
            chrome_hx,
            chrome_hy,
            true,
        );
        for dish in world.dishes() {
            if world.dish_is_empty(dish.id) && !life_setup_open {
                draw_empty_life_button(
                    &frame,
                    &font,
                    &cam,
                    dish.pos,
                    dish.half_x,
                    dish.half_y,
                    mouse,
                    if empty_life_target == Some(dish.id) {
                        life_btn_hover
                    } else {
                        0.0
                    },
                );
            }
        }
        draw_spawn_button(&frame, &font, mouse, tool_hovers.spawn);
        if let Some(ui) = life_ui.as_ref() {
            draw_life_setup(
                &font,
                mouse,
                ui,
                life_pop,
                life_food,
                life_hx,
                life_hy,
            );
        }
        if food_panel_open {
            draw_food_panel(
                &frame,
                &font,
                mouse,
                &food_ui,
                feeder_anchor,
                feed_tool,
                feed_kind,
                food_edit,
                selected_feeder,
                world.feeders(),
                world.food_kinds(),
            );
        }
        draw_food_button(
            &frame,
            &font,
            mouse,
            food_panel_open || feed_tool,
            feed_kind,
            tool_hovers.food,
        );
        if settings_open {
            draw_settings_menu(
                &frame,
                &font,
                mouse,
                &setup,
                settings_section,
                fullscreen,
                bloom_on,
                audio.mix(),
                world.viscosity(),
                world.edges(),
            );
        }
        if let Some(sui) = saves_ui.as_ref() {
            draw_saves_panel(
                &frame,
                &font,
                mouse,
                sui,
                &save_list,
                save_selected,
                &save_name,
                save_name_focus,
                true,
                save_status.as_ref().map(|(m, _)| m.as_str()),
            );
        }

        panel_t = damp(panel_t, if pinned.is_some() { 1.0 } else { 0.0 }, dt, 0.22);
        if panel_t < 0.012 && pinned.is_none() {
            shown = None;
            detail_hit = None;
            net_hit = None;
            net_detail_btn = None;
            net_3d_open = false;
            net_3d_drag = None;
        }
        if let Some(s) = shown.as_ref() {
            if panel_t > 0.012 {
                let (panel, hits) =
                    draw_inspect_right(&frame, &font, s, panel_t, detail_tab, mouse);
                detail_hit = Some(panel);
                detail_hits = Some(hits);
                if let Some(net) = world.network(s.id) {
                    let (panel, detail_btn) = draw_inspect_left(
                        &frame,
                        &font,
                        &net,
                        panel_t,
                        world.time(),
                        mouse,
                    );
                    net_hit = Some(panel);
                    net_detail_btn = Some(detail_btn);
                    if net_3d_open {
                        draw_net_3d_overlay(
                            &frame,
                            &font,
                            &net,
                            world.time(),
                            net_3d_yaw,
                            net_3d_pitch,
                            net_3d_dist,
                            mouse,
                        );
                    }
                } else {
                    net_hit = None;
                    net_detail_btn = None;
                    net_3d_open = false;
                }
            }
        } else {
            detail_hit = None;
            detail_hits = None;
            net_hit = None;
            net_detail_btn = None;
            net_3d_open = false;
        }

        if shot && world.time() >= 18.0 {
            let img = get_screen_data();
            image::save_buffer(
                "shot.png",
                &img.bytes,
                img.width as u32,
                img.height as u32,
                image::ColorType::Rgba8,
            )
            .expect("save shot");
            std::process::exit(0);
        }

        paint_transit_veil(gfx.as_ref(), &frame, screen_transit.as_ref().map(|t| t.t));
        next_frame().await;
    }
}

fn damp(current: f32, target: f32, dt: f32, tau: f32) -> f32 {
    let k = 1.0 - (-dt / tau.max(0.05)).exp();
    current + (target - current) * k
}

fn begin_screen_transit(slot: &mut Option<ScreenTransit>, to: Phase, start_seed: Option<u64>) {
    if slot.is_some() {
        return;
    }
    *slot = Some(ScreenTransit {
        to,
        t: 0.0,
        flipped: false,
        start_seed,
    });
}

/// Logo DNA park / letter dissolve used when returning to the title.
fn logo_compose(phase: Phase, transit: Option<&ScreenTransit>) -> f32 {
    if let Some(tr) = transit {
        match tr.to {
            // Table → title: after flip, DNA returns into the word.
            Phase::Title if tr.flipped => {
                smoother(1.0 - ((tr.t - 0.5) * 2.0).clamp(0.0, 1.0))
            }
            _ => 0.0,
        }
    } else {
        let _ = phase;
        0.0
    }
}

/// Title → new game: logo shatter / fly-toward-camera (first half, still on title).
fn logo_burst(transit: Option<&ScreenTransit>) -> f32 {
    if let Some(tr) = transit {
        if tr.to == Phase::Running && !tr.flipped {
            return smoother((tr.t * 2.0).min(1.0));
        }
    }
    0.0
}

fn transit_visual(t: f32) -> (f32, f32, f32) {
    // Crossfade only — used for returning to title.
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let e = smoother(t * 2.0);
        (1.0, 1.0 - e, e * 0.35)
    } else {
        let e = smoother((t - 0.5) * 2.0);
        (1.0, e, (1.0 - e) * 0.35)
    }
}

/// New-game transition: zoom toward camera + building radial blur.
fn transit_visual_new_game(t: f32) -> (f32, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let e = smoother(t * 2.0);
        (1.0 + e * 2.6, 1.0 - e.powf(2.4) * 0.5, e)
    } else {
        let e = smoother((t - 0.5) * 2.0);
        (1.0, e, (1.0 - e) * 0.2)
    }
}

/// Deterministic 0‥1 hash for burst debris directions.
fn burst_hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747796405).wrapping_add(2891336453);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Random-ish unit debris direction; z > 0 flies toward the camera.
fn burst_dir(seed: u32) -> (f32, f32, f32) {
    let ang = burst_hash(seed) * std::f32::consts::TAU;
    let elev = 0.40 + burst_hash(seed.wrapping_add(17)) * 0.60;
    let spread = 0.45 + burst_hash(seed.wrapping_add(31)) * 0.55;
    let xy = (1.0 - elev * 0.35) * spread;
    (ang.cos() * xy, ang.sin() * xy, elev)
}

fn scale_about(x: f32, y: f32, cx: f32, cy: f32, scale: f32) -> (f32, f32) {
    (cx + (x - cx) * scale, cy + (y - cy) * scale)
}

fn scale_rect(rect: (f32, f32, f32, f32), cx: f32, cy: f32, scale: f32) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = rect;
    let mx = x + w * 0.5;
    let my = y + h * 0.5;
    let (nmx, nmy) = scale_about(mx, my, cx, cy, scale);
    let nw = w * scale;
    let nh = h * scale;
    (nmx - nw * 0.5, nmy - nh * 0.5, nw, nh)
}

fn paint_transit_veil(gfx: Option<&Gfx>, frame: &Frame, t: Option<f32>) {
    let Some(t) = t else {
        return;
    };
    let t = t.clamp(0.0, 1.0);
    let e = if t < 0.5 {
        smoother(t * 2.0)
    } else {
        1.0 - smoother((t - 0.5) * 2.0)
    };
    if e < 0.01 {
        return;
    }
    // Cheap full-screen fade — no per-frame soft-blob rings.
    let _ = gfx;
    draw_rectangle(
        0.0,
        0.0,
        frame.sw,
        frame.sh,
        Color::new(0.02, 0.05, 0.07, 0.55 * e),
    );
}

fn smoother(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn step_fades(fades: &mut Vec<Fade>, hovered: Option<u64>, pinned: Option<u64>, dt: f32) {
    for fade in fades.iter_mut() {
        let on_hover = hovered == Some(fade.id) && pinned != Some(fade.id);
        let on_pin = pinned == Some(fade.id);
        fade.hover = damp(fade.hover, if on_hover { 1.0 } else { 0.0 }, dt, 0.07);
        fade.pin = damp(fade.pin, if on_pin { 1.0 } else { 0.0 }, dt, 0.2);
    }
    for id in [hovered, pinned].into_iter().flatten() {
        if !fades.iter().any(|f| f.id == id) {
            fades.push(Fade {
                id,
                hover: 0.0,
                pin: 0.0,
            });
        }
    }
    fades.retain(|f| {
        f.hover > 0.01
            || f.pin > 0.01
            || hovered == Some(f.id)
            || pinned == Some(f.id)
    });
}

fn fade_of(fades: &[Fade], id: u64) -> (f32, f32) {
    fades
        .iter()
        .find(|f| f.id == id)
        .map(|f| (f.hover, f.pin))
        .unwrap_or((0.0, 0.0))
}

fn draw_senses(frame: &Frame, cam: &Cam, app: &Appearance<'_>, t: f32, sensor_reach: f32) {
    if app.nodes.is_empty() {
        return;
    }
    let head = app.world_node(0);
    let tail = app.world_node(app.nodes.len() - 1);
    let axis = {
        let d = Vec2::new(head.x - tail.x, head.y - tail.y);
        let len = d.length();
        if len < 1e-4 {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(d.x / len, d.y / len)
        }
    };
    let (r, g, b) = hsv(app.hue, 0.75, 1.0);
    let a = smoother(t.clamp(0.0, 1.0)) * 0.32;
    let half = 0.95;
    paint_sense_sector(frame, cam, head, axis, sensor_reach, half, r, g, b, a);
}

fn paint_sense_sector(
    frame: &Frame,
    cam: &Cam,
    origin: Vec2,
    dir: Vec2,
    reach: f32,
    half: f32,
    r: f32,
    g: f32,
    b: f32,
    alpha: f32,
) {
    if alpha < 0.004 {
        return;
    }
    let base = dir.y.atan2(dir.x);
    let (ox, oy) = world_to_screen(frame, cam, origin);
    let layers = 10;
    let wedges = 18;
    for layer in (0..layers).rev() {
        let u0 = layer as f32 / layers as f32;
        let u1 = (layer + 1) as f32 / layers as f32;
        let fall = 1.0 - u1;
        let a = alpha * fall * fall;
        if a < 0.003 {
            continue;
        }
        let rad0 = reach * u0;
        let rad1 = reach * u1;
        for i in 0..wedges {
            let t0 = i as f32 / wedges as f32;
            let t1 = (i + 1) as f32 / wedges as f32;
            let a0 = base - half + 2.0 * half * t0;
            let a1 = base - half + 2.0 * half * t1;
            let p00 = world_to_screen(
                frame,
                cam,
                Vec2::new(origin.x + a0.cos() * rad0, origin.y + a0.sin() * rad0),
            );
            let p01 = world_to_screen(
                frame,
                cam,
                Vec2::new(origin.x + a1.cos() * rad0, origin.y + a1.sin() * rad0),
            );
            let p10 = world_to_screen(
                frame,
                cam,
                Vec2::new(origin.x + a0.cos() * rad1, origin.y + a0.sin() * rad1),
            );
            let p11 = world_to_screen(
                frame,
                cam,
                Vec2::new(origin.x + a1.cos() * rad1, origin.y + a1.sin() * rad1),
            );
            let col = Color::new(r, g, b, a);
            if layer == 0 {
                draw_triangle(
                    macroquad::math::Vec2::new(ox, oy),
                    macroquad::math::Vec2::new(p10.0, p10.1),
                    macroquad::math::Vec2::new(p11.0, p11.1),
                    col,
                );
            } else {
                draw_triangle(
                    macroquad::math::Vec2::new(p00.0, p00.1),
                    macroquad::math::Vec2::new(p10.0, p10.1),
                    macroquad::math::Vec2::new(p11.0, p11.1),
                    col,
                );
                draw_triangle(
                    macroquad::math::Vec2::new(p00.0, p00.1),
                    macroquad::math::Vec2::new(p11.0, p11.1),
                    macroquad::math::Vec2::new(p01.0, p01.1),
                    col,
                );
            }
        }
    }
}

struct ControlBar {
    bar: (f32, f32, f32, f32),
    settings: (f32, f32, f32, f32),
    pause: (f32, f32, f32, f32),
    saves: (f32, f32, f32, f32),
    speed: (f32, f32, f32, f32),
    speed_options: [(f32, f32, f32, f32); 9],
    /// Pixels per world unit — for font / stroke scaling.
    #[allow(dead_code)]
    s: f32,
}

/// Screen region for the dish viewport (full window).
#[allow(dead_code)]
fn dish_fit_rect(frame: &Frame) -> (f32, f32, f32, f32) {
    (0.0, 0.0, frame.sw, frame.sh)
}

fn view_origin(frame: &Frame) -> (f32, f32) {
    (frame.sw * 0.5, frame.sh * 0.5)
}

fn px(world: f32, s: f32) -> f32 {
    world * s
}

fn fill_round_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    if r < 0.5 {
        draw_rectangle(x, y, w, h, color);
        return;
    }
    draw_rectangle(x + r, y, (w - 2.0 * r).max(0.0), h, color);
    draw_rectangle(x, y + r, w, (h - 2.0 * r).max(0.0), color);
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}

fn stroke_round_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, thickness: f32, color: Color) {
    if w <= 0.0 || h <= 0.0 || thickness <= 0.0 {
        return;
    }
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    if r < 0.5 {
        draw_rectangle_lines(x, y, w, h, thickness, color);
        return;
    }
    draw_line(x + r, y, x + w - r, y, thickness, color);
    draw_line(x + r, y + h, x + w - r, y + h, thickness, color);
    draw_line(x, y + r, x, y + h - r, thickness, color);
    draw_line(x + w, y + r, x + w, y + h - r, thickness, color);
    // Macroquad arcs: rotation degrees, sweep `arc` degrees clockwise from +X.
    draw_arc(x + r, y + r, 16, r, 180.0, thickness, 90.0, color);
    draw_arc(x + w - r, y + r, 16, r, 270.0, thickness, 90.0, color);
    draw_arc(x + w - r, y + h - r, 16, r, 0.0, thickness, 90.0, color);
    draw_arc(x + r, y + h - r, 16, r, 90.0, thickness, 90.0, color);
}

fn control_bar_layout(
    frame: &Frame,
    _cam: &Cam,
    _center: Vec2,
    _half_x: f32,
    _half_y: f32,
    speed_open: bool,
) -> ControlBar {
    let icon = 36.0;
    let gap = 12.0;
    let total_btns = 4.0 * icon + 3.0 * gap;
    let cx = frame.sw * 0.5;
    let bx0 = cx - total_btns * 0.5;
    let y = 16.0;
    let bar_pad = 6.0;
    let bar = (bx0 - bar_pad, y - bar_pad * 0.5, total_btns + bar_pad * 2.0, icon + bar_pad);
    let settings = (bx0, y, icon, icon);
    let pause = (bx0 + 1.0 * (icon + gap), y, icon, icon);
    let speed = (bx0 + 2.0 * (icon + gap), y, icon, icon);
    let saves = (bx0 + 3.0 * (icon + gap), y, icon, icon);
    let mut speed_options = [(0.0, 0.0, 0.0, 0.0); 9];
    if speed_open {
        let ow = 36.0;
        let oh = 26.0;
        let og = 4.0;
        let total = 9.0 * ow + 8.0 * og;
        let x0 = cx - total * 0.5;
        let oy = y + icon + 10.0;
        for i in 0..9 {
            speed_options[i] = (x0 + i as f32 * (ow + og), oy, ow, oh);
        }
    }
    ControlBar {
        bar,
        settings,
        pause,
        saves,
        speed,
        speed_options,
        s: 1.0,
    }
}

fn census_rect(
    _frame: &Frame,
    _cam: &Cam,
    _center: Vec2,
    _half_x: f32,
    _half_y: f32,
    open: bool,
) -> (f32, f32, f32, f32) {
    if !open {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let x = 24.0;
    let y = 20.0;
    let w = 240.0;
    let h = 370.0;
    (x, y, w, h)
}

fn spawn_button_rect(frame: &Frame) -> (f32, f32, f32, f32) {
    bottom_tool_slot(frame, 0)
}

fn food_button_rect(frame: &Frame) -> (f32, f32, f32, f32) {
    bottom_tool_slot(frame, 1)
}

fn dish_button_rect(_frame: &Frame) -> (f32, f32, f32, f32) {
    (0.0, 0.0, 0.0, 0.0)
}

fn bottom_tool_slot(frame: &Frame, slot: usize) -> (f32, f32, f32, f32) {
    let count = 2usize;
    let gap = 20.0;
    let total = count as f32 * TOOL + (count - 1) as f32 * gap;
    let x0 = frame.sw * 0.5 - total * 0.5;
    let y = frame.sh - TOOL - 22.0;
    let x = x0 + slot as f32 * (TOOL + gap);
    (x, y, TOOL, TOOL)
}

struct SpeedMenu {
    main: (f32, f32, f32, f32),
    options: [(f32, f32, f32, f32); 9],
}

fn speed_menu(
    frame: &Frame,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
    open: bool,
) -> SpeedMenu {
    let bar = control_bar_layout(frame, cam, center, half_x, half_y, open);
    SpeedMenu {
        main: bar.speed,
        options: bar.speed_options,
    }
}

fn speed_label(speed: u32) -> &'static str {
    SPEED_PRESETS
        .iter()
        .find(|(n, _)| *n == speed)
        .map(|(_, l)| *l)
        .unwrap_or("1×")
}

fn draw_control_bar_bg(_bar: &ControlBar) {
    // Header background and border removed per user request: icons and clock float cleanly.
}


fn paint_header_icon(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    mouse: (f32, f32),
    active: bool,
    hover_t: f32,
) -> (bool, f32, f32, Color) {
    let hot = hit(mouse, x, y, w, h);
    let t = smoother(hover_t.clamp(0.0, 1.0));
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let lit = active || hot || t > 0.35;
    let stroke = (w * 0.045).max(1.0);
    draw_circle(
        cx,
        cy,
        w * 0.48,
        Color::new(
            0.08 + 0.12 * t,
            0.18 + 0.25 * t,
            0.2 + 0.22 * t,
            0.55 + 0.35 * t + if active { 0.2 } else { 0.0 },
        ),
    );
    if lit {
        draw_circle_lines(cx, cy, w * 0.48, stroke, Color::new(0.45, 0.98, 0.92, 0.55 + 0.35 * t));
    } else {
        draw_circle_lines(cx, cy, w * 0.48, stroke, Color::new(0.3, 0.65, 0.62, 0.4));
    }
    let ink = if lit {
        Color::new(0.88, 1.0, 0.97, 1.0)
    } else {
        Color::new(0.65, 0.85, 0.86, 0.95)
    };
    (hot, cx, cy, ink)
}

fn draw_speed_menu(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    speed: u32,
    open: bool,
    ui: &SpeedMenu,
    _bar: &ControlBar,
    hover_t: f32,
) {
    let _ = frame;
    let (x, y, w, h) = ui.main;
    let (_hot, cx, cy, ink) = paint_header_icon(x, y, w, h, mouse, open, hover_t);
    let speed_scale = 1.0;
    center_text_scaled(
        font,
        speed_label(speed),
        cx,
        cy + h * 0.18,
        14,
        speed_scale,
        ink,
    );
    if open {
        for (i, rect) in ui.options.iter().enumerate() {
            let (n, label) = SPEED_PRESETS[i];
            draw_chip(font, *rect, label, speed == n, mouse);
        }
    }
}

fn hit(mouse: (f32, f32), x: f32, y: f32, w: f32, h: f32) -> bool {
    mouse.0 >= x && mouse.0 <= x + w && mouse.1 >= y && mouse.1 <= y + h
}

fn hit_rect(mouse: (f32, f32), rect: (f32, f32, f32, f32)) -> bool {
    rect.2 > 0.0 && rect.3 > 0.0 && hit(mouse, rect.0, rect.1, rect.2, rect.3)
}

fn paint_round_tool(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    mouse: (f32, f32),
    active: bool,
    accent: (f32, f32, f32),
    labeled: bool,
    hover_t: f32,
) -> (bool, f32, f32) {
    let hot = hit(mouse, x, y, w, h);
    let (ar, ag, ab) = accent;
    let t = smoother(hover_t.clamp(0.0, 1.0));
    if t > 0.01 {
        let grow = 0.82 + 0.18 * t;
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let rad = w * 0.5 * grow;
        let edge = if active {
            Color::new(ar, ag, ab, 0.55 + 0.4 * t)
        } else {
            Color::new(
                ar * 0.55 + 0.12,
                ag * 0.55 + 0.16,
                ab * 0.55 + 0.16,
                0.35 + 0.5 * t,
            )
        };
        let bg = if active {
            Color::new(ar * 0.14, ag * 0.16, ab * 0.12, 0.55 + 0.4 * t)
        } else {
            Color::new(0.04, 0.08, 0.1, 0.35 + 0.5 * t)
        };
        draw_circle(cx, cy, rad + 2.0, edge);
        draw_circle(cx, cy, rad, bg);
    }
    let cy = if labeled {
        y + h * 0.5 - 6.0
    } else {
        y + h * 0.5
    };
    (hot, x + w * 0.5, cy)
}

fn tool_label(font: &Option<Font>, label: &str, x: f32, y: f32, w: f32, h: f32, hot: bool) {
    center_text(
        font,
        label,
        x + w * 0.5,
        y + h - 14.0,
        13,
        if hot {
            Color::new(0.85, 0.98, 0.96, 1.0)
        } else {
            Color::new(0.62, 0.8, 0.84, 0.9)
        },
    );
}

fn draw_spawn_button(frame: &Frame, font: &Option<Font>, mouse: (f32, f32), hover_t: f32) {
    let (x, y, w, h) = spawn_button_rect(frame);
    let (hot, cx, cy) =
        paint_round_tool(x, y, w, h, mouse, false, (0.45, 0.82, 0.98), true, hover_t);
    draw_creature_icon(cx, cy, hot || hover_t > 0.4);
    tool_label(font, "Jedinec", x, y, w, h, hot || hover_t > 0.4);
}

fn draw_creature_icon(cx: f32, cy: f32, hot: bool) {
    let nodes = [
        (cx - 13.0, cy + 7.0, 7.5, 0.78),
        (cx - 1.0, cy + 1.0, 8.5, 0.84),
        (cx + 12.0, cy - 7.0, 10.0, 0.92),
    ];
    let ink = if hot { 1.0 } else { 0.82 };
    for (x, y, r, hue) in nodes {
        let (cr, cg, cb) = hsv(hue, 0.85, ink);
        draw_circle(x, y, r, Color::new(0.03, 0.02, 0.05, 1.0));
        draw_circle_lines(x, y, r, 2.2, Color::new(cr, cg, cb, 1.0));
    }
}

fn draw_pause_button(
    font: &Option<Font>,
    bar: &ControlBar,
    mouse: (f32, f32),
    paused: bool,
    hover_t: f32,
) {
    let _ = font;
    let (x, y, w, h) = bar.pause;
    let (_hot, cx, cy, ink) = paint_header_icon(x, y, w, h, mouse, paused, hover_t);
    let u = w / 28.0;
    if paused {
        draw_triangle(
            macroquad::math::Vec2::new(cx - 4.0 * u, cy - 6.5 * u),
            macroquad::math::Vec2::new(cx - 4.0 * u, cy + 6.5 * u),
            macroquad::math::Vec2::new(cx + 7.5 * u, cy),
            ink,
        );
    } else {
        draw_rectangle(cx - 5.5 * u, cy - 6.5 * u, 3.4 * u, 13.0 * u, ink);
        draw_rectangle(cx + 2.0 * u, cy - 6.5 * u, 3.4 * u, 13.0 * u, ink);
    }
}

fn draw_saves_button(
    font: &Option<Font>,
    bar: &ControlBar,
    mouse: (f32, f32),
    open: bool,
    hover_t: f32,
) {
    let _ = font;
    let (x, y, w, h) = bar.saves;
    let (_hot, cx, cy, ink) = paint_header_icon(x, y, w, h, mouse, open, hover_t);
    let u = w / 28.0;
    let stroke = (2.0 * u).max(1.0);
    draw_line(cx, cy - 7.0 * u, cx, cy + 2.0 * u, stroke, ink);
    draw_triangle(
        macroquad::math::Vec2::new(cx - 5.0 * u, cy + 0.5 * u),
        macroquad::math::Vec2::new(cx + 5.0 * u, cy + 0.5 * u),
        macroquad::math::Vec2::new(cx, cy + 6.5 * u),
        ink,
    );
    draw_line(cx - 7.0 * u, cy + 8.0 * u, cx + 7.0 * u, cy + 8.0 * u, stroke, ink);
}

fn draw_dish_settings_button(
    _font: &Option<Font>,
    bar: &ControlBar,
    mouse: (f32, f32),
    open: bool,
    hover_t: f32,
) {
    let (x, y, w, h) = bar.settings;
    let (_hot, cx, cy, ink) = paint_header_icon(x, y, w, h, mouse, open, hover_t);
    let u = w / 28.0;
    let stroke = (1.5 * u).max(1.0);
    draw_circle_lines(cx, cy, 5.8 * u, stroke, ink);
    draw_circle(cx, cy, 2.0 * u, ink);
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::TAU;
        draw_line(
            cx + a.cos() * 6.8 * u,
            cy + a.sin() * 6.8 * u,
            cx + a.cos() * 9.8 * u,
            cy + a.sin() * 9.8 * u,
            stroke,
            ink,
        );
    }
}



const BASE_CHROME_TITLE_FS: u16 = 24;
const BASE_CHROME_ROW_FS: u16 = 16;

fn draw_census(
    frame: &Frame,
    font: &Option<Font>,
    world: &World,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
    open: bool,
) {
    if !open {
        return;
    }
    let c = world.census();
    let (x, y, w, h) = census_rect(frame, cam, center, half_x, half_y, true);
    fill_round_rect(
        x - 10.0,
        y - 8.0,
        w,
        h,
        8.0,
        Color::new(0.01, 0.03, 0.05, 0.45),
    );
    let title_scale = 1.0;
    let row_scale = 1.0;
    let row_h = 24.0;
    let mins = (world.time() / 60.0).floor() as u32;
    let secs = (world.time() % 60.0).floor() as u32;
    let time_str = if mins >= 60 {
        format!("{}:{:02}:{:02}", mins / 60, mins % 60, secs)
    } else {
        format!("{mins}:{secs:02}")
    };
    let lines: [(&str, String, bool); 14] = [
        ("EKOSYSTÉM", time_str, true),
        ("organismy", format!("{}", c.alive), false),
        ("jídlo", format!("{}", c.food), false),
        ("krmítka", format!("{}", c.feeders), false),
        ("hladoví", format!("{}", c.hungry), false),
        ("generace", format!("{}", c.max_generation), false),
        ("narození", format!("{}", c.births), false),
        ("úmrtí", format!("{}", c.deaths), false),
        ("Ø energie", format!("{:.2}", c.mean_energy), false),
        ("Ø hlad", format!("{:.2}", c.mean_hunger), false),
        ("Ø věk", format!("{:.0} s", c.mean_age), false),
        ("Ø hmota", format!("{:.2}", c.mean_mass), false),
        ("Ø neurony", format!("{:.0}", c.mean_neurons), false),
        ("Ø synapse", format!("{:.0}", c.mean_synapses), false),
    ];
    let ink = Color::new(0.9, 0.96, 0.97, 0.95);
    let dim = Color::new(0.62, 0.8, 0.84, 0.85);
    let gold = Color::new(0.40, 0.95, 0.85, 0.98);
    let time_col = Color::new(0.70, 0.95, 0.92, 0.95);
    let mut yy = y + BASE_CHROME_TITLE_FS as f32 * title_scale;
    let value_x = x + 140.0;
    for (i, (label, value, header)) in lines.iter().enumerate() {
        if *header {
            text_scaled(font, label, x, yy, BASE_CHROME_TITLE_FS, title_scale, gold);
            text_scaled(font, value, value_x, yy, BASE_CHROME_TITLE_FS, title_scale, time_col);
            yy += row_h * 1.2;
            continue;
        }
        text_scaled(font, label, x, yy, BASE_CHROME_ROW_FS, row_scale, dim);
        text_scaled(
            font,
            value,
            value_x,
            yy,
            BASE_CHROME_ROW_FS,
            row_scale,
            if i == 2 { ink } else { dim },
        );
        yy += row_h;
    }
}

struct LifeSetupUi {
    panel: (f32, f32, f32, f32),
    pop_minus: (f32, f32, f32, f32),
    pop_value: (f32, f32, f32, f32),
    pop_plus: (f32, f32, f32, f32),
    food_minus: (f32, f32, f32, f32),
    food_value: (f32, f32, f32, f32),
    food_plus: (f32, f32, f32, f32),
    width_minus: (f32, f32, f32, f32),
    width_value: (f32, f32, f32, f32),
    width_plus: (f32, f32, f32, f32),
    height_minus: (f32, f32, f32, f32),
    height_value: (f32, f32, f32, f32),
    height_plus: (f32, f32, f32, f32),
    start: (f32, f32, f32, f32),
    close: (f32, f32, f32, f32),
}

fn empty_life_button_rect(
    frame: &Frame,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
) -> (f32, f32, f32, f32) {
    let (dx, dy, dw, dh) = dish_screen_rect_at(frame, cam, center, half_x, half_y);
    let s = world_scale(frame, cam).max(1e-3);
    // Pure world units — scales with camera zoom (no pixel floor).
    let bw = px(W_LIFE_BTN_W, s);
    let bh = px(W_LIFE_BTN_H, s);
    (dx + (dw - bw) * 0.5, dy + (dh - bh) * 0.5, bw, bh)
}

fn life_setup_layout(frame: &Frame) -> LifeSetupUi {
    let w = 320.0_f32.min(frame.sw - 40.0);
    let h = 280.0;
    let x = (frame.sw - w) * 0.5;
    let y = ((frame.sh - h) * 0.5).max(24.0);
    let ax = x + 18.0;
    let aw = w - 36.0;
    let mut yy = y + 52.0;
    let [pm, pv, pp] = stepper_row(ax, yy, aw, 28.0);
    yy += 48.0;
    let [fm, fv, fp] = stepper_row(ax, yy, aw, 28.0);
    yy += 48.0;
    let [wm, wv, wp] = stepper_row(ax, yy, aw, 28.0);
    yy += 48.0;
    let [hm, hv, hp] = stepper_row(ax, yy, aw, 28.0);
    yy += 44.0;
    let start = (ax, yy, aw, 36.0);
    LifeSetupUi {
        panel: (x, y, w, h),
        pop_minus: pm,
        pop_value: pv,
        pop_plus: pp,
        food_minus: fm,
        food_value: fv,
        food_plus: fp,
        width_minus: wm,
        width_value: wv,
        width_plus: wp,
        height_minus: hm,
        height_value: hv,
        height_plus: hp,
        start,
        close: (x + w - 36.0, y + 10.0, 26.0, 26.0),
    }
}

fn draw_empty_life_button(
    frame: &Frame,
    font: &Option<Font>,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
    mouse: (f32, f32),
    hover_t: f32,
) -> (f32, f32, f32, f32) {
    let rect = empty_life_button_rect(frame, cam, center, half_x, half_y);
    let (x, y, w, h) = rect;
    let s = world_scale(frame, cam).max(1e-3);
    let hot = hit_rect(mouse, rect);
    let t = smoother(hover_t.max(if hot { 1.0 } else { 0.0 }).clamp(0.0, 1.0));
    let r = h * 0.45;
    let stroke = (px(0.004, s)).max(1.0);
    fill_round_rect(
        x,
        y,
        w,
        h,
        r,
        Color::new(0.04 + 0.04 * t, 0.1 + 0.08 * t, 0.12 + 0.06 * t, 0.88),
    );
    stroke_round_rect(
        x,
        y,
        w,
        h,
        r,
        stroke,
        Color::new(0.4 + 0.3 * t, 0.95, 0.88, 0.55 + 0.35 * t),
    );
    let btn_scale = (px(0.045, s) / 16.0).clamp(0.6, 2.2);
    center_text_scaled(
        font,
        "Nový život",
        x + w * 0.5,
        y + h * 0.68,
        16,
        btn_scale,
        Color::new(0.82 + 0.15 * t, 0.98, 0.94, 1.0),
    );
    rect
}

fn draw_life_setup(
    font: &Option<Font>,
    mouse: (f32, f32),
    ui: &LifeSetupUi,
    pop: usize,
    food: usize,
    hx: f32,
    hy: f32,
) {
    let (x, y, w, h) = ui.panel;
    fill_round_rect(x, y, w, h, 12.0, CHROME_FILL);
    stroke_round_rect(x, y, w, h, 12.0, 1.5, CHROME_EDGE);
    let dim = Color::new(0.62, 0.8, 0.84, 0.9);
    center_text(
        font,
        "Počáteční nastavení",
        x + w * 0.5,
        y + 34.0,
        16,
        Color::new(0.55, 0.95, 0.88, 0.95),
    );
    draw_chip(font, ui.close, "×", false, mouse);
    text(font, "jedinci", ui.pop_minus.0, ui.pop_minus.1 - 4.0, 12, dim);
    draw_stepper(
        font,
        ui.pop_minus,
        ui.pop_value,
        ui.pop_plus,
        &format!("{pop}"),
        mouse,
    );
    text(font, "jídlo", ui.food_minus.0, ui.food_minus.1 - 4.0, 12, dim);
    draw_stepper(
        font,
        ui.food_minus,
        ui.food_value,
        ui.food_plus,
        &format!("{food}"),
        mouse,
    );
    text(
        font,
        "šířka misky",
        ui.width_minus.0,
        ui.width_minus.1 - 4.0,
        12,
        dim,
    );
    draw_stepper(
        font,
        ui.width_minus,
        ui.width_value,
        ui.width_plus,
        &format!("{hx:.2}"),
        mouse,
    );
    text(
        font,
        "výška misky",
        ui.height_minus.0,
        ui.height_minus.1 - 4.0,
        12,
        dim,
    );
    draw_stepper(
        font,
        ui.height_minus,
        ui.height_value,
        ui.height_plus,
        &format!("{hy:.2}"),
        mouse,
    );
    draw_chip(font, ui.start, "Založit život", true, mouse);
}

#[allow(dead_code)]
fn dish_screen_rect(frame: &Frame, cam: &Cam, half_x: f32, half_y: f32) -> (f32, f32, f32, f32) {
    dish_screen_rect_at(frame, cam, Vec2::ZERO, half_x, half_y)
}

fn dish_screen_rect_at(
    frame: &Frame,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
) -> (f32, f32, f32, f32) {
    let corners = [
        center + Vec2::new(-half_x, -half_y),
        center + Vec2::new(half_x, -half_y),
        center + Vec2::new(half_x, half_y),
        center + Vec2::new(-half_x, half_y),
    ];
    let pts: Vec<(f32, f32)> = corners
        .iter()
        .map(|p| world_to_screen(frame, cam, *p))
        .collect();
    let min_x = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    (min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
}

/// Lobby water — full window, no dish letterbox.
fn paint_lobby_backdrop(
    gfx: Option<&Gfx>,
    frame: &Frame,
    time: f32,
    floor_light: Option<((f32, f32), f32)>,
) {
    let aspect = (frame.sw / frame.sh.max(1.0)).clamp(0.25, 4.0);
    if let Some(g) = gfx {
        g.draw_water(
            0.0,
            0.0,
            frame.sw,
            frame.sh,
            time,
            aspect,
            floor_light,
            &[],
            0.0,
            (0.0, 0.0),
            0.0,
        );
    } else {
        // CPU path: oversized world half so dish_screen_rect covers the whole window.
        let cam = Cam::identity();
        let scale = world_scale(frame, &cam).max(1e-3);
        let (ox, oy) = view_origin(frame);
        let half_x = (ox.max(frame.sw - ox) / scale).max(aspect);
        let half_y = (oy.max(frame.sh - oy) / scale).max(1.0);
        draw_liquid_bg(frame, &cam, half_x, half_y, time, floor_light, &[]);
    }
}

/// Table water that pans and zooms with the game camera, showing the fluid background
/// and stopping the surrounding green fog at dish boundaries.
fn paint_table_backdrop(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cam: &Cam,
    _table_hx: f32,
    _table_hy: f32,
    time: f32,
    _dishes: &[PetriDish],
) {
    let aspect = (frame.sw / frame.sh.max(1.0)).clamp(0.25, 4.0);
    let dish_uvs = vec![[0.0, 0.0, 1.0, 1.0]];
    let dish_corner_uv = 0.0;

    if let Some(g) = gfx {
        g.draw_water(
            0.0,
            0.0,
            frame.sw,
            frame.sh,
            time,
            aspect,
            None,
            &dish_uvs,
            dish_corner_uv,
            (cam.center.x, cam.center.y),
            cam.zoom,
        );
    } else {
        let (vhx, vhy) = visible_world_half(frame, cam);
        draw_liquid_bg(frame, cam, vhx, vhy, time, None, &[]);
    }
}

/// Dish boundary — subtle laboratory glass rim along window edges.
fn paint_dish_cutout(
    frame: &Frame,
    _cam: &Cam,
    _center: Vec2,
    _half_x: f32,
    _half_y: f32,
    _time: f32,
    _hover: f32,
) {
    let rim = 2.0;
    draw_rectangle_lines(0.0, 0.0, frame.sw, frame.sh, rim * 2.0, Color::new(0.12, 0.45, 0.40, 0.35));
    draw_rectangle_lines(rim, rim, frame.sw - rim * 2.0, frame.sh - rim * 2.0, 1.0, Color::new(0.60, 0.95, 0.92, 0.40));
}

fn paint_food_glow(
    g: &Gfx,
    frame: &Frame,
    cam: &Cam,
    pos: Vec2,
    sense: f32,
    color: (f32, f32, f32),
    bloom: f32,
    fade: Option<f32>,
) {
    let (x, y) = world_to_screen(frame, cam, pos);
    let scale = world_scale(frame, cam);
    let (cr, cg, cb) = color;
    let bloom_e = {
        let t = bloom.clamp(0.0, 1.0);
        1.0 - (1.0 - t) * (1.0 - t)
    };
    let outer = (sense * bloom_e * scale).max(if bloom_e > 0.02 { 6.0 } else { 0.0 });
    let fade_u = fade.unwrap_or(-1.0);
    if outer > 0.5 {
        g.soft_blob_cont(
            x,
            y,
            outer,
            Color::new(cr, cg, cb, 0.55),
            2.2,
            fade_u,
            0.0,
        );
    }
    let core_a = if fade_u < 0.0 {
        1.0
    } else {
        (1.0 - fade_u / 0.18).clamp(0.0, 1.0)
    };
    if core_a > 0.02 {
        let core = (0.012 * scale).max(2.2);
        g.soft_blob_cont(
            x,
            y,
            core * 3.2,
            Color::new(cr, cg, cb, 0.9 * core_a),
            1.4,
            -1.0,
            0.55 * core_a,
        );
    }
}

fn paint_food(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cam: &Cam,
    pos: Vec2,
    sense: f32,
    color: (f32, f32, f32),
    bloom: f32,
    fade: Option<f32>,
) {
    if let Some(g) = gfx {
        g.begin_glow();
        paint_food_glow(g, frame, cam, pos, sense, color, bloom, fade);
        g.end_glow();
    } else {
        draw_food(frame, cam, pos, sense, color, bloom, fade);
    }
}

fn paint_food_ghost(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cam: &Cam,
    mouse: (f32, f32),
    sense: f32,
    color: (f32, f32, f32),
) {
    let Some(p) = screen_to_world(frame, cam, mouse.0, mouse.1) else {
        return;
    };
    paint_food(gfx, frame, cam, p, sense, color, 1.0, None);
}

fn paint_organism(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cam: &Cam,
    font: &Option<Font>,
    app: &Appearance<'_>,
    time: f32,
    hover: f32,
) {
    if app.nodes.is_empty() {
        return;
    }
    let hover = smoother(hover.clamp(0.0, 1.0));
    let s = world_scale(frame, cam);
    let (cr, cg, cb) = hsv(app.hue, 0.85, 1.0);
    let energy = (app.energy / 2.0).clamp(0.0, 1.0);
    let mouth = app.mouth.clamp(0.0, 1.0);
    let pts: Vec<(f32, f32)> = app
        .world_nodes()
        .map(|n| world_to_screen(frame, cam, n))
        .collect();
    let n = pts.len();
    let base_r = (app.node_radius * s).max(3.2) * (1.0 + 0.14 * hover);

    let mut mid_x = 0.0;
    let mut mid_y = 0.0;
    for &(x, y) in &pts {
        mid_x += x;
        mid_y += y;
    }
    mid_x /= n as f32;
    mid_y /= n as f32;

    // Soft bloom halo (neon tube bleed) — not a filled metaball body.
    if let Some(g) = gfx {
        g.soft_blob(
            mid_x,
            mid_y,
            base_r * (2.6 + 0.3 * n as f32 + 0.5 * hover),
            Color::new(cr, cg, cb, 0.07 + 0.06 * energy + 0.08 * hover),
            2.2,
            -1.0,
            0.0,
        );
        for (i, &(x, y)) in pts.iter().enumerate() {
            let fall = 1.0 - i as f32 * (0.55 / n.max(1) as f32);
            let head = i == 0;
            let rad = base_r
                * if head {
                    1.45 + 0.2 * mouth
                } else {
                    1.1
                }
                * fall.max(0.55);
            let chill = (time * 1.4 + app.id as f32 * 0.17 + i as f32).sin() * 0.5 + 0.5;
            g.soft_blob(
                x,
                y,
                rad * 2.4,
                Color::new(cr, cg, cb, 0.1 + 0.08 * energy + 0.08 * hover),
                1.9,
                -1.0,
                0.0,
            );
            g.soft_blob(
                x - 1.4,
                y,
                rad * 2.0,
                Color::new(1.0, 0.25, 0.55, (0.05 + 0.04 * chill) * (0.6 + 0.4 * energy)),
                1.7,
                -1.0,
                0.0,
            );
            g.soft_blob(
                x + 1.4,
                y,
                rad * 2.0,
                Color::new(0.15, 1.0, 0.95, (0.06 + 0.04 * (1.0 - chill)) * (0.6 + 0.4 * energy)),
                1.7,
                -1.0,
                0.0,
            );
        }
    } else {
        draw_circle(
            mid_x,
            mid_y,
            base_r * (2.4 + 0.25 * n as f32 + 0.4 * hover),
            Color::new(cr, cg, cb, 0.08 + 0.06 * energy + 0.08 * hover),
        );
    }

    // Neon tube body: dark fill + bright rim (same language as title creatures).
    let ink = 0.82 + 0.18 * energy;
    for pair in pts.windows(2) {
        draw_line(
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1,
            (base_r * 0.72).max(2.4),
            Color::new(0.02, 0.03, 0.05, 0.92),
        );
        draw_line(
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1,
            (base_r * 0.38).max(1.6),
            Color::new(cr * ink, cg * ink, cb * ink, 0.75 + 0.2 * hover),
        );
    }
    for (i, &(x, y)) in pts.iter().enumerate() {
        let fall = 1.0 - i as f32 * (0.55 / n.max(1) as f32);
        let head = i == 0;
        let rad = base_r
            * if head {
                1.55 + 0.25 * mouth
            } else {
                1.15
            }
            * fall.max(0.55);
        draw_circle(x, y, rad, Color::new(0.03, 0.02, 0.05, 1.0));
        draw_circle_lines(
            x,
            y,
            rad,
            (2.0 + 0.8 * hover).max(1.6),
            Color::new(cr, cg, cb, 0.88 + 0.12 * hover),
        );
        draw_circle_lines(
            x,
            y,
            rad * 0.72,
            1.1,
            Color::new(
                (cr + 0.25).min(1.0),
                (cg + 0.2).min(1.0),
                (cb + 0.15).min(1.0),
                0.35 + 0.25 * energy,
            ),
        );
        if head {
            draw_circle(
                x,
                y,
                rad * 0.28,
                Color::new(0.92, 1.0, 0.98, 0.4 + 0.4 * mouth + 0.15 * hover),
            );
        }
    }
    let core = (base_r * 0.55).max(2.2);
    if let Some(g) = gfx {
        g.soft_blob(
            mid_x,
            mid_y,
            core * (2.4 + 0.4 * hover),
            Color::new(0.72, 0.48, 1.0, 0.55 + 0.25 * hover),
            1.6,
            -1.0,
            0.55 + 0.2 * hover,
        );
    }
    draw_circle(
        mid_x,
        mid_y,
        core + 1.0 + 1.2 * hover,
        Color::new(0.55, 0.32, 0.9, 0.55),
    );
    draw_circle_lines(
        mid_x,
        mid_y,
        core + 0.5,
        1.6,
        Color::new(0.85, 0.65, 1.0, 0.9 + 0.1 * hover),
    );
    draw_circle(mid_x, mid_y, core * 0.4, Color::new(0.9, 0.78, 1.0, 0.95));
    if gfx.is_none() {
        draw_energy_bar(frame, cam, font, app);
    }
    let _ = font;
}

fn paint_select_glow(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cam: &Cam,
    app: &Appearance<'_>,
    t: f32,
    time: f32,
) {
    if let Some(g) = gfx {
        if app.nodes.is_empty() || t < 0.02 {
            return;
        }
        let s = world_scale(frame, cam);
        let (r, gc, b) = hsv(app.hue, 0.9, 1.0);
        let breathe = 1.0 + 0.045 * (time * 1.7).sin() * t;
        for (i, node) in app.nodes.iter().enumerate() {
            let (x, y) = world_to_screen(frame, cam, *node + app.dish_pos);
            let base = app.node_radius * if i == 0 { 1.7 } else { 1.45 } * s;
            let rad = (base * breathe + 8.0 * t).max(4.0);
            g.soft_blob(
                x,
                y,
                rad,
                Color::new(r, gc, b, 0.2 + 0.35 * t),
                2.4,
                0.35,
                0.15 * t,
            );
        }
    } else {
        draw_select_glow(frame, cam, app, t, time);
    }
}

fn paint_spark(gfx: Option<&Gfx>, frame: &Frame, cam: &Cam, spark: &Spark) {
    if let Some(g) = gfx {
        let (x, y) = world_to_screen(frame, cam, spark.pos);
        if x < -20.0 || x > frame.sw + 20.0 || y < -20.0 || y > frame.sh + 20.0 {
            return;
        }
        let t = (spark.life / spark.max_life).clamp(0.0, 1.0);
        let unit = frame.sh * 0.012 * cam.zoom;
        match spark.style {
            1 => {
                let (r, gc, b) = (0.95, 0.18 + spark.hue * 0.15, 0.22);
                let rad = (spark.size * unit * (0.5 + 0.55 * t)).max(1.4);
                g.soft_blob(
                    x,
                    y,
                    rad * 2.2,
                    Color::new(r, gc, b, 0.85 * t),
                    1.5,
                    -1.0,
                    0.4 * t,
                );
            }
            2 => {
                let (r, gc, b) = hsv(spark.hue, 0.7, 1.0);
                let rad = (spark.size * unit * (0.65 + 0.5 * t)).max(2.0);
                g.soft_blob(
                    x,
                    y,
                    rad * 2.4,
                    Color::new(r, gc, b, 0.8 * t),
                    1.6,
                    -1.0,
                    0.5 * t,
                );
            }
            _ => {
                let (r, gc, b) = hsv(spark.hue, 0.75, 1.0);
                let rad = (spark.size * unit * (0.55 + 0.45 * t)).max(1.1);
                g.soft_blob(
                    x,
                    y,
                    rad * 2.0,
                    Color::new(r, gc, b, t),
                    1.8,
                    -1.0,
                    0.35 * t,
                );
            }
        }
    } else {
        draw_spark(frame, cam, spark);
    }
}

fn paint_flash(gfx: Option<&Gfx>, frame: &Frame, cam: &Cam, flash: &Flash) {
    if let Some(g) = gfx {
        let (x, y) = world_to_screen(frame, cam, flash.pos);
        if x < -40.0 || x > frame.sw + 40.0 || y < -40.0 || y > frame.sh + 40.0 {
            return;
        }
        let t = (flash.life / flash.max_life).clamp(0.0, 1.0);
        let unit = frame.sh * 0.014 * cam.zoom;
        let rad = unit * (0.6 + 1.4 * (1.0 - t));
        g.soft_blob(
            x,
            y,
            rad * 2.2,
            Color::new(1.0, 0.85, 0.55, 0.45 * t),
            2.0,
            0.25,
            0.35 * t,
        );
    } else {
        draw_flash(frame, cam, flash);
    }
}

fn paint_mote(gfx: Option<&Gfx>, frame: &Frame, cam: &Cam, mote: &Mote) {
    if let Some(g) = gfx {
        let (x, y) = world_to_screen(frame, cam, mote.pos);
        if x < 0.0 || x > frame.sw || y < 0.0 || y > frame.sh {
            return;
        }
        let t = (mote.life / 0.35).clamp(0.0, 1.0);
        let (r, gc, b) = hsv(mote.hue, 0.7, 1.0);
        let unit = frame.sh * 0.004 * cam.zoom;
        let rad = ((1.3 + t) * unit).max(0.8);
        g.soft_blob(
            x,
            y,
            rad * 2.5,
            Color::new(r, gc, b, 0.5 * t),
            2.0,
            -1.0,
            0.2 * t,
        );
    } else {
        draw_mote(frame, cam, mote);
    }
}


fn draw_feeders(
    frame: &Frame,
    cam: &Cam,
    world: &World,
    selected: Option<usize>,
    feed_tool: bool,
    feed_kind: FoodKind,
    mouse: (f32, f32),
    ui_block: bool,
) {
    let scale = world_scale(frame, cam);
    for (i, feeder) in world.feeders().iter().enumerate() {
        let pos = feeder.pos;
        let (sx, sy) = world_to_screen(frame, cam, pos);
        let spec = world.food_spec(feeder.kind);
        let (cr, cg, cb) = spec.color;
        let lit = selected == Some(i);

        // Draw dispersion area circle when selected
        if lit {
            let pixel_radius = feeder.radius * scale;
            draw_circle(sx, sy, pixel_radius, Color::new(cr, cg, cb, 0.07));
            draw_circle_lines(sx, sy, pixel_radius, 1.4, Color::new(cr, cg, cb, 0.55));
        }

        // Feeder dispenser capsule / node
        let a = if feeder.enabled { 0.95 } else { 0.40 };
        let r = if lit { 11.0 } else { 9.0 };
        draw_circle(sx, sy, r + 4.0, Color::new(cr, cg, cb, 0.18 * a));
        draw_circle(sx, sy, r, Color::new(0.08, 0.12, 0.16, 0.92));
        draw_circle_lines(
            sx,
            sy,
            r,
            1.8,
            Color::new(cr, cg, cb, if lit { 1.0 } else { 0.7 } * a),
        );
        // Core indicator
        draw_circle(
            sx,
            sy,
            r * 0.45,
            Color::new(cr, cg, cb, if feeder.enabled { 0.95 } else { 0.35 }),
        );
        if feeder.enabled {
            let pulse = ((world.time() * 3.5).sin() * 0.5 + 0.5) * 2.5;
            draw_circle_lines(sx, sy, r + 1.0 + pulse, 1.0, Color::new(cr, cg, cb, 0.6));
        }
    }

    // Ghost preview when placing new feeder
    if feed_tool && !ui_block {
        let (sx, sy) = (mouse.0, mouse.1);
        let spec = world.food_spec(feed_kind);
        let (cr, cg, cb) = spec.color;
        let radius = 0.35;
        let pixel_radius = radius * scale;
        draw_circle(sx, sy, pixel_radius, Color::new(cr, cg, cb, 0.06));
        draw_circle_lines(sx, sy, pixel_radius, 1.2, Color::new(cr, cg, cb, 0.40));
        draw_circle(sx, sy, 10.0, Color::new(0.08, 0.12, 0.16, 0.85));
        draw_circle_lines(sx, sy, 10.0, 1.8, Color::new(cr, cg, cb, 0.80));
        draw_circle(sx, sy, 4.5, Color::new(cr, cg, cb, 0.85));
    }
}

fn draw_logo_dna(
    gfx: Option<&Gfx>,
    cx: f32,
    cy: f32,
    mouse: (f32, f32),
    time: f32,
    scale: f32,
    alpha: f32,
    depth_lo: f32,
    depth_hi: f32,
    // 0 = woven through the word, 1 = tall parked helix (setup).
    park: f32,
    // 0 = intact, 1 = debris flying toward camera.
    burst: f32,
) {
    let a = alpha.clamp(0.0, 1.0);
    if a < 0.02 {
        return;
    }
    let s = scale.clamp(0.55, 8.0);
    let park = park.clamp(0.0, 1.0);
    let burst = burst.clamp(0.0, 1.0);
    let burst_e = burst * burst;
    let weave = 1.0 - park;
    const N: usize = 56;

    // Through-word: wider than AETHER so strands overhang left/right. Parked: taller ribbon.
    let span = (620.0 * weave + 240.0 * park) * s;
    let amp = (38.0 * weave + 52.0 * park) * s;
    let twist = time * (0.55 + 0.12 * park);
    let turns = 2.05 + 0.35 * park;

    // Soft cursor tilt — keep it subtle so the helix turns, not slides.
    let mx = ((mouse.0 - cx) / 320.0).clamp(-1.0, 1.0);
    let my = ((mouse.1 - cy) / 260.0).clamp(-1.0, 1.0);
    let tilt_yaw = mx * (0.10 * weave + 0.07 * park) * (1.0 - burst);
    let tilt_pitch = -my * (0.08 * weave + 0.05 * park) * (1.0 - burst);

    // Weave: axis along X (through letters). Park: tip up with classic pitch/roll.
    let pitch = 0.10 * weave + 0.78 * park + tilt_pitch;
    let roll = -0.18 * weave + -0.88 * park;
    let yaw = 0.06 * weave + tilt_yaw;
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cr, sr) = (roll.cos(), roll.sin());
    let (cyaw, syaw) = (yaw.cos(), yaw.sin());

    let project = |lx: f32, ly: f32, lz: f32| -> (f32, f32, f32, f32) {
        // yaw → pitch → roll
        let x0 = lx * cyaw - lz * syaw;
        let z0 = lx * syaw + lz * cyaw;
        let y1 = ly * cp - z0 * sp;
        let z1 = ly * sp + z0 * cp;
        let x1 = x0;
        let persp = 1.0 / (1.0 + z1 * (0.0026 / s.max(0.5)));
        let xr = (x1 * cr - y1 * sr) * persp;
        let yr = (x1 * sr + y1 * cr) * persp;
        // Depth 0‥1 aligned with letter colony (left/back → right/front + helix spin).
        let along = ((lx / span.max(1.0)) + 0.5).clamp(0.0, 1.0);
        let spin = (lz / amp.max(1.0)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        let depth = (along * 0.42 + spin * 0.58).clamp(0.0, 1.0);
        (cx + xr, cy + yr, persp, depth)
    };

    let mut strand_a = [(0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32); N];
    let mut strand_b = [(0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32); N];
    for i in 0..N {
        let t = i as f32 / (N - 1) as f32;
        // Soft ends so helix melts into letter organisms instead of hard caps.
        let envelope = (t * std::f32::consts::PI).sin().powf(0.55 + 0.2 * weave);
        let organic = (time * 0.75 + t * 3.2).sin() * (2.6 * weave + 3.4 * park) * s * envelope;
        let phase = t * std::f32::consts::TAU * turns + twist;
        // Axis along span; weave sits mid-glyph, park rises from under the word.
        let axis = (t - 0.5) * span;
        let rise = (-0.06 * weave + (t - 0.42) * park) * (180.0 * s);
        let r = amp * (0.72 + 0.28 * envelope);
        let lx_a = axis + organic * 0.15;
        let ly_a = phase.sin() * r * 0.72 + rise;
        let lz_a = phase.cos() * r;
        let lx_b = axis - organic * 0.15;
        let ly_b = -phase.sin() * r * 0.72 + rise;
        let lz_b = -phase.cos() * r;
        let (mut sx, mut sy, mut p, d) = project(lx_a, ly_a, lz_a);
        let (mut sx2, mut sy2, mut p2, d2) = project(lx_b, ly_b, lz_b);

        // Shatter beads toward the camera in random directions.
        if burst_e > 0.001 {
            let (dx, dy, dz) = burst_dir(900 + i as u32);
            let (dx2, dy2, dz2) = burst_dir(1900 + i as u32);
            let fly = burst_e * (220.0 + burst_hash(i as u32) * 340.0) * s.max(1.0);
            let fly2 = burst_e * (200.0 + burst_hash(500 + i as u32) * 360.0) * s.max(1.0);
            sx += dx * fly;
            sy += dy * fly;
            p *= 1.0 + dz * burst_e * 4.5;
            sx2 += dx2 * fly2;
            sy2 += dy2 * fly2;
            p2 *= 1.0 + dz2 * burst_e * 4.5;
        }

        let thick = ((7.5 + 5.5 * envelope) * weave + (11.0 + 7.0 * envelope) * park) * s * p;
        strand_a[i] = (sx, sy, thick, d);
        strand_b[i] = (
            sx2,
            sy2,
            thick.max(2.8 * s) * (p2 / p.max(1e-3)).clamp(0.65, 1.35),
            d2,
        );
    }

    let in_band = |d: f32| d >= depth_lo && d < depth_hi;
    let edge_a = |d: f32| ((d - depth_lo).min(depth_hi - d) / 0.1).clamp(0.0, 1.0);
    // Seamless traveling hue wave (sin) — no hard wrap between cycle ends.
    let helix_rgb = |t: f32, hue0: f32, span: f32| {
        let phase = t * std::f32::consts::TAU - time * 0.41;
        let u = 0.5 + 0.5 * phase.sin();
        hsv(hue0 + u * span, 0.72, 0.92)
    };
    let burst_fade = (1.0 - burst_e * 0.75).max(0.15);

    if let Some(g) = gfx {
        g.begin_glow();
        let paint_tube = |pts: &[(f32, f32, f32, f32)], hue0: f32| {
            for (i, &(x, y, thick, d)) in pts.iter().enumerate() {
                if !in_band(d) {
                    continue;
                }
                let fa = a * edge_a(d) * burst_fade;
                if fa < 0.01 {
                    continue;
                }
                let t = i as f32 / (N - 1).max(1) as f32;
                let (cr, cg, cb) = helix_rgb(t, hue0, 0.26);
                let near = (0.55 + 0.45 * d).clamp(0.4, 1.15);
                g.soft_blob_cont(
                    x,
                    y,
                    thick * (2.2 * weave + 2.5 * park) * near,
                    Color::new(cr, cg, cb, (0.05 * weave + 0.055 * park) * fa * near),
                    2.1,
                    -1.0,
                    0.0,
                );
                g.soft_blob_cont(
                    x,
                    y,
                    thick * 1.5 * near,
                    Color::new(cr, cg, cb, 0.17 * fa * near),
                    1.55,
                    -1.0,
                    0.0,
                );
                g.soft_blob_cont(
                    x - thick * 0.1,
                    y - thick * 0.08,
                    thick * 0.8 * near,
                    Color::new(
                        cr * 0.4 + 0.6,
                        cg * 0.4 + 0.6,
                        cb * 0.4 + 0.6,
                        0.11 * fa * near,
                    ),
                    1.35,
                    -1.0,
                    0.45,
                );
            }
        };
        paint_tube(&strand_a, 0.36);
        paint_tube(&strand_b, 0.40);

        // Base-pair rungs — denser beads along a wider helix (skip when shattered).
        let rung_step = if weave > 0.5 { 1 } else { 2 };
        if burst_e < 0.12 {
        for i in (1..N - 1).step_by(rung_step) {
            let (x1, y1, t1, d1) = strand_a[i];
            let (x2, y2, t2, d2) = strand_b[i];
            let dmid = (d1 + d2) * 0.5;
            if !in_band(dmid) {
                continue;
            }
            let fa = a * edge_a(dmid) * burst_fade;
            if fa < 0.01 {
                continue;
            }
            let t = i as f32 / (N - 1).max(1) as f32;
            let (cr, cg, cb) = helix_rgb(t, 0.38, 0.24);
            let beads = if weave > 0.5 { 6 } else { 7 };
            for b in 0..=beads {
                let u = b as f32 / beads as f32;
                let bow = 1.0 - ((u - 0.5) * 2.0).powi(2);
                let x = x1 + (x2 - x1) * u;
                let y = y1 + (y2 - y1) * u;
                let rad = (t1 + (t2 - t1) * u) * (0.18 + 0.11 * bow);
                g.soft_blob_cont(
                    x,
                    y,
                    rad * 2.0,
                    Color::new(cr, cg, cb, 0.075 * fa),
                    1.85,
                    -1.0,
                    0.0,
                );
            }
        }
        } // end rungs (skipped while bursting)
        g.end_glow();
    } else {
        for &(x, y, thick, d) in strand_a.iter().chain(strand_b.iter()) {
            if !in_band(d) {
                continue;
            }
            let fa = a * edge_a(d) * burst_fade;
            if fa < 0.02 {
                continue;
            }
            draw_circle(x, y, thick * 0.4, Color::new(0.35, 0.92, 0.9, 0.4 * fa));
        }
    }
}


fn draw_food_button(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    active: bool,
    kind: FoodKind,
    hover_t: f32,
) {
    let (x, y, w, h) = food_button_rect(frame);
    let (cr, cg, cb) = FoodSpec::defaults()[kind.index()].color;
    let (hot, cx, cy) = paint_round_tool(x, y, w, h, mouse, active, (cr, cg, cb), true, hover_t);
    let lit = active || hot || hover_t > 0.4;
    let blob = if lit {
        Color::new(cr, cg, cb, 1.0)
    } else {
        Color::new(cr * 0.7, cg * 0.7, cb * 0.7, 0.95)
    };
    let leaf = if lit {
        Color::new((cr + 0.2).min(1.0), (cg + 0.15).min(1.0), (cb + 0.1).min(1.0), 1.0)
    } else {
        Color::new(cr * 0.85, cg * 0.85, cb * 0.85, 0.95)
    };
    draw_circle(cx - 8.0, cy + 4.0, 14.0, blob);
    draw_circle(cx + 10.0, cy + 2.0, 11.0, blob);
    draw_circle(cx + 1.0, cy - 12.0, 9.0, leaf);
    draw_circle(cx - 12.0, cy - 2.0, 3.0, Color::new(0.9, 1.0, 0.85, 0.55));
    tool_label(font, "Krmítko", x, y, w, h, lit);
}

#[allow(dead_code)]
fn draw_dish_button(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    active: bool,
    hover_t: f32,
) {
    let (x, y, w, h) = dish_button_rect(frame);
    let (hot, cx, cy) =
        paint_round_tool(x, y, w, h, mouse, active, (0.55, 0.92, 0.88), true, hover_t);
    let lit = active || hot || hover_t > 0.4;
    let ink = if lit {
        Color::new(0.72, 0.98, 0.94, 1.0)
    } else {
        Color::new(0.45, 0.72, 0.7, 0.95)
    };
    let glow = if lit {
        Color::new(0.28, 0.9, 0.82, 0.35)
    } else {
        Color::new(0.2, 0.55, 0.52, 0.22)
    };
    // Nested square = petri dish silhouette.
    draw_rectangle(cx - 16.0, cy - 12.0, 32.0, 24.0, glow);
    draw_rectangle_lines(cx - 16.0, cy - 12.0, 32.0, 24.0, 2.0, ink);
    draw_rectangle_lines(cx - 10.0, cy - 7.0, 20.0, 14.0, 1.4, ink);
    draw_circle(cx + 14.0, cy - 14.0, 5.5, Color::new(0.35, 0.95, 0.85, if lit { 0.95 } else { 0.55 }));
    tool_label(font, "Přidat misku", x, y, w, h, lit);
}

#[allow(dead_code)]
fn draw_dish_ghost(
    frame: &Frame,
    cam: &Cam,
    center: Vec2,
    half_x: f32,
    half_y: f32,
    valid: bool,
) {
    let (line, fill) = if valid {
        (
            Color::new(0.55, 0.98, 0.9, 0.75),
            Color::new(0.2, 0.75, 0.7, 0.08),
        )
    } else {
        (
            Color::new(0.95, 0.35, 0.4, 0.8),
            Color::new(0.7, 0.15, 0.2, 0.1),
        )
    };
    let (x, y, w, h) = dish_screen_rect_at(frame, cam, center, half_x, half_y);
    let s = world_scale(frame, cam);
    let r = px(W_DISH_CORNER.min(half_x * 0.4).min(half_y * 0.4), s);
    fill_round_rect(x, y, w, h, r, fill);
    stroke_round_rect(x, y, w, h, r, (px(0.008, s)).max(2.0), line);
}

#[allow(dead_code)]
fn draw_food_ghost(
    frame: &Frame,
    cam: &Cam,
    mouse: (f32, f32),
    sense: f32,
    color: (f32, f32, f32),
) {
    paint_food_ghost(None, frame, cam, mouse, sense, color);
}

fn draw_food(
    frame: &Frame,
    cam: &Cam,
    pos: Vec2,
    sense: f32,
    color: (f32, f32, f32),
    bloom: f32,
    fade: Option<f32>,
) {
    let (x, y) = world_to_screen(frame, cam, pos);
    let scale = world_scale(frame, cam);
    let (cr, cg, cb) = color;
    let bloom_e = {
        let t = bloom.clamp(0.0, 1.0);
        1.0 - (1.0 - t) * (1.0 - t)
    };
    let outer = (sense * bloom_e * scale).max(if bloom_e > 0.02 { 6.0 } else { 0.0 });
    let fade_u = fade.unwrap_or(-0.05);
    if outer > 0.5 {
        let layers = 20;
        for i in (0..layers).rev() {
            let u = (i as f32 + 1.0) / layers as f32;
            let rad = outer * u;
            let fall = 1.0 - u;
            let mut a = fall * fall * 0.12;
            // Fade clears from the center outward: inner rings die first.
            if fade_u >= 0.0 {
                let clear = ((u - fade_u) / 0.22).clamp(0.0, 1.0);
                a *= clear * clear;
            }
            if a < 0.004 {
                continue;
            }
            draw_circle(x, y, rad, Color::new(cr, cg, cb, a));
        }
    }
    // Core dot vanishes quickly once eaten.
    let core_a = if fade_u < 0.0 {
        1.0
    } else {
        (1.0 - fade_u / 0.18).clamp(0.0, 1.0)
    };
    if core_a > 0.02 {
        let core = (0.012 * scale).max(2.2);
        draw_circle(x, y, core * 1.8, Color::new(cr, cg, cb, 0.35 * core_a));
        draw_circle(
            x,
            y,
            core,
            Color::new(cr * 0.85, cg * 0.9, cb * 0.85, 0.95 * core_a),
        );
        draw_circle(
            x,
            y,
            core * 0.45,
            Color::new(
                (cr + 0.45).min(1.0),
                (cg + 0.35).min(1.0),
                (cb + 0.35).min(1.0),
                core_a,
            ),
        );
    }
}

fn hit_kind_tab(mouse: (f32, f32), tabs: &[(f32, f32, f32, f32); 3], edit: &mut u8) -> bool {
    for (i, rect) in tabs.iter().enumerate() {
        if hit_rect(mouse, *rect) {
            *edit = i as u8;
            return true;
        }
    }
    false
}

fn draw_liquid_bg(
    frame: &Frame,
    cam: &Cam,
    half_x: f32,
    half_y: f32,
    time: f32,
    floor_light: Option<((f32, f32), f32)>,
    dishes: &[PetriDish],
) {
    let corners = [
        Vec2::new(-half_x, -half_y),
        Vec2::new(half_x, -half_y),
        Vec2::new(half_x, half_y),
        Vec2::new(-half_x, half_y),
    ];
    let pts: Vec<(f32, f32)> = corners
        .iter()
        .map(|p| world_to_screen(frame, cam, *p))
        .collect();
    let min_x = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    let dw = (max_x - min_x).max(1.0);
    let dh = (max_y - min_y).max(1.0);

    // Deep water base
    draw_rectangle(min_x, min_y, dw, dh, Color::new(0.018, 0.055, 0.072, 1.0));

    // Soft depth: clearer center, darker rim
    for i in 0..8 {
        let u = i as f32 / 7.0;
        let inset_x = dw * 0.06 * u;
        let inset_y = dh * 0.06 * u;
        draw_rectangle(
            min_x + inset_x,
            min_y + inset_y,
            dw - inset_x * 2.0,
            dh - inset_y * 2.0,
            Color::new(
                0.03 + u * 0.04,
                0.1 + u * 0.08,
                0.12 + u * 0.07,
                0.055,
            ),
        );
    }

    let scale = world_scale(frame, cam);

    // Slow cloudy water volume (world-space, zooms with the camera — kept outside dishes).
    let volumes = [
        (0.12, 0.18, 0.55, 0.11, 0.4, 0.04, 0.14, 0.16),
        (-0.28, -0.2, 0.48, 0.09, 1.1, 0.03, 0.12, 0.15),
        (0.35, -0.3, 0.42, 0.13, 2.0, 0.05, 0.15, 0.17),
        (-0.4, 0.32, 0.5, 0.08, 2.7, 0.035, 0.13, 0.16),
        (0.05, 0.05, 0.7, 0.06, 0.2, 0.025, 0.11, 0.14),
        (0.48, 0.22, 0.38, 0.1, 1.6, 0.04, 0.14, 0.15),
        (-0.15, 0.45, 0.4, 0.07, 3.2, 0.03, 0.12, 0.15),
    ];
    for (bx, by, rad, spd, phase, cr, cg, cb) in volumes {
        let ox = (time * spd + phase).sin() * 0.1;
        let oy = (time * spd * 0.7 + phase * 1.3).cos() * 0.08;
        let p = Vec2::new(bx + ox, by + oy);
        // Exclude volume clouds that fall inside any dish
        let inside_dish = dishes.iter().any(|d| {
            let rel = p - d.pos;
            rel.x.abs() < d.half_x && rel.y.abs() < d.half_y
        });
        if inside_dish {
            continue;
        }
        let breathe = 1.0 + 0.07 * (time * 0.35 + phase).sin();
        let (sx, sy) = world_to_screen(frame, cam, p);
        let r = rad * breathe * scale;
        for i in (0..14).rev() {
            let u = (i as f32 + 1.0) / 14.0;
            let a = 0.012 * (1.0 - u) * (1.0 - u);
            draw_circle(sx, sy, r * u, Color::new(cr, cg, cb, a));
        }
    }

    // Underwater caustics — soft bright pools (no streaks)
    let caustics = [
        (0.2, 0.15, 0.22, 0.22, 0.5),
        (-0.3, 0.1, 0.18, 0.18, 1.3),
        (0.1, -0.35, 0.2, 0.25, 2.1),
        (-0.15, -0.1, 0.16, 0.2, 2.8),
        (0.4, -0.15, 0.17, 0.19, 0.9),
        (-0.45, -0.35, 0.19, 0.17, 1.8),
        (0.25, 0.4, 0.15, 0.21, 3.4),
        (-0.05, 0.3, 0.21, 0.16, 0.3),
        (0.5, 0.05, 0.14, 0.23, 2.4),
        (-0.35, 0.4, 0.16, 0.18, 1.5),
        (0.0, 0.0, 0.25, 0.14, 3.9),
        (0.3, -0.45, 0.13, 0.2, 0.7),
    ];
    for (bx, by, rad, spd, phase) in caustics {
        let wave = time * spd + phase;
        let ox = wave.sin() * 0.14 + (wave * 0.6 + 1.0).cos() * 0.06;
        let oy = (wave * 0.85).cos() * 0.12 + (wave * 0.4).sin() * 0.05;
        let pulse = 0.85 + 0.15 * (wave * 1.3).sin();
        let (sx, sy) = world_to_screen(frame, cam, Vec2::new(bx + ox, by + oy));
        let r = rad * pulse * scale;
        for i in (0..8).rev() {
            let u = (i as f32 + 1.0) / 8.0;
            let fall = 1.0 - u;
            let a = 0.018 * fall * fall * pulse;
            draw_circle(
                sx,
                sy,
                r * u,
                Color::new(0.45 + fall * 0.25, 0.85, 0.9, a),
            );
        }
        draw_circle(
            sx,
            sy,
            r * 0.28,
            Color::new(0.75, 0.95, 0.98, 0.016 * pulse),
        );
    }

    // Surface ripples — expanding soft rings
    for i in 0..5 {
        let seed = i as f32 * 1.7;
        let life = ((time * 0.22 + seed) * 0.35).fract();
        let px = (seed * 1.9).sin() * half_x * 0.55;
        let py = (seed * 2.3).cos() * half_y * 0.5;
        let (sx, sy) = world_to_screen(frame, cam, Vec2::new(px, py));
        let rad = (0.04 + life * 0.22) * scale;
        let a = (1.0 - life) * (1.0 - life) * 0.03;
        draw_circle_lines(sx, sy, rad, 1.5, Color::new(0.55, 0.85, 0.9, a));
        draw_circle_lines(
            sx,
            sy,
            rad * 0.72,
            1.0,
            Color::new(0.4, 0.75, 0.82, a * 0.6),
        );
    }

    // Microbubbles / suspended particles (kept outside dishes)
    for i in 0..18 {
        let seed = i as f32 * 0.73;
        let drift = time * (0.04 + (i % 5) as f32 * 0.01);
        let px = ((seed * 5.1 + drift).sin() * 0.7) * half_x;
        let py = ((seed * 3.7 + drift * 0.8).cos() * 0.65) * half_y;
        let inside_dish = dishes.iter().any(|d| {
            let rel = Vec2::new(px, py) - d.pos;
            rel.x.abs() < d.half_x && rel.y.abs() < d.half_y
        });
        if inside_dish {
            continue;
        }
        let (sx, sy) = world_to_screen(frame, cam, Vec2::new(px, py));
        let twinkle = 0.5 + 0.5 * (time * 1.4 + seed * 2.0).sin();
        let rad = (1.2 + (i % 3) as f32 * 0.6) * cam.zoom.sqrt().max(0.7);
        draw_circle(
            sx,
            sy,
            rad,
            Color::new(0.7, 0.92, 0.95, 0.018 + 0.016 * twinkle),
        );
    }

    // Soft rim shade (stay teal — no black bars).
    for i in 0..3 {
        let u = (i as f32 + 1.0) / 3.0;
        let t = 0.018 * u;
        let col = Color::new(0.03, 0.09, 0.10, 0.06 * (1.0 - u * 0.4));
        draw_rectangle(min_x, min_y, dw, dh * t, col);
        draw_rectangle(min_x, max_y - dh * t, dw, dh * t, col);
        draw_rectangle(min_x, min_y, dw * t, dh, col);
        draw_rectangle(max_x - dw * t, min_y, dw * t, dh, col);
    }

    // CPU fallback: brighten micro-dots and thin green gel under the cursor.
    if let Some(((lx, ly), radius)) = floor_light {
        for i in (0..7).rev() {
            let u = (i as f32 + 1.0) / 7.0;
            let fall = (1.0 - u) * (1.0 - u);
            draw_circle(
                lx,
                ly,
                radius * u,
                Color::new(0.10, 0.18, 0.20, 0.055 * fall),
            );
        }
        for i in 0..28 {
            let seed = i as f32 * 1.37 + time * 0.15;
            let ang = seed * 2.3;
            let dist = ((seed * 0.71).fract()) * radius * 0.95;
            let px = lx + ang.cos() * dist;
            let py = ly + ang.sin() * dist * 0.88;
            let tw = 0.5 + 0.5 * (time * 2.1 + seed).sin();
            let fall = (1.0 - dist / radius.max(1.0)).clamp(0.0, 1.0);
            draw_circle(
                px,
                py,
                0.9 + 0.6 * tw,
                Color::new(0.75, 0.98, 1.0, (0.03 + 0.05 * tw) * fall * fall),
            );
        }
    }
}

fn draw_edge_zones(frame: &Frame, cam: &Cam, half_x: f32, half_y: f32, edges: [EdgeZone; 4]) {
    let scale = world_scale(frame, cam);
    for (i, zone) in edges.iter().enumerate() {
        if zone.effect == EdgeEffect::None || zone.reach < 0.04 {
            continue;
        }
        let (cr, cg, cb) = zone.effect.tint();
        let layers = 8;
        for layer in 0..layers {
            let u = (layer as f32 + 1.0) / layers as f32;
            let depth = zone.reach * u;
            let a = (1.0 - u) * (1.0 - u) * 0.09;
            let col = Color::new(cr, cg, cb, a);
            match i {
                0 => {
                    // left
                    let (x0, y0) = world_to_screen(frame, cam, Vec2::new(-half_x, -half_y));
                    let (x1, y1) = world_to_screen(frame, cam, Vec2::new(-half_x + depth, half_y));
                    draw_rectangle(
                        x0.min(x1),
                        y0.min(y1),
                        (x0 - x1).abs().max(1.0),
                        (y0 - y1).abs().max(1.0),
                        col,
                    );
                }
                1 => {
                    let (x0, y0) = world_to_screen(frame, cam, Vec2::new(half_x - depth, -half_y));
                    let (x1, y1) = world_to_screen(frame, cam, Vec2::new(half_x, half_y));
                    draw_rectangle(
                        x0.min(x1),
                        y0.min(y1),
                        (x0 - x1).abs().max(1.0),
                        (y0 - y1).abs().max(1.0),
                        col,
                    );
                }
                2 => {
                    let (x0, y0) = world_to_screen(frame, cam, Vec2::new(-half_x, -half_y));
                    let (x1, y1) = world_to_screen(frame, cam, Vec2::new(half_x, -half_y + depth));
                    draw_rectangle(
                        x0.min(x1),
                        y0.min(y1),
                        (x0 - x1).abs().max(1.0),
                        (y0 - y1).abs().max(1.0),
                        col,
                    );
                }
                _ => {
                    let (x0, y0) = world_to_screen(frame, cam, Vec2::new(-half_x, half_y - depth));
                    let (x1, y1) = world_to_screen(frame, cam, Vec2::new(half_x, half_y));
                    draw_rectangle(
                        x0.min(x1),
                        y0.min(y1),
                        (x0 - x1).abs().max(1.0),
                        (y0 - y1).abs().max(1.0),
                        col,
                    );
                }
            }
            let _ = scale;
        }
    }
}

#[allow(dead_code)]
fn draw_extra_dishes_and_tubes(frame: &Frame, cam: &Cam, world: &World) {
    // Dish rims come from paint_dish_cutout; only tubes here.
    for tube in world.tubes() {
        let Some(a) = world.dishes().iter().find(|d| d.id == tube.a_dish) else {
            continue;
        };
        let Some(b) = world.dishes().iter().find(|d| d.id == tube.b_dish) else {
            continue;
        };
        let pa = a.to_table(a.rim_pos(tube.a_side, tube.a_along));
        let pb = b.to_table(b.rim_pos(tube.b_side, tube.b_along));
        let (x0, y0) = world_to_screen(frame, cam, pa);
        let (x1, y1) = world_to_screen(frame, cam, pb);
        draw_line(x0, y0, x1, y1, 5.0, Color::new(0.55, 0.75, 0.9, 0.22));
        draw_line(x0, y0, x1, y1, 1.8, Color::new(0.75, 0.92, 1.0, 0.7));
        draw_circle(x0, y0, 4.5, Color::new(0.7, 0.95, 1.0, 0.55));
        draw_circle(x1, y1, 4.5, Color::new(0.7, 0.95, 1.0, 0.55));
    }
}

/// Size handles on dish edges: 0 = šířka (right), 1 = výška (bottom).
fn draw_hover_ring(frame: &Frame, cam: &Cam, app: &Appearance<'_>, t: f32) {
    if app.nodes.is_empty() || t < 0.02 {
        return;
    }
    let s = world_scale(frame, cam);
    let (hr, hg, hb) = hsv(app.hue, 0.55, 1.0);
    let mut cx = 0.0;
    let mut cy = 0.0;
    for node in app.nodes {
        let (x, y) = world_to_screen(frame, cam, *node + app.dish_pos);
        cx += x;
        cy += y;
    }
    let n = app.nodes.len() as f32;
    cx /= n;
    cy /= n;
    let mut rad = 0.0f32;
    for (i, node) in app.nodes.iter().enumerate() {
        let (x, y) = world_to_screen(frame, cam, *node + app.dish_pos);
        let node_r = app.node_radius * if i == 0 { 1.7 } else { 1.35 } * s;
        rad = rad.max(((x - cx).hypot(y - cy) + node_r + 8.0) * (0.94 + 0.1 * t));
    }
    let a = smoother(t);
    draw_circle(cx, cy, rad * 1.08, Color::new(0.35, 1.0, 0.92, 0.05 + 0.1 * a));
    draw_circle_lines(
        cx,
        cy,
        rad,
        2.2,
        Color::new(0.45, 1.0, 0.95, 0.35 + 0.55 * a),
    );
    draw_circle_lines(
        cx,
        cy,
        rad + 3.5,
        1.15,
        Color::new(hr, hg, hb, 0.2 + 0.45 * a),
    );
    // Per-node ticks so long chains read clearly.
    for (i, node) in app.nodes.iter().enumerate() {
        let (x, y) = world_to_screen(frame, cam, *node + app.dish_pos);
        let node_r = app.node_radius * if i == 0 { 1.7 } else { 1.35 } * s;
        draw_circle_lines(
            x,
            y,
            node_r + 3.0 + 2.0 * a,
            1.2,
            Color::new(0.75, 1.0, 0.98, 0.25 + 0.45 * a),
        );
    }
}

fn draw_select_glow(frame: &Frame, cam: &Cam, app: &Appearance<'_>, t: f32, time: f32) {
    if app.nodes.is_empty() || t < 0.02 {
        return;
    }
    let s = world_scale(frame, cam);
    let (r, g, b) = hsv(app.hue, 0.9, 1.0);
    let breathe = 1.0 + 0.045 * (time * 1.7).sin() * t;
    let alpha = 0.15 + 0.7 * t;
    for (i, node) in app.nodes.iter().enumerate() {
        let (x, y) = world_to_screen(frame, cam, *node + app.dish_pos);
        let base = app.node_radius * if i == 0 { 1.7 } else { 1.45 } * s;
        let rad = (base * breathe + 5.0 * t).max(4.0);
        draw_circle_lines(x, y, rad, 1.6, Color::new(r, g, b, alpha));
        draw_circle(x, y, rad + 3.5 * t, Color::new(r, g, b, 0.08 * t));
    }
}

fn center_text(font: &Option<Font>, label: &str, cx: f32, y: f32, size: u16, color: Color) {
    let width = measure_label(font, label, size);
    text(font, label, cx - width * 0.5, y, size, color);
}

fn measure_label(font: &Option<Font>, label: &str, size: u16) -> f32 {
    measure_text(label, font.as_ref(), size, 1.0).width.max(1.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Info,
    Genom,
}

struct DetailHits {
    info: (f32, f32, f32, f32),
    genom: (f32, f32, f32, f32),
}

fn draw_inspect_right(
    frame: &Frame,
    font: &Option<Font>,
    s: &Stats,
    t: f32,
    tab: DetailTab,
    mouse: (f32, f32),
) -> ((f32, f32, f32, f32), DetailHits) {
    let alpha = smoother(t).clamp(0.0, 1.0);
    let w = (frame.sw * 0.32).clamp(280.0, 400.0);
    let x = frame.sw - w * alpha;
    let y = 0.0;
    let h = frame.sh;
    draw_rectangle(x, y, w, h, Color::new(0.02, 0.035, 0.055, 0.9 * alpha));
    draw_rectangle(x, y, 1.0, h, Color::new(0.28, 0.9, 0.82, 0.45 * alpha));
    let ink = Color::new(0.92, 0.96, 0.98, alpha);
    let dim = Color::new(0.55, 0.72, 0.78, 0.92 * alpha);
    let (r, g, b) = hsv(s.hue, 0.55, 0.9);
    let tx = x + 20.0;
    let inner = w - 40.0;
    let mut yy = 22.0;
    let z = (0.0, 0.0, 0.0, 0.0);
    let mut hits = DetailHits { info: z, genom: z };

    draw_circle(tx + 8.0, yy + 8.0, 10.0, Color::new(r, g, b, 0.22 * alpha));
    draw_circle(tx + 8.0, yy + 8.0, 6.0, Color::new(r, g, b, alpha));
    text(font, &format!("Jedinec {}", s.id), tx + 24.0, yy + 7.0, 18, ink);
    yy += 24.0;
    text(
        font,
        &format!("generace {}", s.generation),
        tx + 24.0,
        yy,
        13,
        Color::new(0.28, 0.9, 0.82, 0.95 * alpha),
    );
    yy += 20.0;

    let tabs = [(DetailTab::Info, "Info"), (DetailTab::Genom, "Genom")];
    let gap = 6.0;
    let tab_w = ((inner - gap) / 2.0).max(80.0);
    let tab_h = 28.0;
    let tab_y = yy;
    for (i, (kind, label)) in tabs.iter().enumerate() {
        let tx_tab = tx + i as f32 * (tab_w + gap);
        let rect = (tx_tab, tab_y, tab_w, tab_h);
        match kind {
            DetailTab::Info => hits.info = rect,
            DetailTab::Genom => hits.genom = rect,
        }
        let active = *kind == tab;
        let hot = hit_rect(mouse, rect);
        let bg = if active {
            Color::new(0.08, 0.22, 0.24, 0.98 * alpha)
        } else if hot {
            Color::new(0.06, 0.14, 0.16, 0.95 * alpha)
        } else {
            Color::new(0.04, 0.08, 0.1, 0.85 * alpha)
        };
        draw_rectangle(tx_tab, tab_y, tab_w, tab_h, bg);
        if active {
            draw_rectangle(
                tx_tab,
                tab_y + tab_h - 2.0,
                tab_w,
                2.0,
                Color::new(0.28, 0.9, 0.82, 0.95 * alpha),
            );
        }
        let label_col = if active {
            Color::new(0.92, 0.98, 0.96, alpha)
        } else {
            Color::new(0.28, 0.9, 0.82, 0.85 * alpha)
        };
        center_text(font, label, tx_tab + tab_w * 0.5, tab_y + tab_h * 0.55, 13, label_col);
    }
    yy += tab_h + 12.0;

    match tab {
        DetailTab::Info => {
            let body_reserve = 210.0;
            let state_limit = (frame.sh - body_reserve).max(yy + 80.0);
            if yy < frame.sh - 40.0 {
                yy = draw_info_state(font, tx, yy, inner, s, alpha, state_limit);
            }
            if yy < frame.sh - 40.0 {
                let _ = draw_info_body(font, tx, yy, inner, s, ink, dim, alpha);
            }
        }
        DetailTab::Genom => {
            if yy < frame.sh - 40.0 {
                let _ = draw_genom_card(font, tx, yy, inner, s, ink, dim, alpha);
            }
        }
    }
    ((x, y, w, h), hits)
}

fn draw_info_state(
    font: &Option<Font>,
    x: f32,
    mut y: f32,
    w: f32,
    s: &Stats,
    alpha: f32,
    y_limit: f32,
) -> f32 {
    text(
        font,
        "Stav",
        x,
        y + 11.0,
        12,
        Color::new(0.28, 0.9, 0.82, 0.9 * alpha),
    );
    y += 16.0;
    let rows: [(&str, f32, String, Color, bool); 11] = [
        (
            "Energie",
            s.energy / 2.0,
            format!("{:.2}", s.energy),
            Color::new(0.25, 0.95, 0.62, alpha),
            false,
        ),
        (
            "Hlad",
            (s.hunger / 1.4).clamp(0.0, 1.0),
            format!("{:.2}", s.hunger),
            Color::new(0.95, 0.62, 0.28, alpha),
            false,
        ),
        (
            "Bolest",
            s.pain,
            format!("{:.2}", s.pain),
            Color::new(0.95, 0.32, 0.42, alpha),
            false,
        ),
        (
            "Tep",
            s.heart,
            format!("{:.2}", s.heart),
            Color::new(0.45, 0.95, 0.9, alpha),
            true,
        ),
        (
            "Novinka",
            s.novelty.clamp(0.0, 1.0),
            format!("{:.2}", s.novelty),
            Color::new(0.72, 0.58, 0.98, alpha),
            false,
        ),
        (
            "Potomek",
            s.repro.clamp(0.0, 1.0),
            format!("{:.2}", s.repro),
            Color::new(0.95, 0.78, 0.35, alpha),
            false,
        ),
        (
            "Impulz",
            s.pulse.clamp(-1.0, 1.0),
            format!("{:.2}", s.pulse),
            Color::new(0.35, 0.82, 0.95, alpha),
            true,
        ),
        (
            "Chuť zelené",
            s.food_taste[0].clamp(-1.0, 1.0),
            format!("{:.2}", s.food_taste[0]),
            Color::new(0.35, 0.9, 0.45, alpha),
            true,
        ),
        (
            "Chuť zlaté",
            s.food_taste[1].clamp(-1.0, 1.0),
            format!("{:.2}", s.food_taste[1]),
            Color::new(0.95, 0.75, 0.28, alpha),
            true,
        ),
        (
            "Chuť jedovaté",
            s.food_taste[2].clamp(-1.0, 1.0),
            format!("{:.2}", s.food_taste[2]),
            Color::new(0.78, 0.4, 0.95, alpha),
            true,
        ),
        (
            "Plasticita",
            (s.learn / 0.4).clamp(0.0, 1.0),
            format!("{:.2}", s.learn),
            Color::new(0.55, 0.75, 0.98, alpha),
            false,
        ),
    ];
    for (label, value, txt, col, centered) in rows {
        if y + 24.0 > y_limit {
            break;
        }
        meter_compact(font, x, &mut y, w, label, value, &txt, col, alpha, centered);
    }
    y + 4.0
}

fn draw_info_body(
    font: &Option<Font>,
    x: f32,
    mut y: f32,
    w: f32,
    s: &Stats,
    ink: Color,
    dim: Color,
    alpha: f32,
) -> f32 {
    text(
        font,
        "Tělo a barva",
        x,
        y + 11.0,
        12,
        Color::new(0.28, 0.9, 0.82, 0.9 * alpha),
    );
    y += 18.0;

    // Compact color strip + blurb.
    let (r, g, b) = hsv(s.hue, 0.7, 0.95);
    let (name, blurb) = hue_label(s.hue);
    draw_circle(x + 10.0, y + 10.0, 10.0, Color::new(r, g, b, 0.25 * alpha));
    draw_circle(x + 10.0, y + 10.0, 6.5, Color::new(r, g, b, alpha));
    text(font, name, x + 26.0, y + 6.0, 13, ink);
    text(
        font,
        &format!("hue {:.2}", s.hue),
        x + 26.0,
        y + 20.0,
        11,
        dim,
    );
    y += 34.0;
    let chars = ((w / 6.6).floor() as usize).clamp(24, 48);
    let mut lines = 0;
    for line in wrap_words(blurb, chars) {
        text(font, &line, x, y + 11.0, 11, dim);
        y += 13.0;
        lines += 1;
        if lines >= 2 {
            break;
        }
    }
    let dh = {
        let d = (s.hue - s.root_hue).abs();
        d.min(1.0 - d)
    };
    text(
        font,
        &format!(
            "Posun od gen 0: {:.2} · {}",
            dh,
            if dh < 0.03 {
                "stejná linie"
            } else if dh < 0.12 {
                "mírný posun"
            } else {
                "jiná větev"
            }
        ),
        x,
        y + 12.0,
        11,
        Color::new(0.28, 0.9, 0.82, 0.9 * alpha),
    );
    y += 22.0;

    // Model + short character blurb.
    y = draw_model_compact(font, x, y, w, s, alpha);
    let (title, about) = character(s);
    text(font, &title, x, y + 12.0, 13, ink);
    y += 16.0;
    let mut lines = 0;
    for line in wrap_words(&about, chars) {
        text(font, &line, x, y + 11.0, 11, dim);
        y += 13.0;
        lines += 1;
        if lines >= 3 {
            break;
        }
    }
    y + 4.0
}

fn draw_genom_card(
    font: &Option<Font>,
    x: f32,
    mut y: f32,
    w: f32,
    s: &Stats,
    ink: Color,
    dim: Color,
    alpha: f32,
) -> f32 {
    text(
        font,
        &format!(
            "Drift od zakladatele · {:.0} %",
            s.gene_drift * 100.0
        ),
        x,
        y + 12.0,
        13,
        Color::new(0.28, 0.9, 0.82, 0.95 * alpha),
    );
    y += 20.0;
    if s.generation == 0 {
    text(
        font,
            "Generace 0 — zakladatel linie.",
        x,
            y + 12.0,
            11,
        dim,
    );
        y += 22.0;
    }

    // Snapshot stats (no root pair).
    text(font, "Údaje", x, y + 11.0, 12, Color::new(0.28, 0.9, 0.82, 0.9 * alpha));
    y += 16.0;
    for (label, value) in [
        ("Věk", format!("{:.1} s", s.age)),
        ("Neurony", format!("{}", s.neurons)),
        ("Synapse", format!("{}", s.synapses)),
        ("Tonus", format!("{:.2}", s.tonic)),
    ] {
        info_row(font, x, &mut y, w, label, &value, ink, dim, alpha);
    }

    y += 6.0;
    text(
        font,
        "Znaky · teď / gen 0",
        x,
        y + 11.0,
        12,
        Color::new(0.28, 0.9, 0.82, 0.9 * alpha),
    );
    y += 16.0;
    text(font, "znak  ·  teď  ·  rozdíl", x, y + 11.0, 10, dim);
    y += 14.0;

    let green = Color::new(0.35, 0.9, 0.5, alpha);
    let red = Color::new(0.95, 0.38, 0.4, alpha);
    let neutral = ink;
    let rows: [(&str, f32, f32, bool); 11] = [
        ("Články", s.nodes as f32, s.root_nodes as f32, false),
        ("Hmota", s.mass, s.root_mass, false),
        ("Tělo", s.body, s.root_body, false),
        ("Hue", s.hue, s.root_hue, true),
        ("Trávení", s.hetero, s.root_hetero, false),
        ("Setpoint", s.setpoint, s.root_setpoint, false),
        ("Plasticita", s.learn, s.root_learn, false),
        ("Pud hladu", s.hunger_w, s.root_hunger_w, false),
        ("Pud bolesti", s.pain_w, s.root_pain_w, false),
        ("Pud novinky", s.novelty_w, s.root_novelty_w, false),
        ("Pud potomka", s.birth_w, s.root_birth_w, false),
    ];
    for (label, now, root, circular) in rows {
        let d = if circular {
            let mut raw = now - root;
            if raw > 0.5 {
                raw -= 1.0;
            } else if raw < -0.5 {
                raw += 1.0;
            }
            raw
        } else {
            now - root
        };
        let (value, col) = if label == "Články" {
            let di = (now - root).round() as i32;
            let (sign, c) = if di > 0 {
                (format!("+{di}"), green)
            } else if di < 0 {
                (format!("{di}"), red)
            } else {
                ("±0".to_string(), neutral)
            };
            (format!("{:.0}  ({sign})", now), c)
        } else {
            let (sign, c) = if d.abs() < 1e-4 {
                ("±0".to_string(), neutral)
            } else if d > 0.0 {
                (format!("+{d:.2}"), green)
            } else {
                (format!("{d:.2}"), red)
            };
            (format!("{now:.2}  ({sign})"), c)
        };
        info_row_delta(font, x, &mut y, w, label, &value, dim, col, alpha);
    }

    let mat_d = s.maturity - s.root_maturity;
    let (mat_sign, mat_col) = if mat_d.abs() < 1e-4 {
        ("±0.0".to_string(), neutral)
    } else if mat_d > 0.0 {
        (format!("+{mat_d:.1}"), green)
    } else {
        (format!("{mat_d:.1}"), red)
    };
    info_row_delta(
                font,
        x,
        &mut y,
        w,
        "Zralost",
        &format!("{:.1}  ({mat_sign})", s.maturity),
        dim,
        mat_col,
        alpha,
    );
    y
}

fn meter_compact(
    font: &Option<Font>,
    x: f32,
    y: &mut f32,
    w: f32,
    label: &str,
    value: f32,
    value_txt: &str,
    color: Color,
    alpha: f32,
    centered: bool,
) {
    text(
        font,
        label,
        x,
        *y + 11.0,
        12,
        Color::new(0.62, 0.76, 0.8, 0.95 * alpha),
    );
    let vw = value_txt.chars().count() as f32 * 6.6;
    text(
        font,
        value_txt,
        x + w - vw,
        *y + 11.0,
        12,
        Color::new(0.9, 0.95, 0.97, alpha),
    );
    let by = *y + 15.0;
    let bh = 5.0;
    draw_rectangle(x, by, w, bh, Color::new(0.1, 0.18, 0.22, 0.55 * alpha));
    if centered {
        let mid = x + w * 0.5;
        let span = w * 0.5 * value.clamp(-1.0, 1.0);
        draw_rectangle(mid - 0.5, by, 1.0, bh, Color::new(0.5, 0.65, 0.7, 0.4 * alpha));
        if span >= 0.0 {
            draw_rectangle(mid, by, span, bh, color);
        } else {
            draw_rectangle(mid + span, by, -span, bh, color);
        }
    } else {
        draw_rectangle(x, by, w * value.clamp(0.0, 1.0), bh, color);
    }
    *y += 24.0;
}

fn draw_model_compact(font: &Option<Font>, x: f32, y: f32, w: f32, s: &Stats, alpha: f32) -> f32 {
    let dim = Color::new(0.55, 0.72, 0.78, 0.9 * alpha);
    let n = (s.nodes as usize).clamp(1, 6);
    let cy = y + 28.0;
    let span = (w * 0.72).min(200.0);
    let left = x + (w - span) * 0.5;
    let xs: Vec<f32> = (0..n)
        .map(|i| {
            if n == 1 {
                left + span * 0.5
            } else {
                left + span * (i as f32 / (n - 1) as f32)
            }
        })
        .collect();
    let (r, g, b) = hsv(s.hue, 0.8, 1.0);
    let fill = Color::new(r * 0.12, g * 0.16, b * 0.22, alpha);
    let edge = Color::new(r, g, b, alpha);
    for i in 0..n.saturating_sub(1) {
        draw_line(
            xs[i],
            cy,
            xs[i + 1],
            cy,
            3.0,
            Color::new(r, g, b, 0.75 * alpha),
        );
    }
    for i in 0..n {
        let rad = if i == 0 {
            10.0
        } else if i + 1 == n {
            6.5
        } else {
            8.0
        };
        draw_circle(xs[i], cy, rad, fill);
        draw_circle_lines(xs[i], cy, rad + 1.6, 1.6, edge);
    }
    let mid = xs[n / 2];
    draw_circle(mid, cy, 2.8, Color::new(0.62, 0.42, 1.0, alpha));
    let label = Color::new(0.55, 0.82, 0.86, 0.95 * alpha);
    text(font, "ústa", xs[0] - 10.0, cy - 16.0, 10, label);
    if n > 1 {
        text(font, "ocas", xs[n - 1] - 10.0, cy - 16.0, 10, label);
    }
    text(font, "jádro", mid - 12.0, cy + 18.0, 10, label);
    let line = cy + 30.0;
            text(
                font,
        &format!("{} článků · ústa → svaly → ocas · jádro uprostřed", n),
        x,
        line,
        11,
                dim,
            );
    line + 16.0
}

fn draw_flash(frame: &Frame, cam: &Cam, flash: &Flash) {
    let (x, y) = world_to_screen(frame, cam, flash.pos);
    if x < -40.0 || x > frame.sw + 40.0 || y < -40.0 || y > frame.sh + 40.0 {
        return;
    }
    let t = (flash.life / flash.max_life).clamp(0.0, 1.0);
    let unit = frame.sh * 0.014 * cam.zoom;
    let rad = unit * (0.6 + 1.4 * (1.0 - t));
    // Mild warm flash — soft core + thin expanding ring.
    draw_circle(x, y, rad * 1.8, Color::new(1.0, 0.85, 0.55, 0.12 * t));
    draw_circle(x, y, rad * 0.55, Color::new(1.0, 0.95, 0.8, 0.35 * t));
    draw_circle_lines(x, y, rad * 1.15, 1.4, Color::new(1.0, 0.7, 0.35, 0.55 * t));
}

fn hue_label(hue: f32) -> (&'static str, &'static str) {
    let h = hue.rem_euclid(1.0);
    if h < 0.08 || h >= 0.92 {
        ("červená", "Teplá barva. V rodině často značí agresivnější nebo hladovější větve.")
    } else if h < 0.16 {
        ("oranžová", "Teplý odstín mezi červení a žlutí — dobře viditelný mezi příbuznými.")
    } else if h < 0.28 {
        ("žlutá / limetka", "Světlý teplý tón. Snadno se pozná v kin senzorech.")
    } else if h < 0.42 {
        ("zelená", "Studenější přírodní tón. Častý u klidnějších linií.")
    } else if h < 0.55 {
        ("tyrkysová", "Modrozelená — typická „vodní“ barva misky.")
    } else if h < 0.68 {
        ("modrá", "Studená barva. Silně se liší od teplých příbuzných.")
    } else if h < 0.80 {
        ("fialová", "Mezi modrou a růžovou — výrazný znak linie.")
    } else {
        ("růžová / magentová", "Teplá fialová. Blízko červené na kruhu barev.")
    }
}

fn info_row(
    font: &Option<Font>,
    x: f32,
    y: &mut f32,
    w: f32,
    label: &str,
    value: &str,
    ink: Color,
    dim: Color,
    alpha: f32,
) {
    info_row_delta(font, x, y, w, label, value, dim, ink, alpha);
}

fn info_row_delta(
    font: &Option<Font>,
    x: f32,
    y: &mut f32,
    w: f32,
    label: &str,
    value: &str,
    dim: Color,
    value_color: Color,
    alpha: f32,
) {
    text(font, label, x, *y + 12.0, 13, dim);
    let vw = value.chars().count() as f32 * 7.2;
    text(font, value, x + w - vw, *y + 12.0, 13, value_color);
    *y += 20.0;
    draw_rectangle(
        x,
        *y - 4.0,
        w,
        1.0,
        Color::new(0.2, 0.35, 0.42, 0.18 * alpha),
    );
}

fn draw_inspect_left(
    frame: &Frame,
    font: &Option<Font>,
    net: &Net,
    t: f32,
    time: f32,
    mouse: (f32, f32),
) -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
    let alpha = smoother(t).clamp(0.0, 1.0);
    let w = (frame.sw * 0.36).clamp(300.0, 460.0);
    let x = -w * (1.0 - alpha);
    let y = 0.0;
    let h = frame.sh;
    draw_rectangle(x, y, w, h, Color::new(0.012, 0.028, 0.04, 0.94 * alpha));
    draw_rectangle(
        x + w - 1.0,
        y,
        1.0,
        h,
        Color::new(0.28, 0.9, 0.82, 0.45 * alpha),
    );
    // Soft vignette wash
    draw_rectangle(x, y, w, 70.0, Color::new(0.05, 0.14, 0.16, 0.35 * alpha));

    let ink = Color::new(0.92, 0.96, 0.98, alpha);
    let dim = Color::new(0.55, 0.72, 0.78, 0.92 * alpha);
    text(font, "Neuronová síť", x + 20.0, 28.0, 18, ink);
            text(
                font,
        &format!("{} neuronů · {} synapsí", net.kind.len(), net.wires.len()),
        x + 20.0,
        48.0,
                12,
                dim,
            );

    let btn_w = 78.0;
    let btn_h = 28.0;
    let detail_btn = (x + w - btn_w - 16.0, 16.0, btn_w, btn_h);
    let hot = hit_rect(mouse, detail_btn);
    draw_rectangle(
        detail_btn.0,
        detail_btn.1,
        detail_btn.2,
        detail_btn.3,
        if hot {
            Color::new(0.1, 0.32, 0.34, 0.98 * alpha)
        } else {
            Color::new(0.06, 0.18, 0.2, 0.92 * alpha)
        },
    );
    draw_rectangle_lines(
        detail_btn.0,
        detail_btn.1,
        detail_btn.2,
        detail_btn.3,
        1.2,
        Color::new(0.28, 0.9, 0.82, if hot { 0.95 } else { 0.55 } * alpha),
    );
    center_text(
                font,
        "Detail",
        detail_btn.0 + detail_btn.2 * 0.5,
        detail_btn.1 + detail_btn.3 * 0.55,
        13,
        Color::new(0.85, 0.98, 0.95, alpha),
    );

    let span = (h - 130.0).max(180.0);
    draw_net_live(font, x + 14.0, 72.0, w - 28.0, net, span, time, alpha);
    ((x, y, w, h), detail_btn)
}

fn visible_world_half(frame: &Frame, cam: &Cam) -> (f32, f32) {
    let s = world_scale(frame, cam).max(1e-4);
    (frame.sw * 0.5 / s, frame.sh * 0.5 / s)
}

fn clamp_cam(cam: &mut Cam, frame: &Frame, half_x: f32, half_y: f32) {
    cam.zoom = cam.zoom.clamp(1.0, 24.0);
    let (vhx, vhy) = visible_world_half(frame, cam);
    let max_cx = (half_x - vhx).max(0.0);
    let max_cy = (half_y - vhy).max(0.0);
    cam.center.x = cam.center.x.clamp(-max_cx, max_cx);
    cam.center.y = cam.center.y.clamp(-max_cy, max_cy);
}

fn reset_cam_to_dish(home: &mut Cam, cam: &mut Cam, _frame: &Frame, _half_x: f32, _half_y: f32) {
    *home = Cam {
        center: Vec2::ZERO,
        zoom: 1.0,
    };
    *cam = *home;
}

fn clear_cam_coast(pan_coast: &mut Vec2, zoom_coast: &mut f32) {
    *pan_coast = Vec2::ZERO;
    *zoom_coast = 0.0;
}

#[allow(dead_code)]
fn fit_table_zoom(_frame: &Frame, _half_x: f32, _half_y: f32) -> f32 {
    1.0
}

#[allow(dead_code)]
fn cam_for_table(_frame: &Frame, _table_hx: f32, _table_hy: f32) -> Cam {
    Cam {
        center: Vec2::ZERO,
        zoom: 1.0,
    }
}

fn pick_dish_id(world: &World, frame: &Frame, cam: &Cam, mouse: (f32, f32)) -> Option<u32> {
    let p = screen_to_world(frame, cam, mouse.0, mouse.1)?;
    for dish in world.dishes().iter().rev() {
        let local = dish.to_local(p);
        if dish.contains_local(local, -0.02) {
            return Some(dish.id);
        }
    }
    None
}

fn damp_vec(current: Vec2, target: Vec2, dt: f32, tau: f32) -> Vec2 {
    Vec2::new(
        damp(current.x, target.x, dt, tau),
        damp(current.y, target.y, dt, tau),
    )
}

fn screen_to_world(frame: &Frame, cam: &Cam, sx: f32, sy: f32) -> Option<Vec2> {
    if !(0.0..=frame.sw).contains(&sx) || !(0.0..=frame.sh).contains(&sy) {
        return None;
    }
    let scale = world_scale(frame, cam).max(1e-4);
    let (ox, oy) = view_origin(frame);
    let x = cam.center.x + (sx - ox) / scale;
    let y = cam.center.y + (oy - sy) / scale;
    Some(Vec2::new(x, y))
}

fn world_to_screen(frame: &Frame, cam: &Cam, p: Vec2) -> (f32, f32) {
    let scale = world_scale(frame, cam);
    let (ox, oy) = view_origin(frame);
    let x = ox + (p.x - cam.center.x) * scale;
    let y = oy - (p.y - cam.center.y) * scale;
    (x, y)
}

const SPEED_PRESETS: [(u32, &str); 9] = [
    (1, "1×"),
    (2, "2×"),
    (4, "4×"),
    (6, "6×"),
    (8, "8×"),
    (10, "10×"),
    (20, "20×"),
    (50, "50×"),
    (100, "100×"),
];


struct TitleUi {
    new_sim: (f32, f32, f32, f32),
    load_sim: (f32, f32, f32, f32),
    settings: (f32, f32, f32, f32),
    quit: (f32, f32, f32, f32),
    settings_panel: (f32, f32, f32, f32),
    fullscreen: (f32, f32, f32, f32),
    bloom: (f32, f32, f32, f32),
    master_minus: (f32, f32, f32, f32),
    master_value: (f32, f32, f32, f32),
    master_plus: (f32, f32, f32, f32),
    music_minus: (f32, f32, f32, f32),
    music_value: (f32, f32, f32, f32),
    music_plus: (f32, f32, f32, f32),
    sfx_minus: (f32, f32, f32, f32),
    sfx_value: (f32, f32, f32, f32),
    sfx_plus: (f32, f32, f32, f32),
}

fn title_layout(frame: &Frame) -> TitleUi {
    let btn_w = 150.0_f32;
    let btn_h = 48.0;
    let gap = 28.0;
    let cx = frame.sw * 0.5;
    let y0 = frame.sh * 0.54;
    let total = btn_w * 4.0 + gap * 3.0;
    let x0 = cx - total * 0.5;
    let panel_w = 320.0;
    let panel_h = 220.0;
    let px = (frame.sw - panel_w) * 0.5;
    let py = (y0 + btn_h + 28.0).min(frame.sh - panel_h - 16.0);
    let ax = px + 16.0;
    let aw = panel_w - 32.0;
    let mut yy = py + 16.0;
    let fullscreen = (ax, yy, aw, 28.0);
    yy += 34.0;
    let bloom = (ax, yy, aw, 28.0);
    yy += 40.0;
    let [mm, mv, mp] = stepper_row(ax, yy, aw, 26.0);
    yy += 40.0;
    let [um, uv, up] = stepper_row(ax, yy, aw, 26.0);
    yy += 40.0;
    let [sm, sv, sp] = stepper_row(ax, yy, aw, 26.0);
    TitleUi {
        new_sim: (x0, y0, btn_w, btn_h),
        load_sim: (x0 + (btn_w + gap), y0, btn_w, btn_h),
        settings: (x0 + (btn_w + gap) * 2.0, y0, btn_w, btn_h),
        quit: (x0 + (btn_w + gap) * 3.0, y0, btn_w, btn_h),
        settings_panel: (px, py, panel_w, panel_h),
        fullscreen,
        bloom,
        master_minus: mm,
        master_value: mv,
        master_plus: mp,
        music_minus: um,
        music_value: uv,
        music_plus: up,
        sfx_minus: sm,
        sfx_value: sv,
        sfx_plus: sp,
    }
}

fn reset_run_ui(
    paused: &mut bool,
    pinned: &mut Option<u64>,
    feed_tool: &mut bool,
    dish_tool: &mut bool,
    feed_kind: &mut FoodKind,
    selected_feeder: &mut Option<usize>,
    settings_open: &mut bool,
    settings_section: &mut u8,
    food_panel_open: &mut bool,
    shown: &mut Option<Stats>,
    panel_t: &mut f32,
    detail_hit: &mut Option<(f32, f32, f32, f32)>,
    net_hit: &mut Option<(f32, f32, f32, f32)>,
    motes: &mut Vec<Mote>,
    fades: &mut Vec<Fade>,
    home: &mut Cam,
    cam: &mut Cam,
    following: &mut bool,
    follow_id: &mut Option<u64>,
    inspect_zoom: &mut f32,
    pan_coast: &mut Vec2,
    zoom_coast: &mut f32,
    frame: &Frame,
    half_x: f32,
    half_y: f32,
) {
    *paused = false;
    *pinned = None;
    *feed_tool = false;
    *dish_tool = false;
    *feed_kind = FoodKind::Green;
    *selected_feeder = None;
    *settings_open = false;
    *settings_section = 0;
    *food_panel_open = false;
    *shown = None;
    *panel_t = 0.0;
    *detail_hit = None;
    *net_hit = None;
    motes.clear();
    fades.clear();
    *following = false;
    *follow_id = None;
    *inspect_zoom = 3.0;
    clear_cam_coast(pan_coast, zoom_coast);
    reset_cam_to_dish(home, cam, frame, half_x, half_y);
}

struct SavesPanel {
    panel: (f32, f32, f32, f32),
    close: (f32, f32, f32, f32),
    name_box: (f32, f32, f32, f32),
    rows: Vec<(f32, f32, f32, f32)>,
    save: (f32, f32, f32, f32),
    load: (f32, f32, f32, f32),
    delete: (f32, f32, f32, f32),
}

fn saves_panel_layout(frame: &Frame, row_count: usize, allow_save: bool) -> SavesPanel {
    let w = 360.0_f32.min(frame.sw - 40.0);
    let row_h = 28.0;
    let list_h = (row_count.max(1).min(8) as f32) * (row_h + 4.0) + 8.0;
    let name_h = if allow_save { 36.0 } else { 0.0 };
    let btn_h = 36.0;
    let h = 48.0
        + if allow_save { name_h + 10.0 } else { 0.0 }
        + list_h
        + 12.0
        + btn_h
        + 16.0;
    let x = (frame.sw - w) * 0.5;
    let y = ((frame.sh - h) * 0.5).max(20.0);
    let panel = (x, y, w, h);
    let close = (x + w - 36.0, y + 10.0, 26.0, 26.0);
    let mut cy = y + 46.0;
    let name_box = if allow_save {
        let r = (x + 16.0, cy, w - 32.0, name_h);
        cy += name_h + 10.0;
        r
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    let mut rows = Vec::new();
    let list_top = cy;
    for i in 0..row_count.min(8) {
        rows.push((x + 16.0, list_top + i as f32 * (row_h + 4.0), w - 32.0, row_h));
    }
    cy = list_top + list_h + 8.0;
    let btn_w = if allow_save {
        (w - 32.0 - 12.0) / 3.0
    } else {
        (w - 32.0 - 6.0) / 2.0
    };
    let save = if allow_save {
        (x + 16.0, cy, btn_w, btn_h)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    let load_x = if allow_save {
        x + 16.0 + btn_w + 6.0
    } else {
        x + 16.0
    };
    let load = (load_x, cy, btn_w, btn_h);
    let delete = (load_x + btn_w + 6.0, cy, btn_w, btn_h);
    SavesPanel {
        panel,
        close,
        name_box,
        rows,
        save,
        load,
        delete,
    }
}

fn draw_saves_panel(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    ui: &SavesPanel,
    list: &[SaveMeta],
    selected: Option<i64>,
    name: &str,
    name_focus: bool,
    allow_save: bool,
    status: Option<&str>,
) {
    let _ = frame;
    let (x, y, w, h) = ui.panel;
    draw_rectangle(x, y, w, h, Color::new(0.02, 0.05, 0.07, 0.96));
    draw_rectangle(x, y, w, 2.0, Color::new(0.28, 0.9, 0.82, 0.55));
    let ink = Color::new(0.9, 0.97, 0.98, 1.0);
    let dim = Color::new(0.55, 0.72, 0.78, 0.9);
    center_text(
        font,
        if allow_save {
            "Uložení"
        } else {
            "Načíst simulaci"
        },
        x + w * 0.5,
        y + 28.0,
        16,
        ink,
    );

    let close_hot = hit_rect(mouse, ui.close);
    center_text(
        font,
        "×",
        ui.close.0 + ui.close.2 * 0.5,
        ui.close.1 + ui.close.3 * 0.5 + 6.0,
        18,
        if close_hot {
            Color::new(1.0, 0.7, 0.65, 1.0)
        } else {
            dim
        },
    );

    if allow_save && ui.name_box.2 > 0.0 {
        let (nx, ny, nw, nh) = ui.name_box;
        let hot = hit_rect(mouse, ui.name_box) || name_focus;
        draw_rectangle(
            nx,
            ny,
            nw,
            nh,
            Color::new(0.04, 0.08, 0.1, if hot { 0.95 } else { 0.8 }),
        );
        draw_rectangle(
            nx,
            ny,
            nw,
            1.0,
            Color::new(0.28, 0.9, 0.82, if name_focus { 0.7 } else { 0.3 }),
        );
        let shown = if name.is_empty() {
            "název uložení…"
        } else {
            name
        };
    text(
        font,
            shown,
            nx + 10.0,
            ny + nh * 0.5 + 5.0,
            14,
            if name.is_empty() {
                Color::new(0.4, 0.55, 0.58, 0.7)
            } else {
                ink
            },
        );
    }

    if list.is_empty() {
        center_text(
            font,
            "Zatím žádná uložení",
            x + w * 0.5,
            ui.load.1 - 28.0,
            13,
            dim,
        );
    } else {
        for (i, meta) in list.iter().take(ui.rows.len()).enumerate() {
            let row = ui.rows[i];
            let sel = selected == Some(meta.id);
            let hot = hit_rect(mouse, row);
            draw_rectangle(
                row.0,
                row.1,
                row.2,
                row.3,
                if sel {
                    Color::new(0.08, 0.28, 0.26, 0.95)
                } else if hot {
                    Color::new(0.05, 0.14, 0.16, 0.9)
                } else {
                    Color::new(0.04, 0.08, 0.1, 0.75)
                },
            );
    text(
                font,
                &meta.name,
                row.0 + 8.0,
                row.1 + 12.0,
                13,
                ink,
            );
            text(
                font,
                &format!(
                    "{:.0}s · {} org · gen {}",
                    meta.sim_time, meta.population, meta.max_generation
                ),
                row.0 + 8.0,
                row.1 + 24.0,
                11,
                dim,
            );
        }
    }

    let draw_btn = |rect: (f32, f32, f32, f32), label: &str, accent: bool| {
        if rect.2 <= 0.0 {
            return;
        }
        let hot = hit_rect(mouse, rect);
        let bg = if accent {
            if hot {
                Color::new(0.12, 0.42, 0.38, 0.95)
            } else {
                Color::new(0.06, 0.22, 0.2, 0.95)
            }
        } else if hot {
            Color::new(0.12, 0.16, 0.18, 0.95)
        } else {
            Color::new(0.05, 0.09, 0.11, 0.9)
        };
        draw_rectangle(rect.0, rect.1, rect.2, rect.3, bg);
        draw_rectangle(
            rect.0,
            rect.1,
            rect.2,
            1.0,
            Color::new(0.28, 0.9, 0.82, if accent { 0.55 } else { 0.25 }),
        );
        center_text(
        font,
        label,
            rect.0 + rect.2 * 0.5,
            rect.1 + rect.3 * 0.5 + 5.0,
            13,
            ink,
        );
    };
    if allow_save {
        draw_btn(ui.save, "Uložit", true);
    }
    draw_btn(ui.load, "Načíst", !allow_save);
    draw_btn(ui.delete, "Smazat", false);

    if let Some(s) = status {
        center_text(
            font,
            s,
            x + w * 0.5,
            y + h - 8.0,
            12,
            Color::new(0.85, 0.95, 0.7, 0.95),
        );
    }
}

fn draw_title(
    gfx: Option<&Gfx>,
    frame: &Frame,
    font: &Option<Font>,
    logo_font: &Option<Font>,
    mouse: (f32, f32),
    ui: &TitleUi,
    time: f32,
    can_load: bool,
    msg: Option<&str>,
    hover_new: f32,
    hover_load: f32,
    hover_settings: f32,
    hover_quit: f32,
    settings_open: bool,
    fullscreen: bool,
    bloom_on: bool,
    mix: Mix,
    scale: f32,
    alpha: f32,
    blur: f32,
    compose: f32,
    burst: f32,
) {
    draw_title_once(
        gfx,
        frame,
        font,
        logo_font,
        mouse,
        ui,
        time,
        can_load,
        msg,
        hover_new,
        hover_load,
        hover_settings,
        hover_quit,
        settings_open,
        fullscreen,
        bloom_on,
        mix,
        scale,
        alpha,
        blur,
        compose,
        burst,
    );
}

fn draw_title_once(
    gfx: Option<&Gfx>,
    frame: &Frame,
    font: &Option<Font>,
    logo_font: &Option<Font>,
    mouse: (f32, f32),
    ui: &TitleUi,
    time: f32,
    can_load: bool,
    msg: Option<&str>,
    hover_new: f32,
    hover_load: f32,
    hover_settings: f32,
    hover_quit: f32,
    settings_open: bool,
    fullscreen: bool,
    bloom_on: bool,
    mix: Mix,
    scale: f32,
    alpha: f32,
    blur: f32,
    compose: f32,
    burst: f32,
) {
    let logo_cx = frame.sw * 0.5;
    let logo_cy = frame.sh * 0.28;
    let mark = if logo_font.is_some() { logo_font } else { font };
    draw_neon_logo(
        gfx,
        mark,
        frame,
        logo_cx,
        logo_cy,
        mouse,
        time,
        scale,
        alpha,
        blur,
        compose,
        burst,
    );

    let hide = burst.max(compose);
    let menu_a = alpha * (1.0 - hide).clamp(0.0, 1.0);
    if menu_a < 0.04 && hide > 0.5 {
        return;
    }

    let actions = [
        (ui.new_sim, "NOVÁ HRA", true, hover_new, 26u16),
        (ui.load_sim, "NAČÍST", can_load, hover_load, 26u16),
        (ui.settings, "NASTAVENÍ", true, hover_settings, 24u16),
        (ui.quit, "UKONČIT", true, hover_quit, 24u16),
    ];
    for (rect, label, en, hover, size) in actions {
        draw_text_action(
            font,
            scale_rect(rect, logo_cx, logo_cy, scale),
            label,
            mouse,
            en,
            hover,
            menu_a,
            size,
        );
    }

    if settings_open && menu_a > 0.05 {
        let (px, py, pw, ph) = ui.settings_panel;
        draw_rectangle(px, py, pw, ph, Color::new(0.02, 0.05, 0.07, 0.94 * menu_a));
        draw_rectangle(px, py, pw, 1.0, Color::new(0.28, 0.9, 0.82, 0.5 * alpha));
        let dim = Color::new(0.55, 0.72, 0.78, 0.9 * alpha);
        draw_chip(
            font,
            ui.fullscreen,
            if fullscreen {
                "Celá obrazovka zapnutá"
        } else {
                "Celá obrazovka"
            },
            fullscreen,
            mouse,
        );
        draw_chip(
            font,
            ui.bloom,
            if bloom_on {
                "Bloom / shadery zapnuté"
            } else {
                "Bloom / shadery"
            },
            bloom_on,
            mouse,
        );
        text(font, "hlasitost", ui.master_minus.0, ui.master_minus.1 - 2.0, 11, dim);
        draw_stepper(
            font,
            ui.master_minus,
            ui.master_value,
            ui.master_plus,
            &format!("{:.0}%", mix.master * 100.0),
            mouse,
        );
        text(font, "hudba", ui.music_minus.0, ui.music_minus.1 - 2.0, 11, dim);
        draw_stepper(
            font,
            ui.music_minus,
            ui.music_value,
            ui.music_plus,
            &format!("{:.0}%", mix.music * 100.0),
            mouse,
        );
        text(font, "efekty", ui.sfx_minus.0, ui.sfx_minus.1 - 2.0, 11, dim);
        draw_stepper(
            font,
            ui.sfx_minus,
            ui.sfx_value,
            ui.sfx_plus,
            &format!("{:.0}%", mix.sfx * 100.0),
            mouse,
        );
    }

    if let Some(m) = msg {
        center_text(
            font,
            m,
            frame.sw * 0.5,
            ui.quit.1 + ui.quit.3 + 28.0,
            14,
            Color::new(1.0, 0.55, 0.45, 0.95 * alpha),
        );
    }
}

fn paint_title_ambience(
    gfx: Option<&Gfx>,
    frame: &Frame,
    cursor: (f32, f32),
    cursor_vel: (f32, f32),
    roamers: &[TitleRoamer],
    time: f32,
    fly_scale: f32,
    fly_alpha: f32,
    fly_blur: f32,
) {
    let _ = (cursor, cursor_vel, fly_scale, fly_blur);
    let a0 = fly_alpha.clamp(0.0, 1.0);
    if let Some(g) = gfx {
        g.begin_glow();
        g.soft_blob_cont(
            frame.sw * 0.22,
            frame.sh * 0.28,
            100.0,
            Color::new(0.12, 0.55, 0.7, 0.03 * a0),
            2.2,
            -1.0,
            0.0,
        );
        g.soft_blob_cont(
            frame.sw * 0.78,
            frame.sh * 0.55,
            120.0,
            Color::new(0.15, 0.7, 0.65, 0.028 * a0),
            2.4,
            -1.0,
            0.0,
        );
        // Soft auras + per-node glow for roamers (still one material bind).
        for roamer in roamers {
            let (cr, cg, cb) = hsv(roamer.hue, 0.7, 0.85);
            let nodes = roamer.nodes.max(2) as usize;
            g.soft_blob_cont(
                roamer.pos.0,
                roamer.pos.1,
                36.0 + nodes as f32 * 3.0,
                Color::new(cr, cg, cb, 0.07 * a0),
                2.2,
                -1.0,
                0.0,
            );
            let len = roamer.vel.0.hypot(roamer.vel.1).max(1e-3);
            let ax = roamer.vel.0 / len;
            let ay = roamer.vel.1 / len;
            let px = -ay;
            let py = ax;
            let spacing = 10.0 + 0.6 * nodes as f32;
            for i in 0..nodes {
                // Head (i=0) leads in velocity direction.
                let t = 0.5 - i as f32 / (nodes - 1) as f32;
                let und = (time * 2.6 + roamer.phase + i as f32 * 0.9).sin() * 3.2;
                let along = t * spacing * (nodes - 1) as f32;
                let x = roamer.pos.0 + ax * along + px * und * (1.0 - t.abs() * 0.35);
                let y = roamer.pos.1 + ay * along + py * und * (1.0 - t.abs() * 0.35);
                // Head (i=0) is the largest segment.
                let fall = 1.0 - (i as f32 / (nodes - 1) as f32) * 0.62;
                g.soft_blob_cont(
                    x,
                    y,
                    16.0 * fall,
                    Color::new(cr, cg, cb, 0.1 * a0 * fall),
                    1.7,
                    -1.0,
                    if i == 0 { 0.35 } else { 0.0 },
                );
            }
        }
        g.end_glow();
    }
    for roamer in roamers {
        draw_ambience_silhouette(
            None,
            roamer.pos,
            roamer.vel,
            roamer.hue,
            roamer.phase,
            time,
            0.55 * a0,
            roamer.nodes as usize,
        );
    }
}

/// Fixed lobby cursor-attraction radius (max of the old scroll range).
const LOBBY_ATTRACT_R: f32 = 360.0;

#[derive(Clone, Copy)]
struct TitleRoamer {
    pos: (f32, f32),
    vel: (f32, f32),
    hue: f32,
    phase: f32,
    turn: f32,
    /// Body segments (head = index 0, largest).
    nodes: u8,
}

impl TitleRoamer {
    fn spawn(sw: f32, sh: f32) -> Vec<Self> {
        // Scatter across the full title field — free to pass behind logo / menu.
        // (px, py, vx, vy, hue, phase, turn, nodes)
        let specs = [
            (0.12, 0.22, 16.0, 10.0, 0.42, 0.2, 0.4, 3u8),
            (0.88, 0.70, -14.0, -8.0, 0.55, 1.1, 1.0, 6),
            (0.28, 0.78, 10.0, -12.0, 0.36, 2.4, 1.8, 4),
            (0.72, 0.18, -12.0, 14.0, 0.62, 0.8, 2.5, 7),
            (0.48, 0.52, 8.0, -10.0, 0.48, 3.0, 0.6, 5),
        ];
        specs
            .into_iter()
            .map(|(px, py, vx, vy, hue, phase, turn, nodes)| Self {
                pos: (sw * px, sh * py),
                vel: (vx, vy),
                hue,
                phase,
                turn,
                nodes,
            })
            .collect()
    }

    fn step(
        &mut self,
        sw: f32,
        sh: f32,
        cursor: (f32, f32),
        cursor_vel: (f32, f32),
        attract_r: f32,
        dt: f32,
    ) {
        let attract_r = attract_r.max(40.0);
        let dx = cursor.0 - self.pos.0;
        let dy = cursor.1 - self.pos.1;
        let dist = dx.hypot(dy);

        if dist < attract_r {
            let influence = (1.0 - dist / attract_r).powf(1.2);
            let cv_len = cursor_vel.0.hypot(cursor_vel.1);
            if cv_len > 18.0 {
                self.vel.0 += (cursor_vel.0 / cv_len) * 220.0 * influence * dt;
                self.vel.1 += (cursor_vel.1 / cv_len) * 220.0 * influence * dt;
            } else if dist > 1.0 {
                self.vel.0 += (dx / dist) * 90.0 * influence * dt;
                self.vel.1 += (dy / dist) * 90.0 * influence * dt;
            }
        } else {
            self.turn += dt * (0.55 + 0.35 * (self.phase + self.pos.0 * 0.01).sin());
            let steer = self.turn.sin() * 1.2 + (self.phase * 0.7 + self.turn * 0.3).cos() * 0.8;
            let ang = self.vel.1.atan2(self.vel.0) + steer * dt * 1.4;
            let target_spd = 34.0 + 12.0 * (self.phase + self.turn).sin().abs();
            self.vel.0 += (ang.cos() * target_spd - self.vel.0) * (1.6 * dt);
            self.vel.1 += (ang.sin() * target_spd - self.vel.1) * (1.6 * dt);
        }

        self.vel.0 *= 1.0 - 1.1 * dt;
        self.vel.1 *= 1.0 - 1.1 * dt;
        let spd = self.vel.0.hypot(self.vel.1);
        if spd > 240.0 {
            self.vel.0 *= 240.0 / spd;
            self.vel.1 *= 240.0 / spd;
        }
        if spd < 12.0 {
            let ang = self.phase + self.turn;
            self.vel.0 += ang.cos() * 40.0 * dt;
            self.vel.1 += ang.sin() * 40.0 * dt;
        }

        self.pos.0 += self.vel.0 * dt;
        self.pos.1 += self.vel.1 * dt;
        self.phase += dt * 1.7;

        // Soft bounce on screen edges only — roam the whole frame.
        let m = 28.0;
        if self.pos.0 < m {
            self.pos.0 = m;
            self.vel.0 = self.vel.0.abs();
        } else if self.pos.0 > sw - m {
            self.pos.0 = sw - m;
            self.vel.0 = -self.vel.0.abs();
        }
        if self.pos.1 < m {
            self.pos.1 = m;
            self.vel.1 = self.vel.1.abs();
        } else if self.pos.1 > sh - m {
            self.pos.1 = sh - m;
            self.vel.1 = -self.vel.1.abs();
        }
    }
}

fn draw_ambience_silhouette(
    gfx: Option<&Gfx>,
    pos: (f32, f32),
    dir: (f32, f32),
    hue: f32,
    phase: f32,
    time: f32,
    alpha: f32,
    nodes: usize,
) {
    let len = dir.0.hypot(dir.1).max(1e-3);
    let ax = dir.0 / len;
    let ay = dir.1 / len;
    let px = -ay;
    let py = ax;
    let (cr, cg, cb) = hsv(hue, 0.75, 0.9);
    let nodes = nodes.clamp(2, 8);
    let spacing = 9.5 + 0.55 * nodes as f32;
    let base_r = 7.2 + 0.35 * nodes as f32;
    let a = alpha.clamp(0.0, 1.0);

    let mut pts = [(0.0_f32, 0.0_f32); 8];
    for i in 0..nodes {
        // Head leads: i=0 sits ahead along velocity, tail trails behind.
        let t = 0.5 - i as f32 / (nodes - 1) as f32;
        let und = (time * 2.6 + phase + i as f32 * 0.9).sin() * 3.2;
        let along = t * spacing * (nodes - 1) as f32;
        pts[i] = (
            pos.0 + ax * along + px * und * (1.0 - t.abs() * 0.35),
            pos.1 + ay * along + py * und * (1.0 - t.abs() * 0.35),
        );
    }

    // Optional single aura when a glow batch isn't already open.
    if let Some(g) = gfx {
        g.soft_blob(
            pos.0,
            pos.1,
            28.0 + nodes as f32 * 3.5,
            Color::new(cr, cg, cb, 0.07 * a),
            2.2,
            -1.0,
            0.0,
        );
    }

    for i in 0..nodes.saturating_sub(1) {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[i + 1];
        draw_line(
            x0,
            y0,
            x1,
            y1,
            2.2,
            Color::new(0.02, 0.03, 0.05, 0.9 * a),
        );
        draw_line(
            x0,
            y0,
            x1,
            y1,
            1.2,
            Color::new(cr, cg, cb, 0.7 * a),
        );
    }
    for i in 0..nodes {
        // Head is the largest segment; body tapers toward the tail.
        let fall = 1.0 - (i as f32 / (nodes - 1) as f32) * 0.62;
        let rad = (base_r * fall).max(2.8);
        let (x, y) = pts[i];
        draw_circle(x, y, rad, Color::new(0.03, 0.02, 0.05, a));
        draw_circle_lines(x, y, rad, 1.8, Color::new(cr, cg, cb, (0.72 + 0.26 * fall) * a));
    }
}

fn draw_neon_logo(
    gfx: Option<&Gfx>,
    font: &Option<Font>,
    frame: &Frame,
    cx: f32,
    cy: f32,
    mouse: (f32, f32),
    time: f32,
    scale: f32,
    alpha: f32,
    blur: f32,
    compose: f32,
    burst: f32,
) {
    let compose = compose.clamp(0.0, 1.0);
    let burst = burst.clamp(0.0, 1.0);
    let blur = blur.clamp(0.0, 1.0);
    let a = alpha.clamp(0.0, 1.0);
    // During burst letters stay visible while flying toward camera.
    let letter_a = if burst > 0.01 {
        a * (1.0 - burst * 0.55).max(0.08)
    } else {
        a * (1.0 - compose).powf(0.9)
    };
    let breath = 0.5 + 0.5 * (time * 0.35).sin();

    let drift_x = (time * 0.22).sin() * 1.4 + (time * 0.11).cos() * 0.6;
    let drift_y = (time * 0.18).cos() * 1.0 + (time * 0.14).sin() * 0.45;
    let (lx, ly) = scale_about(cx + drift_x, cy + drift_y, cx, cy, scale);

    let letter_scale = 56.0 * scale;
    // Park helix only when returning to title (compose); burst keeps DNA in place then shatters.
    let dna_tx = frame.sw * 0.18;
    let dna_ty = frame.sh * 0.48;
    let dna_home_cx = lx + letter_scale * 0.02;
    let dna_home_cy = ly + letter_scale * 0.04;
    let dna_cx = dna_home_cx + (dna_tx - dna_home_cx) * compose;
    let dna_cy = dna_home_cy + (dna_ty - dna_home_cy) * compose;
    let dna_s = scale * (0.98 + 0.85 * compose) * (1.0 + burst * 1.8);
    let dna_a = a * (0.88 + 0.12 * compose) * (1.0 - burst * 0.35).max(0.12);
    let dna_park = if burst > 0.01 { 0.0 } else { compose };

    if let Some(g) = gfx {
        g.begin_glow();
        if letter_a > 0.02 {
            g.soft_blob_cont(
                lx,
                ly - 4.0,
                (280.0 + 36.0 * breath) * (1.0 + burst * 1.2),
                Color::new(0.28, 0.88, 1.0, 0.03 * letter_a),
                2.9,
                -1.0,
                0.0,
            );
        } else {
            g.soft_blob_cont(
                dna_cx,
                dna_cy,
                200.0 + 30.0 * breath,
                Color::new(0.25, 0.85, 0.95, 0.04 * a),
                2.6,
                -1.0,
                0.0,
            );
        }
        g.end_glow();
    }

    // Radial blur streaks — ghost logo passes outward from center.
    let radial = (blur * 0.85 + burst * 0.55).clamp(0.0, 1.0);
    if radial > 0.05 {
        let ghosts = 2;
        for g in 1..=ghosts {
            let t = g as f32 / ghosts as f32;
            let stretch = 1.0 + radial * (0.06 + 0.14 * t);
            let ga = letter_a * (0.14 / ghosts as f32) * (1.0 - t * 0.4);
            let ox = (g as f32 - 0.5) * radial * 3.0;
            if ga > 0.01 {
                paint_logo_wordmark(
                    None,
                    font,
                    lx + ox,
                    ly,
                    mouse,
                    letter_scale * stretch,
                    ga,
                    time,
                    burst,
                    true,
                );
            }
            if burst < 0.6 {
                draw_logo_dna(
                    gfx,
                    dna_cx,
                    dna_cy,
                    mouse,
                    time,
                    dna_s * stretch,
                    dna_a * (0.2 / ghosts as f32),
                    0.0,
                    1.05,
                    dna_park,
                    burst * (0.7 + 0.3 * t),
                );
            }
        }
    }

    draw_logo_dna(
        gfx, dna_cx, dna_cy, mouse, time, dna_s, dna_a, 0.0, 0.45, dna_park, burst,
    );
    if letter_a > 0.02 {
        paint_logo_wordmark(gfx, font, lx, ly, mouse, letter_scale, letter_a, time, burst, false);
    }
    draw_logo_dna(
        gfx, dna_cx, dna_cy, mouse, time, dna_s, dna_a * 1.04, 0.40, 1.05, dna_park, burst,
    );
}

const BASE_LOGO_FS: u16 = 72;

/// Stretched glass wordmark — organic per-layer motion; burst flies letters at camera.
fn paint_logo_wordmark(
    gfx: Option<&Gfx>,
    font: &Option<Font>,
    lx: f32,
    ly: f32,
    mouse: (f32, f32),
    letter_scale: f32,
    alpha: f32,
    time: f32,
    burst: f32,
    is_ghost: bool,
) {
    let a = alpha.clamp(0.0, 1.0);
    if a < 0.02 {
        return;
    }
    let burst = burst.clamp(0.0, 1.0);
    let burst_e = burst * burst;
    const WORD: &str = "AETHER";
    const Y_STAGGER: [f32; 6] = [1.8, -3.2, 2.4, -1.6, 3.0, -2.2];
    const SCATTER_MID: [(f32, f32); 6] = [
        (0.82, -0.41),
        (-0.55, 0.78),
        (0.18, 0.92),
        (-0.91, -0.22),
        (0.62, 0.55),
        (-0.28, -0.88),
    ];
    const SCATTER_BACK: [(f32, f32); 6] = [
        (-0.72, 0.48),
        (0.44, -0.84),
        (0.90, 0.15),
        (-0.35, 0.88),
        (-0.68, -0.58),
        (0.25, 0.91),
    ];

    // Glass: ~90% transparent body, ~34% blur spread.
    const GLASS_OPACITY: f32 = 0.10;
    const BLUR_STRENGTH: f32 = 0.34;

    let size = (letter_scale * 1.62).clamp(46.0, 220.0);
    let aspect = 1.38;
    let tracking = size * 0.06;
    let blur_r = size * BLUR_STRENGTH;

    let mx = ((mouse.0 - lx) / 300.0).clamp(-1.0, 1.0);
    let my = ((mouse.1 - ly) / 240.0).clamp(-1.0, 1.0);
    let push = (mx * mx + my * my).sqrt().clamp(0.0, 1.0) * 0.055 * (1.0 - burst);

    // Same seamless traveling hue wave as the DNA helix.
    let helix_at = |along: f32| {
        let phase = along * std::f32::consts::TAU - time * 0.41;
        let u = 0.5 + 0.5 * phase.sin();
        hsv(0.36 + u * 0.26, 0.72, 0.95)
    };

    let mut advances = [0.0_f32; 6];
    let mut total_w = 0.0;
    let base_scale = size / BASE_LOGO_FS as f32;
    for (i, ch) in WORD.chars().enumerate() {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        let w = measure_text(s, font.as_ref(), BASE_LOGO_FS, 1.0).width * base_scale * aspect;
        advances[i] = w + tracking;
        total_w += advances[i];
    }
    total_w -= tracking;

    let base_x = lx - total_w * 0.5;
    let base_y = ly + size * 0.30;

    // Per-letter float; layer 1 (front) least, layer 3 (back) most.
    let layer_anim = |layer: usize, i: usize| -> (f32, f32) {
        let li = i as f32;
        let float_x = (time * 0.62 + li * 1.37 + layer as f32 * 0.9).sin()
            + (time * 0.39 + li * 0.71).cos() * 0.55;
        let float_y = (time * 0.54 + li * 1.15 + 0.6).cos()
            + (time * 0.47 + li * 0.88).sin() * 0.6;
        let damp = 1.0 - burst * 0.85;
        match layer {
            0 => {
                let amp = 6.8 * damp;
                let ox = float_x * amp
                    + (time * 0.28 + li * 0.5).sin() * 2.4
                    + (time * 0.11).sin() * 1.6;
                let oy = float_y * amp * 0.85
                    + (time * 0.24 + li * 0.45).cos() * 2.0;
                (ox, oy)
            }
            1 => {
                let amp = 4.2 * damp;
                let ox = float_x * amp
                    + (time * 0.95 + li * 0.9).sin() * 1.8
                    + (time * 0.22 + 2.0).sin() * -1.4;
                let oy = float_y * amp * 0.9
                    + (time * 0.78 + li * 1.05).cos() * 2.2;
                (ox, oy)
            }
            _ => {
                let amp = 1.85 * damp;
                let breath = (time * 0.48).sin();
                let ox = float_x * amp + breath * 0.4;
                let oy = float_y * amp * 0.9 + breath * 0.55;
                (ox, oy)
            }
        }
    };

    let layer_style = |layer: usize| -> (f32, f32, Option<&'static [(f32, f32); 6]>) {
        match layer {
            0 => (GLASS_OPACITY * a * 0.55, 1.15, Some(&SCATTER_BACK)),
            1 => (GLASS_OPACITY * a * 0.75, 1.0, Some(&SCATTER_MID)),
            _ => (GLASS_OPACITY * a, 0.72, None),
        }
    };

    // Soft glass halo only — fades as burst takes over.
    if !is_ghost {
        if let Some(g) = gfx {
            g.begin_glow();
            g.soft_blob_cont(
                lx,
                ly,
                total_w * 0.52 + size * 0.3,
                Color::new(0.28, 0.86, 0.92, 0.035 * a * (1.0 - burst_e)),
                2.7,
                -1.0,
                0.0,
            );
            g.end_glow();
        }
    }

    let layer_range = if is_ghost { 1..2 } else { 0..3 };
    for layer in layer_range {
        let (op, blur_mul, scatter) = layer_style(layer);
        let mut x = base_x;
        for (i, ch) in WORD.chars().enumerate() {
            let mut buf = [0u8; 4];
            let glyph = ch.encode_utf8(&mut buf);
            let (ax, ay) = layer_anim(layer, i);
            let mut ox = ax;
            let mut oy = ay + Y_STAGGER[i];
            if let Some(dirs) = scatter {
                let (dx, dy) = dirs[i];
                let amp = (4.5 + i as f32 * 0.6) * push;
                ox += dx * amp;
                oy += dy * amp;
            }

            // Shatter: every layer/letter flies toward camera on its own vector.
            let mut zoom = 1.0;
            let mut la = a;
            if burst_e > 0.001 {
                let seed = (layer as u32) * 64 + i as u32 * 7 + 42;
                let (dx, dy, dz) = burst_dir(seed);
                let delay = layer as f32 * 0.08 + i as f32 * 0.02;
                let local = ((burst - delay) / (1.0 - delay).max(0.2)).clamp(0.0, 1.0);
                let local_e = local * local;
                let speed = 260.0 + burst_hash(seed.wrapping_add(3)) * 420.0;
                let fly = local_e * speed;
                ox += dx * fly;
                oy += dy * fly;
                zoom = 1.0 + dz * local_e * (2.8 + burst_hash(seed.wrapping_add(9)) * 2.2);
                la = a * (1.0 - local_e * 0.72).max(0.05);
            }

            let gx = x + ox;
            let gy = base_y + oy;
            let cw = advances[i] - tracking;
            let glyph_size = (size * zoom).clamp(20.0, 360.0);
            let glyph_scale = glyph_size / BASE_LOGO_FS as f32;

            // Diagonal sample: left→right + top→bottom (šikmo dolů).
            let x_n = ((gx + cw * 0.5 * zoom - base_x) / total_w.max(1.0)).clamp(0.0, 1.0);
            let y_n = ((gy - (base_y - size * 0.55)) / (size * 1.15)).clamp(0.0, 1.0);
            let diag = (x_n * 0.68 + y_n * 0.32).clamp(0.0, 1.0);
            let (hr, hg, hb) = helix_at(diag);
            let body = Color::new(
                0.35 + hr * 0.55,
                0.40 + hg * 0.50,
                0.42 + hb * 0.50,
                op * (la / a.max(1e-3)),
            );
            let rim = Color::new(
                0.55 + hr * 0.45,
                0.70 + hg * 0.30,
                0.75 + hb * 0.25,
                op * 0.55 * (la / a.max(1e-3)),
            );

            let blur_steps = if is_ghost {
                1
            } else if burst > 0.2 {
                2
            } else {
                5
            };
            for b in 0..blur_steps {
                let t = (b as f32 + 1.0) / (blur_steps as f32 + 1.0);
                let ang = t * std::f32::consts::TAU + i as f32 * 0.85 + layer as f32 * 0.4;
                let r = blur_r * blur_mul * zoom * (0.35 + t * 0.65) * (1.0 + burst_e * 1.4);
                let ba = body.a * (0.34 / blur_steps as f32) * (1.0 - t * 0.55);
                text_stretched(
                    font,
                    glyph,
                    gx + ang.cos() * r,
                    gy + ang.sin() * r * 0.78,
                    glyph_scale,
                    aspect,
                    Color::new(body.r, body.g, body.b, ba),
                );
            }

            if !is_ghost {
                text_stretched(
                    font,
                    glyph,
                    gx + 1.0,
                    gy + 1.1,
                    glyph_scale,
                    aspect,
                    Color::new(0.0, 0.02, 0.04, 0.035 * la),
                );
            }
            text_stretched(font, glyph, gx, gy, glyph_scale, aspect, body);
            text_stretched(font, glyph, gx - 0.55, gy - 0.65, glyph_scale, aspect, rim);

            if !is_ghost && layer == 2 {
                text_stretched(
                    font,
                    glyph,
                    gx - 0.3,
                    gy - 0.8,
                    glyph_scale,
                    aspect,
                    Color::new(hr, hg, hb, 0.10 * la),
                );
                text_stretched(
                    font,
                    glyph,
                    gx + 0.4,
                    gy - 1.2,
                    glyph_scale,
                    aspect,
                    Color::new(
                        0.70 + hr * 0.30,
                        0.82 + hg * 0.18,
                        0.88 + hb * 0.12,
                        0.07 * la,
                    ),
                );
            }

            x += advances[i];
        }
    }
}

fn text_stretched(
    font: &Option<Font>,
    s: &str,
    x: f32,
    y: f32,
    scale: f32,
    aspect: f32,
    color: Color,
) {
    match font {
        Some(f) => {
            draw_text_ex(
                s,
                x,
                y,
                TextParams {
                    font: Some(f),
                    font_size: BASE_LOGO_FS,
                    font_scale: scale,
                    font_scale_aspect: aspect,
                    color,
                    ..Default::default()
                },
            );
        }
        None => {
            draw_text(s, x, y, BASE_LOGO_FS as f32 * scale, color);
        }
    }
}


fn draw_text_action(
    font: &Option<Font>,
    rect: (f32, f32, f32, f32),
    label: &str,
    mouse: (f32, f32),
    enabled: bool,
    hover: f32,
    alpha: f32,
    size_base: u16,
) {
    let _ = mouse;
    let hover = if enabled {
        smoother(hover.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let a = alpha.clamp(0.0, 1.0);
    let (_x, _y, _w, h) = rect;
    let cx = rect.0 + rect.2 * 0.5;
    let cy = rect.1 + h * 0.55;
    let text_w = measure_label(font, label, size_base);
    let size_f = size_base as f32;

    // Compact soft black veil — just enough to lift the label off the fluid.
    if hover > 0.012 && enabled {
        for i in (0..7).rev() {
            let u = i as f32 / 6.0;
            let fall = (1.0 - u).powf(1.7);
            let rx = text_w * 0.42 + 10.0 + u * 28.0 * hover;
            let ry = size_f * 0.48 + 5.0 + u * 16.0 * hover;
            fill_round_rect(
                cx - rx,
                cy - ry * 0.9,
                rx * 2.0,
                ry * 1.75,
                ry,
                Color::new(0.0, 0.0, 0.0, 0.09 * fall * hover * a),
            );
        }
    }

    let ink = if !enabled {
        Color::new(0.35, 0.45, 0.5, 0.45 * a)
    } else {
        Color::new(
            0.58 + (0.95 - 0.58) * hover,
            0.75 + (0.98 - 0.75) * hover,
            0.8 + (0.96 - 0.8) * hover,
            (0.88 + 0.12 * hover) * a,
        )
    };
    center_text(font, label, cx, cy, size_base, ink);

    // Smooth underline grow to full measured text width.
    if hover > 0.01 && enabled {
        let under_w = text_w * hover;
        let under_y = cy + size_f * 0.28;
        let under_h = 1.6 + 0.5 * hover;
        fill_round_rect(
            cx - under_w * 0.5 - 4.0 * hover,
            under_y - 2.0,
            under_w + 8.0 * hover,
            under_h + 4.0,
            3.0,
            Color::new(0.15, 0.7, 0.68, 0.1 * hover * a),
        );
        fill_round_rect(
            cx - under_w * 0.5,
            under_y,
            under_w,
            under_h,
            under_h * 0.5,
            Color::new(0.75, 0.98, 0.94, (0.65 + 0.35 * hover) * a),
        );
    }
}

struct SettingsMenu {
    panel: (f32, f32, f32, f32),
    cat_obraz: (f32, f32, f32, f32),
    cat_zvuk: (f32, f32, f32, f32),
    cat_prostredi: (f32, f32, f32, f32),
    fullscreen: (f32, f32, f32, f32),
    bloom: (f32, f32, f32, f32),
    master_minus: (f32, f32, f32, f32),
    master_value: (f32, f32, f32, f32),
    master_plus: (f32, f32, f32, f32),
    music_minus: (f32, f32, f32, f32),
    music_value: (f32, f32, f32, f32),
    music_plus: (f32, f32, f32, f32),
    sfx_minus: (f32, f32, f32, f32),
    sfx_value: (f32, f32, f32, f32),
    sfx_plus: (f32, f32, f32, f32),
    viscosity_minus: (f32, f32, f32, f32),
    viscosity_plus: (f32, f32, f32, f32),
    viscosity_value: (f32, f32, f32, f32),
    edge_effect: [(f32, f32, f32, f32); 4],
    edge_reach_minus: [(f32, f32, f32, f32); 4],
    edge_reach_plus: [(f32, f32, f32, f32); 4],
    edge_reach_value: [(f32, f32, f32, f32); 4],
}

struct FoodPanel {
    panel: (f32, f32, f32, f32),
    close: (f32, f32, f32, f32),
    feeder_enable: (f32, f32, f32, f32),
    feeder_delete: (f32, f32, f32, f32),
    kind_icons: [(f32, f32, f32, f32); 3],
    food_rate_minus: (f32, f32, f32, f32),
    food_rate_value: (f32, f32, f32, f32),
    food_rate_plus: (f32, f32, f32, f32),
    food_radius_minus: (f32, f32, f32, f32),
    food_radius_value: (f32, f32, f32, f32),
    food_radius_plus: (f32, f32, f32, f32),
    food_sense_minus: (f32, f32, f32, f32),
    food_sense_value: (f32, f32, f32, f32),
    food_sense_plus: (f32, f32, f32, f32),
    food_energy_minus: (f32, f32, f32, f32),
    food_energy_value: (f32, f32, f32, f32),
    food_energy_plus: (f32, f32, f32, f32),
    food_harm_minus: (f32, f32, f32, f32),
    food_harm_value: (f32, f32, f32, f32),
    food_harm_plus: (f32, f32, f32, f32),
}

fn stepper_row(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32, f32, f32); 3] {
    let btn = h;
    let gap = 6.0;
    let mid = (w - btn * 2.0 - gap * 2.0).max(48.0);
    [
        (x, y, btn, h),
        (x + btn + gap, y, mid, h),
        (x + btn + gap + mid + gap, y, btn, h),
    ]
}

fn settings_menu(
    frame: &Frame,
    anchor: (f32, f32, f32, f32),
    open: bool,
    section: u8,
) -> SettingsMenu {
    let (gx, gy, gw, gh) = anchor;
    let w = 300.0;
    let x = (gx + gw * 0.5 - w * 0.5).clamp(8.0, frame.sw - w - 8.0);
    let row = 32.0;
    let z = (0.0, 0.0, 0.0, 0.0);

    let mut y = 10.0;
    let cat_obraz = (x + 10.0, y, w - 20.0, row);
    y += row + 4.0;
    let mut fullscreen = z;
    let mut bloom = z;
    if open && section == 1 {
        fullscreen = (x + 10.0, y, w - 20.0, row);
        y += row + 4.0;
        bloom = (x + 10.0, y, w - 20.0, row);
        y += row + 6.0;
    }
    let cat_zvuk = (x + 10.0, y, w - 20.0, row);
    y += row + 4.0;
    let mut master_minus = z;
    let mut master_value = z;
    let mut master_plus = z;
    let mut music_minus = z;
    let mut music_value = z;
    let mut music_plus = z;
    let mut sfx_minus = z;
    let mut sfx_value = z;
    let mut sfx_plus = z;
    if open && section == 2 {
        y += 14.0;
        let [a, b, c] = stepper_row(x + 10.0, y, w - 20.0, 28.0);
        master_minus = a;
        master_value = b;
        master_plus = c;
        y += 36.0;
        y += 14.0;
        let [a, b, c] = stepper_row(x + 10.0, y, w - 20.0, 28.0);
        music_minus = a;
        music_value = b;
        music_plus = c;
        y += 36.0;
        y += 14.0;
        let [a, b, c] = stepper_row(x + 10.0, y, w - 20.0, 28.0);
        sfx_minus = a;
        sfx_value = b;
        sfx_plus = c;
        y += 36.0;
    }
    let cat_prostredi = (x + 10.0, y, w - 20.0, row);
    y += row + 4.0;
    let mut viscosity_minus = z;
    let mut viscosity_value = z;
    let mut viscosity_plus = z;
    let mut edge_effect = [z; 4];
    let mut edge_reach_minus = [z; 4];
    let mut edge_reach_value = [z; 4];
    let mut edge_reach_plus = [z; 4];
    if open && section == 3 {
        y += 14.0;
        let [vm, vv, vp] = stepper_row(x + 10.0, y, w - 20.0, 28.0);
        viscosity_minus = vm;
        viscosity_value = vv;
        viscosity_plus = vp;
        y += 36.0;
        y += 14.0;
        for i in 0..4 {
            let effect_w = (w - 20.0) * 0.42;
            edge_effect[i] = (x + 10.0, y, effect_w, 28.0);
            let rx = x + 10.0 + effect_w + 8.0;
            let rw = w - 20.0 - effect_w - 8.0;
            let [rm, rv, rp] = stepper_row(rx, y, rw, 28.0);
            edge_reach_minus[i] = rm;
            edge_reach_value[i] = rv;
            edge_reach_plus[i] = rp;
            y += 34.0;
        }
    }
    let content_h = if open {
        (y + 10.0).max(row * 3.0 + 24.0)
    } else {
        0.0
    };
    let y0 = (gy + gh + 8.0).clamp(8.0, frame.sh - content_h - 8.0);
    let shift = |r: (f32, f32, f32, f32)| {
        if r.2 <= 0.0 {
            r
        } else {
            (r.0, r.1 + y0, r.2, r.3)
        }
    };
    let shift_arr = |arr: [(f32, f32, f32, f32); 4]| {
        [
            shift(arr[0]),
            shift(arr[1]),
            shift(arr[2]),
            shift(arr[3]),
        ]
    };
    SettingsMenu {
        panel: (x, y0, w, content_h),
        cat_obraz: shift(cat_obraz),
        cat_zvuk: shift(cat_zvuk),
        cat_prostredi: shift(cat_prostredi),
        fullscreen: shift(fullscreen),
        bloom: shift(bloom),
        master_minus: shift(master_minus),
        master_value: shift(master_value),
        master_plus: shift(master_plus),
        music_minus: shift(music_minus),
        music_value: shift(music_value),
        music_plus: shift(music_plus),
        sfx_minus: shift(sfx_minus),
        sfx_value: shift(sfx_value),
        sfx_plus: shift(sfx_plus),
        viscosity_minus: shift(viscosity_minus),
        viscosity_plus: shift(viscosity_plus),
        viscosity_value: shift(viscosity_value),
        edge_effect: shift_arr(edge_effect),
        edge_reach_minus: shift_arr(edge_reach_minus),
        edge_reach_plus: shift_arr(edge_reach_plus),
        edge_reach_value: shift_arr(edge_reach_value),
    }
}


fn food_panel(frame: &Frame, anchor: Option<(f32, f32)>, open: bool) -> FoodPanel {
    let z = (0.0, 0.0, 0.0, 0.0);
    if !open {
        return FoodPanel {
            panel: z,
            close: z,
            feeder_enable: z,
            feeder_delete: z,
            kind_icons: [z; 3],
            food_rate_minus: z,
            food_rate_value: z,
            food_rate_plus: z,
            food_radius_minus: z,
            food_radius_value: z,
            food_radius_plus: z,
            food_sense_minus: z,
            food_sense_value: z,
            food_sense_plus: z,
            food_energy_minus: z,
            food_energy_value: z,
            food_energy_plus: z,
            food_harm_minus: z,
            food_harm_value: z,
            food_harm_plus: z,
        };
    }
    let w = 270.0;
    let h = 338.0;
    let (x, y) = if let Some((fx, fy)) = anchor {
        let gap = 24.0;
        let px = if fx + gap + w <= frame.sw - 12.0 {
            fx + gap
        } else if fx - gap - w >= 12.0 {
            fx - gap - w
        } else {
            (frame.sw - w) * 0.5
        };
        let py = (fy - h * 0.5).clamp(12.0, (frame.sh - h - 12.0).max(12.0));
        (px, py)
    } else {
        let (bx, by, bw, _bh) = food_button_rect(frame);
        let px = (bx + bw * 0.5 - w * 0.5).clamp(12.0, frame.sw - w - 12.0);
        let py = (by - h - 12.0).clamp(12.0, frame.sh - h - 12.0);
        (px, py)
    };
    let panel = (x, y, w, h);
    let close = (x + w - 30.0, y + 8.0, 22.0, 22.0);

    let pad_x = x + 14.0;
    let pad_w = w - 28.0;
    let mut cy = y + 36.0;

    let half = (pad_w - 8.0) * 0.5;
    let feeder_enable = (pad_x, cy, half, 26.0);
    let feeder_delete = (pad_x + half + 8.0, cy, half, 26.0);
    cy += 34.0;

    let kw = (pad_w - 12.0) / 3.0;
    let kind_icons = [
        (pad_x, cy, kw, 24.0),
        (pad_x + kw + 6.0, cy, kw, 24.0),
        (pad_x + 2.0 * (kw + 6.0), cy, kw, 24.0),
    ];
    cy += 30.0;

    cy += 10.0;
    let [food_rate_minus, food_rate_value, food_rate_plus] = stepper_row(pad_x, cy, pad_w, 22.0);
    cy += 26.0;

    cy += 10.0;
    let [food_radius_minus, food_radius_value, food_radius_plus] = stepper_row(pad_x, cy, pad_w, 22.0);
    cy += 26.0;

    cy += 10.0;
    let [food_energy_minus, food_energy_value, food_energy_plus] = stepper_row(pad_x, cy, pad_w, 22.0);
    cy += 26.0;

    cy += 10.0;
    let [food_sense_minus, food_sense_value, food_sense_plus] = stepper_row(pad_x, cy, pad_w, 22.0);
    cy += 26.0;

    cy += 10.0;
    let [food_harm_minus, food_harm_value, food_harm_plus] = stepper_row(pad_x, cy, pad_w, 22.0);

    FoodPanel {
        panel,
        close,
        feeder_enable,
        feeder_delete,
        kind_icons,
        food_rate_minus,
        food_rate_value,
        food_rate_plus,
        food_radius_minus,
        food_radius_value,
        food_radius_plus,
        food_sense_minus,
        food_sense_value,
        food_sense_plus,
        food_energy_minus,
        food_energy_value,
        food_energy_plus,
        food_harm_minus,
        food_harm_value,
        food_harm_plus,
    }
}


fn draw_settings_menu(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    ui: &SettingsMenu,
    section: u8,
    fullscreen: bool,
    bloom_on: bool,
    mix: Mix,
    viscosity: f32,
    edges: [EdgeZone; 4],
) {
    let _ = frame;
    let (x, y, w, h) = ui.panel;
    if h < 8.0 {
        return;
    }
    draw_rectangle(x, y, w, h, Color::new(0.03, 0.05, 0.08, 0.92));
    draw_rectangle(x, y, w, 1.0, Color::new(0.28, 0.9, 0.82, 0.35));
    let dim = Color::new(0.55, 0.7, 0.76, 0.9);
    let mark = |open: bool| if open { "▾ " } else { "▸ " };
    draw_chip(
        font,
        ui.cat_obraz,
        &format!("{}Obraz", mark(section == 1)),
        section == 1,
        mouse,
    );
    if section == 1 && ui.fullscreen.2 > 0.0 {
        draw_chip(
            font,
            ui.fullscreen,
            if fullscreen {
                "Celá obrazovka zapnutá"
            } else {
                "Celá obrazovka"
            },
            fullscreen,
            mouse,
        );
        draw_chip(
            font,
            ui.bloom,
            if bloom_on {
                "Bloom / shadery zapnuté"
            } else {
                "Bloom / shadery"
            },
            bloom_on,
            mouse,
        );
    }
    draw_chip(
        font,
        ui.cat_zvuk,
        &format!("{}Zvuk", mark(section == 2)),
        section == 2,
        mouse,
    );
    if section == 2 && ui.master_minus.2 > 0.0 {
        text(
            font,
            "hlasitost",
            ui.master_minus.0,
            ui.master_minus.1 - 2.0,
            11,
            dim,
        );
        draw_stepper(
            font,
            ui.master_minus,
            ui.master_value,
            ui.master_plus,
            &format!("{:.0}%", mix.master * 100.0),
            mouse,
        );
        text(
            font,
            "hudba",
            ui.music_minus.0,
            ui.music_minus.1 - 2.0,
            11,
            dim,
        );
        draw_stepper(
            font,
            ui.music_minus,
            ui.music_value,
            ui.music_plus,
            &format!("{:.0}%", mix.music * 100.0),
            mouse,
        );
        text(
            font,
            "efekty",
            ui.sfx_minus.0,
            ui.sfx_minus.1 - 2.0,
            11,
            dim,
        );
        draw_stepper(
            font,
            ui.sfx_minus,
            ui.sfx_value,
            ui.sfx_plus,
            &format!("{:.0}%", mix.sfx * 100.0),
            mouse,
        );
    }
    draw_chip(
        font,
        ui.cat_prostredi,
        &format!("{}Prostředí", mark(section == 3)),
        section == 3,
        mouse,
    );
    if section == 3 && ui.viscosity_minus.2 > 0.0 {
        text(
            font,
            "viskozita (odpor)",
            ui.viscosity_minus.0,
            ui.viscosity_minus.1 - 2.0,
            11,
            dim,
        );
        draw_stepper(
            font,
            ui.viscosity_minus,
            ui.viscosity_value,
            ui.viscosity_plus,
            &format!("{viscosity:.1}"),
            mouse,
        );
        text(
            font,
            "okraje · klik = účinek · −/+ = dosah",
            ui.edge_effect[0].0,
            ui.edge_effect[0].1 - 2.0,
            11,
            dim,
        );
        let names = ["L", "P", "D", "H"];
        for i in 0..4 {
            draw_chip(
                font,
                ui.edge_effect[i],
                &format!("{} · {}", names[i], edges[i].effect.label()),
                edges[i].effect != EdgeEffect::None,
                mouse,
            );
            draw_stepper(
                font,
                ui.edge_reach_minus[i],
                ui.edge_reach_value[i],
                ui.edge_reach_plus[i],
                &format!("{:.2}", edges[i].reach),
                mouse,
            );
        }
    }
}


fn draw_food_panel(
    frame: &Frame,
    font: &Option<Font>,
    mouse: (f32, f32),
    ui: &FoodPanel,
    anchor: Option<(f32, f32)>,
    _feed_tool: bool,
    _feed_kind: FoodKind,
    edit: u8,
    selected_feeder: Option<usize>,
    feeders: &[Feeder],
    specs: &[FoodSpec; 3],
) {
    let _ = frame;
    let (x, y, w, h) = ui.panel;
    if w < 8.0 || h < 8.0 {
        return;
    }
    let selected = selected_feeder.and_then(|i| feeders.get(i));
    let cur_kind = selected.map(|f| f.kind).unwrap_or(FoodKind::from_index(edit as usize));
    let spec = specs[cur_kind.index()];
    let (cr, cg, cb) = spec.color;

    // Connector line from anchor to panel
    if let Some((fx, fy)) = anchor {
        let edge_x = if x > fx { x } else { x + w };
        draw_line(fx, fy, edge_x, (y + 24.0).clamp(y, y + h), 1.6, Color::new(cr, cg, cb, 0.45));
    }

    // Panel background & border
    fill_round_rect(x, y, w, h, 8.0, Color::new(0.02, 0.05, 0.08, 0.94));
    stroke_round_rect(x, y, w, h, 8.0, 1.2, Color::new(cr, cg, cb, 0.45));

    // Header
    let gold = Color::new(0.40, 0.95, 0.85, 0.98);
    text(font, "KRMÍTKO", x + 16.0, y + 20.0, 14, gold);
    draw_chip(font, ui.close, "✕", false, mouse);

    // Row 1: Enable / Disable + Delete
    let is_on = selected.map(|f| f.enabled).unwrap_or(false);
    let (on_label, on_active) = if is_on {
        ("ZAPNUTO", true)
    } else {
        ("VYPNUTO", false)
    };
    draw_chip(font, ui.feeder_enable, on_label, on_active, mouse);
    draw_chip(font, ui.feeder_delete, "Smazat", false, mouse);

    // Row 2: Food Kind Selection (3 buttons: Zelené, Jantar, Jed)
    for (i, rect) in ui.kind_icons.iter().enumerate() {
        let k = FoodKind::from_index(i);
        let is_selected = cur_kind.index() == i;
        let label = match k {
            FoodKind::Green => "Zelené",
            FoodKind::Amber => "Jantar",
            FoodKind::Toxic => "Jed",
        };
        draw_chip(font, *rect, label, is_selected, mouse);
    }

    let dim = Color::new(0.55, 0.7, 0.76, 0.9);
    let rate = selected.map(|f| f.rate).unwrap_or(1.0);
    let radius = selected.map(|f| f.radius).unwrap_or(0.35);

    // Row 3: Rate
    text(font, "množství (za s)", ui.food_rate_minus.0, ui.food_rate_minus.1 - 2.0, 11, dim);
    draw_stepper(font, ui.food_rate_minus, ui.food_rate_value, ui.food_rate_plus, &format!("{:.1} / s", rate), mouse);

    // Row 4: Radius
    text(font, "oblast (radius)", ui.food_radius_minus.0, ui.food_radius_minus.1 - 2.0, 11, dim);
    draw_stepper(font, ui.food_radius_minus, ui.food_radius_value, ui.food_radius_plus, &format!("{:.2}", radius), mouse);

    // Row 5: Energy
    text(font, "energie", ui.food_energy_minus.0, ui.food_energy_minus.1 - 2.0, 11, dim);
    draw_stepper(font, ui.food_energy_minus, ui.food_energy_value, ui.food_energy_plus, &format!("{:.2}", spec.energy), mouse);

    // Row 6: Sense
    text(font, "vůně", ui.food_sense_minus.0, ui.food_sense_minus.1 - 2.0, 11, dim);
    draw_stepper(font, ui.food_sense_minus, ui.food_sense_value, ui.food_sense_plus, &format!("{:.2}", spec.sense), mouse);

    // Row 7: Harm
    text(font, "škoda / toxicita", ui.food_harm_minus.0, ui.food_harm_minus.1 - 2.0, 11, dim);
    draw_stepper(font, ui.food_harm_minus, ui.food_harm_value, ui.food_harm_plus, &format!("{:.2}", spec.harm), mouse);
}




fn draw_stepper(
    font: &Option<Font>,
    minus: (f32, f32, f32, f32),
    value: (f32, f32, f32, f32),
    plus: (f32, f32, f32, f32),
    label: &str,
    mouse: (f32, f32),
) {
    draw_chip(font, minus, "−", false, mouse);
    draw_chip(font, value, label, true, mouse);
    draw_chip(font, plus, "+", false, mouse);
}

fn draw_chip(
    font: &Option<Font>,
    rect: (f32, f32, f32, f32),
    label: &str,
    active: bool,
    mouse: (f32, f32),
) {
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let hot = hit_rect(mouse, rect);
    let t = if hot { 1.0 } else { 0.0 };
    let cx = x + w * 0.5;
    let cy = y + h * 0.58;
    let size = ((h * 0.55) * (1.0 + 0.1 * t)).round().clamp(6.0, 64.0) as u16;
    let lift = h * 0.04 * t;
    let split = if hot { h * 0.05 } else { h * 0.022 };

    if hot || active {
        center_text(
            font,
            label,
            cx - split,
            cy - lift,
            size,
            Color::new(1.0, 0.32, 0.5, if hot { 0.22 } else { 0.1 }),
        );
        center_text(
            font,
            label,
            cx + split,
            cy - lift,
            size,
            Color::new(0.2, 1.0, 0.95, if hot { 0.22 } else { 0.1 }),
        );
    }

    let ink = if active {
        Color::new(0.55 + 0.35 * t, 0.98, 0.92, 1.0)
    } else if hot {
        Color::new(0.88, 0.98, 0.96, 1.0)
    } else {
        Color::new(0.62, 0.78, 0.82, 0.88)
    };
    center_text(font, label, cx, cy - lift, size, ink);
}

fn draw_net_live(
    font: &Option<Font>,
    x: f32,
    y: f32,
    w: f32,
    net: &Net,
    span: f32,
    time: f32,
    alpha: f32,
) {
    let mut cols: [Vec<usize>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (i, kind) in net.kind.iter().enumerate() {
        cols[(*kind as usize).min(2)].push(i);
    }
    let rows = cols.iter().map(|c| c.len()).max().unwrap_or(1).max(1);
    let gap = if rows > 1 {
        (span / (rows - 1) as f32).clamp(14.0, 34.0)
    } else {
        0.0
    };
    let height = gap * rows.saturating_sub(1) as f32 + 10.0;
    let inset = 38.0;
    let xs = [x + inset, x + w * 0.5, x + w - inset];
    let band_w = (w * 0.24).clamp(52.0, 84.0);
    let band_colors = [
        Color::new(0.08, 0.32, 0.36, 0.16 * alpha),
        Color::new(0.22, 0.12, 0.36, 0.14 * alpha),
        Color::new(0.36, 0.18, 0.08, 0.14 * alpha),
    ];
    for i in 0..3 {
        draw_rectangle(
            xs[i] - band_w * 0.5,
            y - 10.0,
            band_w,
            height + 28.0,
            band_colors[i],
        );
        // Glass column edge
        draw_rectangle(
            xs[i] - band_w * 0.5,
            y - 10.0,
            2.0,
            height + 28.0,
            Color::new(1.0, 1.0, 1.0, 0.04 * alpha),
        );
    }

    let mut pos = vec![(0.0f32, 0.0f32); net.kind.len().max(net.act.len())];
    let mut placed = vec![false; pos.len()];
    for (col, nodes) in cols.iter().enumerate() {
        if nodes.is_empty() {
            continue;
        }
        let used = gap * nodes.len().saturating_sub(1) as f32;
        let y0 = y + (height - used) * 0.5;
        for (row, &idx) in nodes.iter().enumerate() {
            if idx < pos.len() {
                // Slight organic jitter so it doesn't look like a spreadsheet.
                let jx = ((idx as f32 * 1.7).sin() * 2.2) as f32;
                let jy = ((idx as f32 * 2.3).cos() * 1.4) as f32;
                pos[idx] = (xs[col] + jx, y0 + row as f32 * gap + jy);
                placed[idx] = true;
            }
        }
    }

    let node_r = (gap * 0.3).clamp(7.5, 14.0);

    // Soft underglow for active wires
    for (wi, wire) in net.wires.iter().enumerate() {
        if wire.from >= pos.len()
            || wire.to >= pos.len()
            || !placed[wire.from]
            || !placed[wire.to]
        {
            continue;
        }
        let (x0, y0) = pos[wire.from];
        let (x1, y1) = pos[wire.to];
        let src = net.act.get(wire.from).copied().unwrap_or(0.0);
        let flow = (src.abs() * wire.weight.abs() * 0.55).clamp(0.0, 1.0);
        if flow < 0.1 {
            continue;
        }
        let (cr, cg, cb) = if wire.weight >= 0.0 {
            (0.45, 0.95, 0.85)
        } else {
            (0.95, 0.35, 0.4)
        };
        draw_synapse_curve(
            x0,
            y0,
            x1,
            y1,
            wi,
            5.5 + flow * 4.0,
            Color::new(cr, cg, cb, 0.07 * flow * alpha),
        );
    }

    for (wi, wire) in net.wires.iter().enumerate() {
        if wire.from >= pos.len()
            || wire.to >= pos.len()
            || !placed[wire.from]
            || !placed[wire.to]
        {
            continue;
        }
        let (x0, y0) = pos[wire.from];
        let (x1, y1) = pos[wire.to];
        let src = net.act.get(wire.from).copied().unwrap_or(0.0);
        let flow = (src.abs() * wire.weight.abs() * 0.55).clamp(0.0, 1.0);
        let mag = (wire.weight.abs() / 2.2).clamp(0.12, 1.0);
        let (cr, cg, cb) = if wire.weight >= 0.0 {
            (0.55, 0.92, 0.88)
        } else {
            (0.95, 0.4, 0.45)
        };
        let a = (0.08 + 0.26 * mag + 0.48 * flow) * alpha;
        // myelin-ish double stroke
        draw_synapse_curve(
            x0,
            y0,
            x1,
            y1,
            wi,
            1.8 + mag * 1.4,
            Color::new(cr * 0.4, cg * 0.4, cb * 0.45, a * 0.35),
        );
        draw_synapse_curve(
            x0,
            y0,
            x1,
            y1,
            wi,
            0.7 + mag * 2.0 + flow * 1.2,
            Color::new(cr, cg, cb, a),
        );
        if flow > 0.06 {
            let pulses = 1 + (flow * 2.5) as i32;
            for p in 0..pulses {
                let phase = time * (0.85 + flow * 1.8) + wi as f32 * 0.19 + p as f32 * 0.37;
                let u = phase.fract();
                let (px, py) = synapse_point(x0, y0, x1, y1, wi, u);
                let pulse = Color::new(
                    if wire.weight >= 0.0 { 0.75 } else { 1.0 },
                    if wire.weight >= 0.0 { 1.0 } else { 0.55 },
                    if wire.weight >= 0.0 { 0.92 } else { 0.5 },
                    (0.5 + 0.5 * flow) * alpha,
                );
                draw_circle(
                    px,
                    py,
                    6.0 + flow * 3.5,
                    Color::new(pulse.r, pulse.g, pulse.b, 0.1 * alpha),
                );
                draw_circle(px, py, 2.2 + flow * 2.2, pulse);
                draw_circle(
                    px,
                    py,
                    0.9,
                    Color::new(1.0, 1.0, 1.0, 0.6 * flow * alpha),
                );
            }
            let (tx, ty) = synapse_point(x0, y0, x1, y1, wi, 0.92);
            let (sx, sy) = synapse_point(x0, y0, x1, y1, wi, 0.82);
            let dx = tx - sx;
            let dy = ty - sy;
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let ux = dx / len;
            let uy = dy / len;
            let px = -uy;
            let py = ux;
            let tip = 4.0 + flow * 2.0;
            draw_triangle(
                macroquad::math::Vec2::new(tx + ux * tip * 0.2, ty + uy * tip * 0.2),
                macroquad::math::Vec2::new(
                    tx - ux * tip + px * tip * 0.55,
                    ty - uy * tip + py * tip * 0.55,
                ),
                macroquad::math::Vec2::new(
                    tx - ux * tip - px * tip * 0.55,
                    ty - uy * tip - py * tip * 0.55,
                ),
                Color::new(cr, cg, cb, (0.35 + 0.5 * flow) * alpha),
            );
        }
    }

    for (i, &(px, py)) in pos.iter().enumerate() {
        if !placed.get(i).copied().unwrap_or(false) {
            continue;
        }
        let act = net.act.get(i).copied().unwrap_or(0.0);
        let k = act.abs().clamp(0.0, 1.0);
        let kind = net.kind.get(i).copied().unwrap_or(1);
        let (r, g, b) = match kind {
            0 => (0.28, 0.92, 0.95),
            2 => (0.98, 0.62, 0.3),
            _ => (0.8, 0.5, 0.98),
        };
        let lit = 0.38 + 0.62 * ((act / 2.0).clamp(-1.0, 1.0) * 0.5 + 0.5);
        let pulse = 1.0 + 0.07 * (time * 3.0 + i as f32 * 0.7).sin() * k;
        let rad = node_r * pulse;

        // Drop shadow for depth
        draw_circle(
            px + 1.6,
            py + 2.2,
            rad + 1.0,
            Color::new(0.0, 0.0, 0.0, 0.28 * alpha),
        );
        // Soma halo
        draw_circle(
            px,
            py,
            rad + 7.0 + k * 5.0,
            Color::new(r, g, b, (0.05 + 0.2 * k) * alpha),
        );
        draw_circle(
            px,
            py,
            rad + 3.0,
            Color::new(r, g, b, (0.1 + 0.22 * k) * alpha),
        );
        // Membrane
        draw_circle(px, py, rad, Color::new(0.025, 0.04, 0.06, alpha));
        draw_circle(
            px,
            py,
            rad * 0.88,
            Color::new(r * 0.22 * lit, g * 0.24 * lit, b * 0.28 * lit, 0.9 * alpha),
        );
        // Cytoplasm gradient (brighter toward top-left)
        draw_circle(
            px - rad * 0.2,
            py - rad * 0.22,
            rad * 0.5,
            Color::new(r * 0.55, g * 0.55, b * 0.6, (0.18 + 0.2 * k) * alpha),
        );
        draw_circle_lines(
            px,
            py,
            rad,
            2.0,
            Color::new(r * lit, g * lit, b * lit, alpha),
        );
        draw_circle_lines(
            px,
            py,
            rad * 0.72,
            1.0,
            Color::new(r, g, b, 0.25 * alpha),
        );
        // Nucleus + nucleolus
        let nr = rad * (0.3 + 0.2 * k);
        draw_circle(
            px - rad * 0.1,
            py - rad * 0.08,
            nr,
            Color::new(r * 0.9, g * 0.88, b * 0.95, (0.55 + 0.4 * k) * alpha),
        );
        draw_circle(
            px - rad * 0.16,
            py - rad * 0.16,
            nr * 0.32,
            Color::new(1.0, 1.0, 1.0, 0.4 * alpha),
        );

        if kind == 0 {
            // Dendrite tree (sensors)
            for d in 0..3 {
                let ang = -2.4 + d as f32 * 0.55;
                let len = rad + 7.0 + (d as f32) * 1.5;
                let ex = px + ang.cos() * len;
                let ey = py + ang.sin() * len;
                draw_line(px, py, ex, ey, 1.5, Color::new(r, g, b, 0.45 * alpha));
                draw_circle(ex, ey, 2.0, Color::new(r, g, b, 0.65 * alpha));
                let bx = ex + (ang - 0.5).cos() * 4.0;
                let by = ey + (ang - 0.5).sin() * 4.0;
                draw_line(ex, ey, bx, by, 1.1, Color::new(r, g, b, 0.35 * alpha));
            }
        } else if kind == 2 {
            // Axon + bouton (actions)
            let tip = px + rad + 9.0;
            draw_line(px + rad, py, tip, py, 1.8, Color::new(r, g, b, 0.55 * alpha));
            draw_circle(tip + 1.5, py, 3.0, Color::new(r, g, b, 0.35 * alpha));
            draw_circle(tip + 1.5, py, 2.0, Color::new(r, g, b, 0.8 * alpha));
        } else {
            // Tiny radial fibers for hidden
            for d in 0..4 {
                let ang = d as f32 * 1.57 + 0.4;
                draw_line(
                    px + ang.cos() * rad * 0.7,
                    py + ang.sin() * rad * 0.7,
                    px + ang.cos() * (rad + 3.5),
                    py + ang.sin() * (rad + 3.5),
                    1.0,
                    Color::new(r, g, b, 0.28 * alpha),
                );
            }
        }
    }

    let label_y = y + height + 30.0;
    let labels = [
        (xs[0], "smysly", Color::new(0.3, 0.9, 0.94, 0.95 * alpha)),
        (xs[1], "skryté", Color::new(0.78, 0.48, 0.98, 0.95 * alpha)),
        (xs[2], "akce", Color::new(0.98, 0.58, 0.28, 0.95 * alpha)),
    ];
    for (cx, label, color) in labels {
        let lw = label.chars().count() as f32 * 6.5;
        text(font, label, cx - lw * 0.5, label_y, 12, color);
    }
    text(
        font,
        "záře = aktivita · tečky = impulzy · Detail = 3D",
        x + 4.0,
        label_y + 20.0,
        11,
        Color::new(0.55, 0.7, 0.76, 0.85 * alpha),
    );
}

fn net_3d_close_rect(frame: &Frame) -> (f32, f32, f32, f32) {
    (frame.sw - 118.0, 16.0, 100.0, 34.0)
}

fn net_layout_3d(net: &Net) -> Vec<macroquad::math::Vec3> {
    use macroquad::prelude::vec3;
    let mut cols: [Vec<usize>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (i, kind) in net.kind.iter().enumerate() {
        cols[(*kind as usize).min(2)].push(i);
    }
    let mut out = vec![vec3(0.0, 0.0, 0.0); net.kind.len()];
    let xs = [-1.55_f32, 0.0, 1.55];
    for (col, nodes) in cols.iter().enumerate() {
        let n = nodes.len().max(1) as f32;
        let span = (n * 0.38).max(0.8).min(3.4);
        for (row, &idx) in nodes.iter().enumerate() {
            let t = if nodes.len() == 1 {
                0.0
            } else {
                row as f32 / (nodes.len() - 1) as f32 - 0.5
            };
            let y = t * span;
            let z = ((idx as f32 * 1.9).sin() * 0.35)
                + ((idx as f32 * 0.7).cos() * 0.18)
                + (col as f32 - 1.0) * 0.08;
            let x = xs[col] + ((idx as f32 * 2.1).cos() * 0.08);
            if idx < out.len() {
                out[idx] = vec3(x, y, z);
            }
        }
    }
    out
}

fn project_net3(
    cam: &Camera3D,
    p: macroquad::math::Vec3,
    sw: f32,
    sh: f32,
) -> Option<(f32, f32)> {
    let clip = cam.matrix() * p.extend(1.0);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.z < -1.05 || ndc.z > 1.05 {
        return None;
    }
    Some(((ndc.x + 1.0) * 0.5 * sw, (1.0 - ndc.y) * 0.5 * sh))
}

fn draw_net_3d_overlay(
    frame: &Frame,
    font: &Option<Font>,
    net: &Net,
    time: f32,
    yaw: f32,
    pitch: f32,
    dist: f32,
    mouse: (f32, f32),
) {
    use macroquad::prelude::{vec3, Camera3D};

    // Dim the world behind
    draw_rectangle(
        0.0,
        0.0,
        frame.sw,
        frame.sh,
        Color::new(0.01, 0.03, 0.04, 0.88),
    );

    let eye = vec3(
        dist * yaw.cos() * pitch.cos(),
        dist * pitch.sin(),
        dist * yaw.sin() * pitch.cos(),
    );
    let cam = Camera3D {
        position: eye,
        target: vec3(0.0, 0.0, 0.0),
        up: vec3(0.0, 1.0, 0.0),
        fovy: 48.0_f32.to_radians(),
        ..Default::default()
    };
    set_camera(&cam);

    let pos = net_layout_3d(net);

    // Soft ground disc
    draw_sphere(
        vec3(0.0, -2.1, 0.0),
        0.08,
        None,
        Color::new(0.2, 0.7, 0.7, 0.15),
    );

    // Synapses
    for (wi, wire) in net.wires.iter().enumerate() {
        if wire.from >= pos.len() || wire.to >= pos.len() {
            continue;
        }
        let a = pos[wire.from];
        let b = pos[wire.to];
        let src = net.act.get(wire.from).copied().unwrap_or(0.0);
        let flow = (src.abs() * wire.weight.abs() * 0.5).clamp(0.0, 1.0);
        let mag = (wire.weight.abs() / 2.2).clamp(0.15, 1.0);
        let col = if wire.weight >= 0.0 {
            Color::new(0.4, 0.95, 0.85, 0.18 + 0.55 * mag + 0.25 * flow)
        } else {
            Color::new(0.95, 0.35, 0.4, 0.18 + 0.55 * mag + 0.25 * flow)
        };
        draw_line_3d(a, b, col);
        if flow > 0.08 {
            let u = (time * (1.1 + flow) + wi as f32 * 0.27).fract();
            let p = a.lerp(b, u);
            draw_sphere(
                p,
                0.045 + flow * 0.04,
                None,
                Color::new(1.0, 1.0, 0.95, 0.75 * flow),
            );
        }
    }

    // Neurons
    for (i, p) in pos.iter().enumerate() {
        let act = net.act.get(i).copied().unwrap_or(0.0);
        let k = act.abs().clamp(0.0, 1.0);
        let kind = net.kind.get(i).copied().unwrap_or(1);
        let (r, g, b) = match kind {
            0 => (0.3, 0.92, 0.95),
            2 => (0.98, 0.6, 0.3),
            _ => (0.8, 0.5, 0.98),
        };
        let rad = 0.1 + 0.05 * k;
        draw_sphere(
            *p,
            rad + 0.06,
            None,
            Color::new(r, g, b, 0.12 + 0.2 * k),
        );
        draw_sphere(*p, rad, None, Color::new(r * 0.35, g * 0.35, b * 0.4, 0.95));
        draw_sphere(
            *p + vec3(-0.02, 0.025, 0.02),
            rad * 0.35,
            None,
            Color::new(1.0, 1.0, 1.0, 0.45),
        );
    }

    set_default_camera();

    // Screen-space labels (active / nearby)
    let mut labeled = 0usize;
    for (i, p) in pos.iter().enumerate() {
        let Some((sx, sy)) = project_net3(&cam, *p, frame.sw, frame.sh) else {
            continue;
        };
        if sx < 40.0 || sy < 50.0 || sx > frame.sw - 40.0 || sy > frame.sh - 40.0 {
            continue;
        }
        let act = net.act.get(i).copied().unwrap_or(0.0).abs();
        let near = (mouse.0 - sx).hypot(mouse.1 - sy) < 28.0;
        if act < 0.35 && !near && labeled > 14 {
            continue;
        }
        if act < 0.12 && !near {
            continue;
        }
        let label = net
            .labels
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("?");
        let kind = net.kind.get(i).copied().unwrap_or(1);
        let col = match kind {
            0 => Color::new(0.45, 0.95, 0.98, 0.95),
            2 => Color::new(1.0, 0.72, 0.4, 0.95),
            _ => Color::new(0.88, 0.7, 1.0, 0.95),
        };
        let tw = label.chars().count() as f32 * 6.2 + 10.0;
        draw_rectangle(
            sx - tw * 0.5,
            sy - 22.0,
            tw,
            16.0,
            Color::new(0.02, 0.05, 0.07, 0.72),
        );
        center_text(font, label, sx, sy - 14.0, 11, col);
        labeled += 1;
    }

    // Header chrome
    text(
        font,
        "Neuronová síť · 3D",
        24.0,
        32.0,
        22,
        Color::new(0.92, 0.97, 0.98, 1.0),
    );
    text(
        font,
        "táhni myší = otáčení · kolečko = zoom · Esc = zavřít",
        24.0,
        54.0,
        13,
        Color::new(0.55, 0.75, 0.78, 0.92),
    );
    text(
        font,
        &format!(
            "{} neuronů · {} synapsí · hover = popisek",
            net.kind.len(),
            net.wires.len()
        ),
        24.0,
        74.0,
        12,
        Color::new(0.45, 0.65, 0.7, 0.9),
    );

    // Legend
    let legend = [
        (Color::new(0.3, 0.92, 0.95, 1.0), "smysly"),
        (Color::new(0.8, 0.5, 0.98, 1.0), "skryté"),
        (Color::new(0.98, 0.6, 0.3, 1.0), "akce"),
    ];
    for (i, (col, name)) in legend.iter().enumerate() {
        let lx = 24.0 + i as f32 * 110.0;
        let ly = frame.sh - 36.0;
        draw_circle(lx + 8.0, ly, 6.0, *col);
        text(font, name, lx + 20.0, ly + 4.0, 12, Color::new(0.75, 0.88, 0.9, 0.95));
    }

    let close = net_3d_close_rect(frame);
    let hot = hit_rect(mouse, close);
    draw_rectangle(
        close.0,
        close.1,
        close.2,
        close.3,
        if hot {
            Color::new(0.14, 0.32, 0.34, 0.98)
        } else {
            Color::new(0.06, 0.14, 0.16, 0.92)
        },
    );
    draw_rectangle_lines(
        close.0,
        close.1,
        close.2,
        close.3,
        1.3,
        Color::new(0.28, 0.9, 0.82, if hot { 1.0 } else { 0.6 }),
    );
    center_text(
        font,
        "Zavřít",
        close.0 + close.2 * 0.5,
        close.1 + close.3 * 0.55,
        14,
        Color::new(0.9, 0.98, 0.96, 1.0),
    );
}

fn synapse_ctrl(x0: f32, y0: f32, x1: f32, y1: f32, salt: usize) -> (f32, f32) {
    let mx = (x0 + x1) * 0.5;
    let my = (y0 + y1) * 0.5;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let side = if salt % 2 == 0 { 1.0 } else { -1.0 };
    let bulge = (14.0 + (salt % 5) as f32 * 3.0) * side;
    (mx - dy / len * bulge, my + dx / len * bulge)
}

fn synapse_point(x0: f32, y0: f32, x1: f32, y1: f32, salt: usize, t: f32) -> (f32, f32) {
    let (cx, cy) = synapse_ctrl(x0, y0, x1, y1, salt);
    let u = 1.0 - t;
    (
        u * u * x0 + 2.0 * u * t * cx + t * t * x1,
        u * u * y0 + 2.0 * u * t * cy + t * t * y1,
    )
}

fn draw_synapse_curve(x0: f32, y0: f32, x1: f32, y1: f32, salt: usize, width: f32, color: Color) {
    let n = 14;
    let mut prev = (x0, y0);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let p = synapse_point(x0, y0, x1, y1, salt, t);
        draw_line(prev.0, prev.1, p.0, p.1, width, color);
        prev = p;
    }
}

#[allow(dead_code)]
fn draw_genome(font: &Option<Font>, x: f32, y: f32, w: f32, s: &Stats) -> f32 {
    let gold = Color::new(0.28, 0.9, 0.82, 1.0);
    let dim = Color::new(0.55, 0.7, 0.76, 0.86);
    let ink = Color::new(0.9, 0.95, 0.97, 0.95);
    let good_c = Color::new(0.55, 0.82, 0.58, 1.0);
    let bad_c = Color::new(0.86, 0.48, 0.42, 1.0);
    let (goods, bads) = genome_sides(s);
    let (title, about) = character(s);
    let mut y = y;
    text(font, "POZITIVA", x, y + 12.0, 11, gold);
    y += 22.0;
    y = write_notes(font, x, y, w, &goods, good_c, "Bez výrazné výhody.");
    y += 6.0;
    text(font, "NEGATIVA", x, y + 12.0, 11, gold);
    y += 22.0;
    y = write_notes(font, x, y, w, &bads, bad_c, "Bez výrazné vady.");
    y += 8.0;
    text(font, &title, x, y + 16.0, 16, ink);
    y += 26.0;
    let chars = ((w / 6.6).floor() as usize).clamp(22, 52);
    for line in wrap_words(&about, chars) {
        text(font, &line, x, y + 14.0, 13, dim);
        y += 16.0;
    }
    y + 10.0
}

fn write_notes(
    font: &Option<Font>,
    x: f32,
    mut y: f32,
    w: f32,
    notes: &[String],
    color: Color,
    empty: &str,
) -> f32 {
    let chars = ((w / 6.4).floor() as usize).clamp(22, 52);
    if notes.is_empty() {
        text(font, empty, x, y + 13.0, 13, color);
        return y + 18.0;
    }
    for note in notes {
        for line in wrap_words(note, chars) {
            text(font, &line, x, y + 13.0, 13, color);
            y += 16.0;
        }
        y += 2.0;
    }
    y
}

fn genome_sides(s: &Stats) -> (Vec<String>, Vec<String>) {
    let mut good = Vec::new();
    let mut bad = Vec::new();
    let push = |list: &mut Vec<(f32, String)>, score: f32, text: &str| {
        if score > 0.08 {
            list.push((score, text.to_owned()));
        }
    };
    let mut g: Vec<(f32, String)> = Vec::new();
    let mut b: Vec<(f32, String)> = Vec::new();
    push(
        &mut g,
        (s.hetero - 0.85) / 0.4,
        "Ústa dobře berou cizí hmotu.",
    );
    push(&mut b, (0.5 - s.hetero) / 0.3, "Z agaru a těl bere málo.");
    push(
        &mut g,
        (s.learn - 0.18) / 0.2,
        "Synapse se přepisují rychle.",
    );
    push(&mut b, (0.09 - s.learn) / 0.08, "Síť se téměř neučí.");
    push(&mut g, (0.65 - s.mass) / 0.25, "Lehké tělo, levný klid.");
    push(
        &mut b,
        (s.mass - 1.05) / 0.35,
        "Těžké tělo, drahý metabolismus.",
    );
    push(&mut g, (s.birth_w - 1.05) / 0.6, "Silně tlačí na potomka.");
    push(
        &mut b,
        (0.45 - s.birth_w) / 0.35,
        "Rozmnožení ho málo táhne.",
    );
    push(&mut g, (8.0 - s.maturity) / 4.0, "Dospívá brzy.");
    push(&mut b, (s.maturity - 13.0) / 5.0, "Na potomka čeká dlouho.");
    push(
        &mut g,
        ((s.hunger_w - 1.05) / 0.4).min((1.55 - s.hunger_w) / 0.3),
        "Hlad ho vede k jídlu.",
    );
    push(
        &mut b,
        (s.hunger_w - 1.55) / 0.4,
        "Hlad přebíjí ostatní pudy.",
    );
    push(
        &mut g,
        ((s.pain_w - 1.05) / 0.4).min((1.5 - s.pain_w) / 0.3),
        "Bolest ho včas brzdí.",
    );
    push(&mut b, (s.pain_w - 1.5) / 0.45, "Bolest ho snadno zastaví.");
    push(
        &mut g,
        (s.novelty_w - 0.55) / 0.5,
        "Nové podněty ho táhnou.",
    );
    push(&mut b, (0.18 - s.novelty_w) / 0.18, "Nové téměř nevnímá.");
    push(
        &mut g,
        (0.75 - s.setpoint) / 0.2,
        "Vystačí s menší zásobou.",
    );
    push(
        &mut b,
        (s.setpoint - 1.1) / 0.25,
        "Chce velkou zásobu a dřív hladoví.",
    );
    push(
        &mut g,
        (24.0 - s.neurons as f32) / 8.0,
        "Malá síť, levný mozek.",
    );
    push(
        &mut b,
        (s.neurons as f32 - 32.0) / 10.0,
        "Velká síť, drahé myšlení.",
    );
    g.sort_by(|a, c| c.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    b.sort_by(|a, c| c.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    good.extend(g.into_iter().take(3).map(|(_, t)| t));
    bad.extend(b.into_iter().take(3).map(|(_, t)| t));
    (good, bad)
}

fn character(s: &Stats) -> (String, String) {
    let mut best = ("Vyvážený organismus", 0.0f32);
    let keep = |best: &mut (&str, f32), name: &'static str, score: f32| {
        if score > best.1 {
            *best = (name, score);
        }
    };
    keep(&mut best, "Žrout hmoty", (s.hetero - 0.7) / 0.45);
    keep(&mut best, "Rychle se učí", (s.learn - 0.14) / 0.2);
    keep(&mut best, "Zaměřený na potomky", (s.birth_w - 0.9) / 0.6);
    keep(&mut best, "Zvědavý", (s.novelty_w - 0.4) / 0.5);
    keep(&mut best, "Opatrný", (s.pain_w - 1.0) / 0.6);
    keep(&mut best, "Hnaný hladem", (s.hunger_w - 1.0) / 0.6);
    keep(&mut best, "Lehký běžec", (0.7 - s.mass) / 0.3);
    keep(&mut best, "Těžké tělo", (s.mass - 1.0) / 0.35);
    let feed = if s.hetero > 0.9 {
        "Živí se hmotou z agaru a ze sousedů."
    } else {
        "Ústa berou hmotu, žádná jiná cesta k energii není."
    };
    let temper = if s.hunger_w >= s.pain_w && s.hunger_w >= s.novelty_w && s.hunger_w > 1.05 {
        "Nejsilnější pud je hlad."
    } else if s.pain_w >= s.hunger_w && s.pain_w > 1.05 {
        "Nejsilnější pud je vyhnout se bolesti."
    } else if s.novelty_w > 0.6 {
        "Nové ho táhne víc než klid."
    } else if s.birth_w > 1.15 {
        "Nad hledáním potravy často vyhraje potomek."
    } else {
        "Pudy jsou vyrovnané."
    };
    let learn = if s.learn > 0.22 {
        "Synapse se přepisují rychle."
    } else if s.learn < 0.1 {
        "Síť se mění pomalu."
    } else {
        "Učení je průměrné."
    };
    let size = if s.body > 0.072 {
        "Články jsou velké."
    } else if s.body < 0.05 {
        "Články jsou drobné."
    } else {
        "Velikost článků je běžná."
    };
    let about = format!(
        "{feed} {temper} Tělo má {} článků a {} neuronů. {size} {learn}",
        s.nodes, s.neurons
    );
    (best.0.to_owned(), about)
}

fn wrap_words(s: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
        } else if cur.chars().count() + 1 + word.chars().count() <= max_chars {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[allow(dead_code)]
fn bar(font: &Option<Font>, x: f32, w: f32, y: &mut f32, label: &str, value: f32, color: Color) {
    text(
        font,
        label,
        x,
        *y + 11.0,
        12,
        Color::new(0.55, 0.7, 0.76, 0.9),
    );
    *y += 13.0;
    draw_rectangle(x, *y, w, 4.0, Color::new(0.15, 0.28, 0.34, 0.45));
    draw_rectangle(x, *y, w * value.clamp(0.0, 1.0), 4.0, color);
    *y += 14.0;
}

#[allow(dead_code)]
fn center_bar(font: &Option<Font>, x: f32, width: f32, y: &mut f32, label: &str, value: f32) {
    text(
        font,
        label,
        x,
        *y + 11.0,
        12,
        Color::new(0.55, 0.7, 0.76, 0.9),
    );
    *y += 13.0;
    draw_rectangle(x, *y, width, 4.0, Color::new(0.15, 0.28, 0.34, 0.45));
    let mid = x + width * 0.5;
    let w = width * 0.5 * value.clamp(-1.0, 1.0);
    if w >= 0.0 {
        draw_rectangle(mid, *y, w, 4.0, Color::new(0.45, 0.95, 0.9, 1.0));
    } else {
        draw_rectangle(mid + w, *y, -w, 4.0, Color::new(0.35, 0.55, 0.75, 1.0));
    }
    *y += 14.0;
}

fn text(font: &Option<Font>, s: &str, x: f32, y: f32, size: u16, color: Color) {
    match font {
        Some(f) => {
            draw_text_ex(
                s,
                x,
                y,
                TextParams {
                    font: Some(f),
                    font_size: size,
                    font_scale: 1.0,
                    color,
                    ..Default::default()
                },
            );
        }
        None => {
            draw_text(s, x, y, size as f32, color);
        }
    }
}

fn text_scaled(
    font: &Option<Font>,
    s: &str,
    x: f32,
    y: f32,
    base_size: u16,
    scale: f32,
    color: Color,
) {
    match font {
        Some(f) => {
            draw_text_ex(
                s,
                x,
                y,
                TextParams {
                    font: Some(f),
                    font_size: base_size,
                    font_scale: scale,
                    color,
                    ..Default::default()
                },
            );
        }
        None => {
            draw_text(s, x, y, base_size as f32 * scale, color);
        }
    }
}

fn center_text_scaled(
    font: &Option<Font>,
    label: &str,
    cx: f32,
    y: f32,
    base_size: u16,
    scale: f32,
    color: Color,
) {
    let width = measure_text(label, font.as_ref(), base_size, 1.0).width * scale;
    text_scaled(font, label, cx - width * 0.5, y, base_size, scale, color);
}

fn world_scale(frame: &Frame, cam: &Cam) -> f32 {
    frame.sh * 0.5 * cam.zoom
}

fn draw_energy_bar(frame: &Frame, cam: &Cam, font: &Option<Font>, app: &Appearance<'_>) {
    let mut top = app.world_node(0);
    for n in app.world_nodes() {
        if n.y > top.y {
            top = n;
        }
    }
    let (cx, cy) = world_to_screen(frame, cam, top);
    let s = world_scale(frame, cam);
    let bw = (app.radius * 1.7 * s).clamp(22.0, 54.0);
    let bh = 3.2;
    let x = cx - bw * 0.5;
    let y = cy - app.node_radius * 2.1 * s - 8.0;
    let fill = (app.energy / 2.0).clamp(0.0, 1.0);
    draw_rectangle(
        x - 1.0,
        y - 1.0,
        bw + 2.0,
        bh + 2.0,
        Color::new(0.02, 0.04, 0.06, 0.7),
    );
    draw_rectangle(x, y, bw, bh, Color::new(0.12, 0.2, 0.24, 0.85));
    let (r, g, b) = if fill > 0.55 {
        (0.28, 0.92, 0.55)
    } else if fill > 0.28 {
        (0.95, 0.78, 0.28)
    } else {
        (0.95, 0.35, 0.38)
    };
    draw_rectangle(x, y, bw * fill, bh, Color::new(r, g, b, 0.95));
    text(
        font,
        &format!("{}", app.generation),
        x + bw + 4.0,
        y + bh + 1.0,
        11,
        Color::new(0.85, 0.92, 0.95, 0.9),
    );
}

fn spawn_motes(motes: &mut Vec<Mote>, apps: &[Appearance<'_>]) {
    for app in apps {
        if app.thrust.abs() < 0.25 || app.nodes.len() < 2 {
            continue;
        }
        let head = app.world_node(0);
        let tail = app.world_node(app.nodes.len() - 1);
        let axis = Vec2::new(head.x - tail.x, head.y - tail.y);
        let len = (axis.x * axis.x + axis.y * axis.y).sqrt().max(1e-4);
        let dir = Vec2::new(axis.x / len, axis.y / len);
        motes.push(Mote {
            pos: head,
            vel: Vec2::new(-dir.x, -dir.y) * (0.12 + app.thrust.abs() * 0.18),
            life: 0.35,
            hue: app.hue,
        });
    }
    if motes.len() > 600 {
        let extra = motes.len() - 600;
        motes.drain(0..extra);
    }
}

fn step_motes(motes: &mut Vec<Mote>, dt: f32) {
    for mote in motes.iter_mut() {
        mote.pos = Vec2::new(mote.pos.x + mote.vel.x * dt, mote.pos.y + mote.vel.y * dt);
        mote.life -= dt;
    }
    motes.retain(|m| m.life > 0.0);
}

fn draw_spark(frame: &Frame, cam: &Cam, spark: &Spark) {
    let (x, y) = world_to_screen(frame, cam, spark.pos);
    if x < -20.0 || x > frame.sw + 20.0 || y < -20.0 || y > frame.sh + 20.0 {
        return;
    }
    let t = (spark.life / spark.max_life).clamp(0.0, 1.0);
    let unit = frame.sh * 0.012 * cam.zoom;
    match spark.style {
        1 => {
            // Blood flecks — saturated red, elongated streak feeling via two circles.
            let (r, g, b) = (0.95, 0.18 + spark.hue * 0.15, 0.22);
            let rad = (spark.size * unit * (0.5 + 0.55 * t)).max(1.4);
            draw_circle(x, y, rad * 1.6, Color::new(r, g, b, 0.35 * t));
            draw_circle(x, y, rad, Color::new(r, g * 0.7, b * 0.7, 0.95 * t));
        }
        2 => {
            // Death chunks — larger, hue of the body, soft glow.
            let (r, g, b) = hsv(spark.hue, 0.7, 1.0);
            let rad = (spark.size * unit * (0.65 + 0.5 * t)).max(2.0);
            draw_circle(x, y, rad * 1.8, Color::new(r, g, b, 0.22 * t));
            draw_circle(x, y, rad, Color::new(r, g, b, 0.85 * t));
            draw_circle(
                x,
                y,
                rad * 0.4,
                Color::new((r + 0.35).min(1.0), (g + 0.35).min(1.0), (b + 0.35).min(1.0), t),
            );
        }
        _ => {
            let (r, g, b) = hsv(spark.hue, 0.75, 1.0);
    draw_circle(
        x,
        y,
                (spark.size * unit * (0.55 + 0.45 * t)).max(1.1),
                Color::new(r, g, b, t),
    );
        }
    }
}

fn draw_mote(frame: &Frame, cam: &Cam, mote: &Mote) {
    let (x, y) = world_to_screen(frame, cam, mote.pos);
    if x < 0.0 || x > frame.sw || y < 0.0 || y > frame.sh {
        return;
    }
    let t = (mote.life / 0.35).clamp(0.0, 1.0);
    let (r, g, b) = hsv(mote.hue, 0.7, 1.0);
    let unit = frame.sh * 0.004 * cam.zoom;
    draw_circle(
        x,
        y,
        ((1.3 + t) * unit).max(0.8),
        Color::new(r, g, b, 0.45 * t),
    );
}

fn hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = h.rem_euclid(1.0);
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}
