# Aether — přehled projektu

**Stav:** aktivní vývoj (v0.1.0)  
**Žánr:** evoluční / life-sim sandbox v Petri misce  
**Platforma:** desktop (Macroquad / OpenGL) a web (`wasm32-unknown-unknown`, GitHub Pages)

## Co to je

Aether je 2D laboratoř: na stole leží jedna nebo více Petri misek s organismy. Každý organismus má:

1. **Genom** — binární geny (32bit connection genes) + morfologie  
2. **Mozek** — feed-forward síť odvozená z genomu (s 1-step pamětí vnitřních neuronů)  
3. **Tělo** — řetěz segmentů (spring-mass), senzory, aktuátory  

Chování není skriptované: pohyb, krmení, signalizace, útok i rozmnožování vychází z výstupů sítě a plasticity.

## Vrstvy systému

```
┌─────────────────────────────────────────┐
│  view (Macroquad UI + camera + lobby)   │
│  gfx (GLSL water / glow / bloom)        │
│  audio (procedurální SFX — zatím off)   │
├─────────────────────────────────────────┤
│  world (simulace, jídlo, edge zones)    │
│  dish / field / spatial / organism      │
│  genome / brain / tune / math / store   │
└─────────────────────────────────────────┘
```

- **Knihovna** (`src/lib.rs`) — čistá simulace + save/load  
- **Binárka** (`src/main.rs` → `view::run`) — okno, lobby, runtime UI  

## Herní fáze

| Fáze | Popis |
|------|--------|
| **Title (lobby)** | Logo AETHER (sklo + DNA), menu, fullscreen liquid pozadí, roamující siluety |
| **Running** | Lab table s tekutinou, dish cutout, organismy, nástroje, inspect panel |

Přechod *Nová hra*: zoom-in + radial blur + rozpad písmen/DNA směrem ke kameře.

## Klíčové herní objekty

- **Organism** — genom, brain, nodes/vels, energie, damage, actuators  
- **Food** — Green / Amber / Toxic (různý smell + energie / toxicita)  
- **Feeder** — automatický krmič v misce  
- **Fields** — feromonová mřížka (signal)  
- **Edges** — okrajové zóny (reflect / kill / …)  
- **Tube** — propojení misek (více dishes na stole)  

## Tech stack

| Část | Volba |
|------|--------|
| Jazyk | Rust 2021 |
| Render / window | Macroquad 0.4 (+ audio feature) |
| Matematika | vlastní `Vec2` (+ glam přes macroquad) |
| Persist | SQLite (`rusqlite` bundled) + `bincode`; web: paměť + `localStorage` |
| Shadery | GLSL 100 (water, soft glow, bloom) |

## Spuštění

```bash
cargo run                 # lobby
cargo run -- --shot       # rovnou simulace (seed 7)
./scripts/serve-web.sh    # totéž v prohlížeči (release wasm)
```

Veřejně: <https://jakubkroca23.github.io/aether-2/> (`?run=1` přeskočí lobby).

Dev profil: `opt-level = 1` (rychlejší simulace při debug buildu).

## Kam dál číst

- Struktura kódu → [STRUCTURE.md](STRUCTURE.md)  
- Detaily chování a API → [SPEC.md](SPEC.md)  
- Design decisions a konvence → [MEMORY.md](MEMORY.md)  
