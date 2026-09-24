# Aether

Petri dish simulace organismů řízených **výhradně vlastními neuronovými sítěmi** (evoluční genom → feed-forward brain).

```bash
cargo run
```

Volitelně: `cargo run -- --shot` spustí rovnou běžící svět (bez lobby).

## Dokumentace

| Dokument | Obsah |
|----------|--------|
| [docs/OVERVIEW.md](docs/OVERVIEW.md) | Přehled projektu, vize, stack |
| [docs/STRUCTURE.md](docs/STRUCTURE.md) | Struktura repozitáře a modulů |
| [docs/SPEC.md](docs/SPEC.md) | Specifikace (simulace, UI, grafika, data) |
| [docs/MEMORY.md](docs/MEMORY.md) | Dlouhodobá paměť / design decisions |
| [AGENTS.md](AGENTS.md) | Pokyny pro AI agenty |

UI je v češtině. Simulační jádro je knihovna `aether`; binárka je Macroquad loop ve `view`.
