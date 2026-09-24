# AGENTS.md — Aether

Pokyny pro AI coding agenty pracující v tomto repozitáři.

## Povinná četba

1. [docs/OVERVIEW.md](docs/OVERVIEW.md) — co projekt je  
2. [docs/STRUCTURE.md](docs/STRUCTURE.md) — kde je kód  
3. [docs/SPEC.md](docs/SPEC.md) — specifikace chování  
4. [docs/MEMORY.md](docs/MEMORY.md) — **dlouhodobá paměť a design decisions**

Před změnou lobby/loga/DNA vždy znovu načti sekci Lobby v MEMORY.md.

## Pravidla práce

- Simulační logika patří do `src/` lib modulů; Macroquad jen v `view` / `gfx` / `audio` / `main`.  
- Konstanty balansu → `tune.rs`.  
- UI texty česky, konzistentní s existujícími labely.  
- Lobby liquid: fullscreen, ne dish letterbox.  
- Nepřidávej skriptované AI chování organismů.  
- Commity jen na výslovnou žádost uživatele.  
- Po substantivní změně designu aktualizuj `docs/MEMORY.md` (a SPEC pokud se mění kontrakt).

## Rychlé kotvy v kódu

| Oblast | Symbol / soubor |
|--------|-----------------|
| Main loop | `view::run` |
| Lobby water | `paint_lobby_backdrop`, `gfx::WATER_FRAG` |
| Logo | `draw_neon_logo`, `paint_logo_wordmark`, `draw_logo_dna` |
| New-game FX | `logo_burst`, `transit_visual_new_game` |
| World tick | `World` v `world.rs` |
| Saves | `store.rs` |

## Build

```bash
cargo check
cargo run
```
