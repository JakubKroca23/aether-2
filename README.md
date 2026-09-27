# Aether

Petri dish simulace organismů řízených **výhradně vlastními neuronovými sítěmi** (evoluční genom → feed-forward brain).

```bash
cargo run
```

Volitelně: `cargo run -- --shot` spustí rovnou běžící svět (bez lobby).

## V prohlížeči

Veřejná stránka: <https://jakubkroca23.github.io/aether-2/>

Lobby se otevře hned. Adresa s `?run=1` přeskočí lobby a rovnou spustí misku (obdoba `--shot`, ale bez ukládání screenshotu).

Lokálně ve prohlížeči:

```bash
rustup target add wasm32-unknown-unknown   # jednou
./scripts/serve-web.sh
```

Skript sestaví `wasm32-unknown-unknown` release, složí `web/dist` (`index.html`, `aether.wasm`, JS glue) a naservíruje ho na <http://127.0.0.1:8080/>. Jiný port: `./scripts/serve-web.sh 9000`.

Desktopové `cargo run` se nemění. Web verze nemá SQLite: uložené hry drží paměť stránky a `localStorage` (klíč `aether.saves.v1`). Audio feature je ve webu vypnuté — mix je stejně ztišený. Písmo Fira Sans je ve webu vložené (SIL OFL, `assets/fonts/OFL.txt`); nativní build dál bere systémové OTF.

## Dokumentace

| Dokument | Obsah |
|----------|--------|
| [docs/OVERVIEW.md](docs/OVERVIEW.md) | Přehled projektu, vize, stack |
| [docs/STRUCTURE.md](docs/STRUCTURE.md) | Struktura repozitáře a modulů |
| [docs/SPEC.md](docs/SPEC.md) | Specifikace (simulace, UI, grafika, data) |
| [docs/MEMORY.md](docs/MEMORY.md) | Dlouhodobá paměť / design decisions |
| [AGENTS.md](AGENTS.md) | Pokyny pro AI agenty |

UI je v češtině. Simulační jádro je knihovna `aether`; binárka je Macroquad loop ve `view`.
