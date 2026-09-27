# Specifikace Aether

Kompletní funkční a technická specifikace podle aktuálního kódu (v0.1.0).

---

## 1. Produkt

### 1.1 Cíl
Sandbox evoluční simulace: organismy žijí v Petri misce, rozhodují se jen vlastní NN odvozenou z genomu. Hráč pozoruje, krmí, nastavuje prostředí, inspectuje jedince; nesměřuje chování skriptem.

### 1.2 Jazyk UI
Čeština (menu, labely jídel, inspect texty).

### 1.3 Fáze aplikace

| Phase | Vstup | Výstup |
|-------|--------|--------|
| `Title` | Lobby | Nová hra / Načíst / Nastavení / Ukončit |
| `Running` | Simulace | Domů (title), pauza, nástroje, saves |

`ScreenTransit` (~1.15 s): crossfade / special new-game burst; midpoint spawnuje svět a flipne phase.

---

## 2. Simulační jádro

### 2.1 Souřadnice
- **Dish-local**: organismus/jídlo vůči středu misky  
- **Table**: `dish.pos + local`  
- Kamera mapuje table → screen přes `world_scale` = `min(sw,sh)*0.5*zoom` a `view_origin` z `dish_fit_rect` (pad + TOOL bar)

### 2.2 Limity (`tune.rs`)

| Konstanta | Hodnota | Význam |
|-----------|---------|--------|
| `DISH` | 1.0 | výchozí half-extent |
| `GRID` | 128 | field resolution |
| `MAX_POP` | 96 | max organismů |
| `MAX_DISHES` | 8 | max misek |
| `FOOD_CAP` | 48 | soft food ceiling |
| `DT_CAP` | 1/30 | max sim dt |
| `GENOME_LENGTH` | 128 | výchozí # genů |
| `GENOME_LENGTH_MAX` | 1000 | strop |
| `INNER_NEURONS` | 16 | modulo pro inner IDs |
| `NODES_MIN`/`MAX` | 2 / 12 | délka těla |
| `NODES_BIRTH_*` | 3 / 7 | rozsah při narození |
| `BIT_FLIP` | 0.001 | mutace bitu |
| `WEIGHT_DIVISOR` | 8000 | i16 → float váha |
| `FOOD_BITE` | 0.12 | dosah snězení (min. s radiusem těla) |
| `MOUTH_OPEN` | 0.32 | práh survival tlamy |
| `REPRO_THRESHOLD` | 0.92 | energie pro potomka |
| `START_ENERGY` | 1.35 | energie nového jedince |

### 2.3 Genom

**Gene (32 bit):**
```
31      source type (0=sensor, 1=inner)
30..24  source id % count
23      sink type (0=inner, 1=action)
22..16  sink id % count
15..0   weight i16 / 8000
```

**Sensory ports** (`SENSOR_BASE` = 26): pozice, okraj, feromon, hustota, crowd grad, věk, oscilátor, vůně druhů jídla, Δ vůně, **jídlo vpřed/stranou**, kin, touch, energie, bolest, svalové délky.  
**Action ports** (`ACTION_BASE` = 15): move X/Y/forward/random/cardinal, pheromone, responsiveness, oscillator, kill-forward, enzyme, reproduce, growth + per-node muscles.

### 2.4 Brain
- Build z genomu, prune vnitřních bez cesty k akci  
- Step: sensors → (inners s 1-step memory) → actions  
- Plasticita: eligibility traces, tonic / expected reward, časově omezené `DW_PER_SEC`

### 2.5 Organismus
- Body: `nodes` + `vels`, spring `SPRING_K`, muscle shorten  
- Energie: basal (mass + neurons + synapses), move/signal cost, eat yield, age tax  
- Repro: threshold + cooldown + child energy split  
- Bite: `ACT_KILL_FORWARD` → attack (ne food mouth)  
- Food valence: učení aversion u Toxic  
- **Hardcoded survival pud** (jediný): (1) otevření tlamy při hladu + přijatelné vůni, (2) při `hunger×smell` urgency přimíchání směru k jídlu. Pohyb, útok, enzym, signal, reprodukce = NN.

### 2.6 Jídlo

| Kind | Label | Charakter |
|------|-------|-----------|
| Green | zelené | vyvážené |
| Amber | zlaté | krátký smell, vysoká energie |
| Toxic | jedovaté | dlouhý smell, damage |

`FoodSpec` (per kind) je editovatelný v boot/settings. Bloom smell 0→1, fade při snězení.

