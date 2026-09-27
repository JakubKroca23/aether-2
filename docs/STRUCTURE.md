# Struktura repozitáře

```
aether-2/
├── Cargo.toml / Cargo.lock
├── README.md
├── AGENTS.md                 # pokyny pro AI
├── docs/
│   ├── OVERVIEW.md
│   ├── STRUCTURE.md          # tento soubor
│   ├── SPEC.md
│   └── MEMORY.md
├── .cursor/rules/            # Cursor project rules
├── .cargo/
├── .github/workflows/pages.yml  # wasm release → GitHub Pages
├── .gitignore                # /target, shot.png, web/dist
├── assets/fonts/             # Fira Sans (OFL) vložené jen do wasm buildu
├── scripts/serve-web.sh      # lokální wasm build + statický server
├── web/                      # index.html, mq_js_bundle.js, aether_host.js
├── shot.png                  # (ignorováno) screenshot helper
├── src/
│   ├── main.rs               # bin entry, window conf
│   ├── lib.rs                # crate root (simulace)
│   ├── view.rs               # ~8k LOC — UI, lobby, render loop
│   ├── world.rs              # simulace světa
│   ├── organism.rs           # tělo + senses + actuators
│   ├── genome.rs             # geny, morph, mutace
│   ├── brain.rs              # NN + plasticita
│   ├── dish.rs               # PetriDish, Tube
│   ├── field.rs              # chemická mřížka
│   ├── spatial.rs            # spatial hash
│   ├── store.rs              # SQLite saves (web: paměť + localStorage)
│   ├── host.rs               # wasm: čas, ?run=1, localStorage importy
│   ├── tune.rs               # konstanty balansu
│   ├── math.rs               # Vec2, hue helpers
│   ├── gfx.rs                # GLSL materials
│   └── audio.rs              # procedurální audio hub
└── target/                   # build artifacts
```

## Oddělení crate vs bin

| Modul | Crate | Role |
|-------|-------|------|
| `lib.rs` + simulační moduly | `aether` lib | logika světa, serializace |
| `main.rs`, `view`, `gfx`, `audio` | bin `aether` | prezentace |

Lib **nezná** Macroquad. View importuje `aether::{World, …}`.

## Mapa modulů (lib)

### `tune.rs`
Jediné místo pro balanční konstanty (energie, drag, genom, limity populace/misek).

### `math.rs`
`Vec2`, `hue_similarity`, `gaussian`, `wrap_unit`.

### `genome.rs`
- `Gene` — 32bit packed connection  
- `Morph` — nodes, radius, mass, sensors/actions  
- `Genome` — genes + morph + hue / temperament  
- Mutace: bit-flip, délka genomu, node count  

### `brain.rs`
Feed-forward síť z genomu; prune mrtvých větví; eligibility / plasticita; `step(inputs) → outputs`.

### `organism.rs`
Tělo (nodes/vels), energie, senses layout, interpretace action neuronů → `Actuators`.

### `field.rs`
`Fields` — 128×128 signal grid, deposit / sample / diffuse.

### `spatial.rs`
Uniform grid pro near-neighbour (kolize, crowd, bite).

### `dish.rs`
`PetriDish` (lokální souřadnice), `Tube` (port-to-port mezi miskami).

### `world.rs`
Orchestruje tick: senses → brain → actuators → physics → eat/bite/repro → fields → FX.  
Public typy: `World`, `Food`, `FoodKind`, `FoodSpec`, `Feeder`, `EdgeZone`, `Stats`, `Appearance`, `Census`, snapshot pro save.

### `store.rs`
Desktop: SQLite v user data dir (`…/aether/aether.db`), tabulka `saves`, payload = bincode `WorldSnapshot`.  
Web: stejné funkce nad `Db` v paměti, persist do `localStorage` přes `host.rs`.

## Mapa modulů (bin)

### `main.rs`
Window 1280×800, title „Aether“, volá `view::run().await`.

### `view.rs` (největší soubor)
Fáze Title / Running, kamera, chrome UI, lobby logo/DNA, backdrop, inspect, saves panel, screen transit.

### `gfx.rs`
Materiály: water (cursor light + green thinning), soft glow blobs, bloom post.

### `audio.rs`
`AudioHub`, SFX enum, mix; `LOAD_AUDIO = false` a `master: 0.0` (audio dočasně vypnuté).

## Kde co hledat

| Chci změnit… | Soubor |
|--------------|--------|
| Balanc energie / mutace | `tune.rs` |
| Senzory / akce NN | `genome.rs`, `organism.rs` |
| Učení vah | `brain.rs` |
| Tick simulace | `world.rs` |
| Lobby / logo / přechody | `view.rs` (`paint_logo_*`, `logo_burst`, …) |
| Liquid shader | `gfx.rs` (`WATER_FRAG`) |
| Save formát | `store.rs`, `WorldSnapshot` |
| Více misek | `dish.rs`, `world.rs` |

## Velikosti (orientačně)

| Soubor | ~LOC |
|--------|------|
| `view.rs` | 8000 |
| `world.rs` | 2300 |
| `organism.rs` | 550 |
| `brain.rs` | 460 |
| ostatní | &lt; 400 |