### 2.7 Fields
Jeden kanál `Signal` (feromon). Deposit / sample / periodic diffuse.

### 2.8 Edges
4 strany misky, `EdgeEffect` + `reach`. Vizualizace tintem.

### 2.9 Multi-dish
`PetriDish` + `Tube` (side/along ports). Soft cap 8.

### 2.10 World tick (zjednodušeně)
1. Spatial hash  
2. Senses → brain → actuators  
3. Physics (thrust, drag, springs, dish clamp)  
4. Eat / bite / reproduce  
5. Field diffuse  
6. Food bloom/fade, feeders  
7. Sparks / flashes  

Rychlost: speed presets 1× … 100× (UI).

---

## 3. Persistace

**Desktop:** SQLite `{dirs::data_dir}/aether/aether.db`

**Schema `saves`:**
- `id`, `name` UNIQUE, timestamps  
- `sim_time`, `population`, `max_generation`  
- `payload` BLOB = bincode `WorldSnapshot`

**Web (`wasm32`):** stejné API nad pamětí stránky, zrcadlené do `localStorage` (`aether.saves.v1`, bincode + hlavička `AETHSAV1`). Obnovení stránky uložení zachová, dokud se vejde do kvóty prohlížeče; při selhání zápisu běží dál jen v paměti. SQLite ani `dirs` se na webu nelinkují.

API: `open_default`, `list_saves`, `save_simulation`, `load_simulation`, `delete_save`, `rename_save`.

---

## 4. Prezentace / UI

### 4.1 Lobby (Title)
- Fullscreen liquid water (`paint_lobby_backdrop` → celý window)  
- Cursor floor-light: zesvětlení teček + zprůhlednění zeleného gelu (shader)  
- Title roamers: volný pohyb po celé ploše (bez kolize s logem/menu)  
- Logo: Fira Sans Heavy, 3 skleněné vrstvy, organický float (L1 nejméně, L3 nejvíc)  
- DNA helix: 3D tilt (slabý), seamless sin hue travel, sync s barvou písmen  
- Diagonal color na písmenech; glass opacity ~10 %, blur ~34 %  
- Menu: Nová hra, Načíst, Nastavení, Ukončit  

### 4.2 Přechod Nová hra
- `transit_visual_new_game`: zoom + radial blur  
- `logo_burst`: písmena všech vrstev + DNA beads letí ke kameře (deterministické `burst_dir`)  
- Midpoint: spawn world + flip na Running  

### 4.3 Running chrome
- Side tools top-right (stacked): Jedinec, Krmítko, Nová kolonie
- Header bez rámečku a pozadí; přehled INFO trvale u misky
- Miska součástí tekutého pozadí; uvnitř bez mlhy a teček
- Inspect right panel (Info / Genom); open via Shift+click on organism
- Life setup modal for **Nová kolonie** (`seed_dish`)
- Camera: pan (incl. RMB drag), zoom, follow pinned organism
- Boot: splash + font atlas warm-up before Title
- Glow: one `begin_glow` batch per Running frame for soft blobs
- New-game: burst LOD + incremental population seed under veil 

### 4.4 Fonty (Linux paths)
- UI: `/usr/share/fonts/opentype/fira/FiraSans-Medium.otf`  
- Logo: `/usr/share/fonts/opentype/fira/FiraSans-Heavy.otf`  

### 4.5 Grafika (`gfx.rs`)
| Material | Účel |
|----------|------|
| water | liquid + cursor clear/dots |
| glow | soft_blob / soft_blob_cont |
| bloom | optional fullscreen bloom |

Fallback: CPU `draw_liquid_bg` pokud shadery selžou.

### 4.6 Audio
Procedurální WAV synth existuje, ale `LOAD_AUDIO = false` a `Mix.master = 0.0` — záměrně ticho do vytunění.

---

## 5. Boot / nastavení světa

`BootConfig`: population, start_food, dish half extents, food_kinds[3], viscosity, edges[4].  
Aplikuje se při nové hře a po loadu.

---

## 6. CLI

| Arg | Efekt |
|-----|--------|
| (none) | Title lobby |
| `--shot` | Running, seed 7, 16 organismů, 12 food |

---

## 7. Ne-cíle (aktuálně)
- Síťový multiplayer  
- Skriptované AI chování mimo NN  
- Mobilní build  

---

## 8. Testovací poznámky
- Manuální: lobby motion, new-game burst, eat/repro, save/load roundtrip  
- Unit testy: zatím minimální / žádné oficiální suite v crate  
