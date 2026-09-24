# Dlouhodobá paměť projektu (Aether)

Živý dokument: rozhodnutí, konvence a kontext, které mají přežít jednotlivé chaty.  
**Aktualizuj při větších změnách.** Poslední sync: 2026-09-24.

---

## Identita produktu

- Název: **Aether**  
- Tagline crate: *Petri dish of organisms controlled only by their own neural networks*  
- Estetika: organická sci-fi laboratoř — teal/cyan gel, měkké glow, skleněné UI  
- UI jazyk: **čeština**  
- Hráč je pozorovatel / experimentátor, ne „ovladač postavy“

---

## Architektonické invarianty

1. **Simulace ≠ view** — lib nemá Macroquad; view jen prezentuje.  
2. **Balanc patří do `tune.rs`** — nehardcodit magické konstanty do world/organism bez důvodu.  
3. **Genom je zdroj pravdy** pro topologii mozku; brain se z něj skládá a prunuje.  
4. **Dish-local souřadnice** pro těla/jídlo; table transform přes `PetriDish::{to_table,to_local}`.  
5. **Kill-forward ≠ food mouth** — útok je záměrný actuator, ne kolize pusy s jídlem.  
6. **Save = bincode snapshot + SQLite meta** — payload opaque blob.

---

## Lobby / branding (stav po iteracích 2026-09)

### Pozadí
- Liquid musí jít **edge-to-edge** (`draw_water(0,0,sw,sh)`), ne přes `dish_fit_rect` (ten nechává černé okraje).  
- Cursor light: (a) zesvětlí floor dots, (b) **ztenčí zelený gel** (mix do clear water + ubrání G).  
- Rim shaderu: jemný teal, **ne** mix do near-black.

### Roameři
- Volný pohyb po celé ploše včetně za logem/menu.  
- Žádná repulsion od logo/menu plate, žádné side lanes — jen bounce na okraji okna.

### Logo
- Font: **Fira Sans Heavy**, horizontálně roztažený (`font_scale_aspect` ~1.38).  
- 3 skleněné vrstvy: opacity ~10 %, blur ~34 %, organický float; **L1 (front) nejméně, L3 (back) nejvíc**.  
- Písmena plavou vůči sobě (nezávislé fáze).  
- DNA: slabý 3D tilt kurzorem (ne planar slide).  
- Hue: **seamless sin wave** podél helixu i písmen (žádný hard wrap 1→0).  
- Barva písmen: diagonála L→R + top→bottom, sync s DNA.  
- Odlesky: permanentní tint z helix barvy; **bez** myších kuliček/caustic blobs.

### Přechod Nová hra
- Zoom-in + postupný radial blur.  
- Burst: všechna písmena všech vrstev + DNA beads letí **ke kameře** v náhodných (hashovaných) směrech.  
- `logo_compose` park DNA zůstává pro **návrat na title**; pro new game používat `logo_burst`.

---

## Running / table

- Table floor = stejný liquid jako lobby, ale **world-space** s kamerou.  
- Miska je **vizuálně součástí pozadí** (tekutina a kaustika prochází vnitřkem misky).  
- Ohraničení misky (skleněný lem) funguje jako fyzická bariéra: **uvnitř misky není ani zelená mlha, ani tečky/bubliny** (ty zůstávají na stole vně misek).  
- Přehled **EKOSYSTÉM** (census) je trvale zobrazen vpravo vedle misky, zvětšený o 50 %, bez rámečku a pozadí (čistá plovoucí typografie). Je pevně svázán s miskou (pohybuje se 1:1 s ní bez odskakování u horního okraje) a rozestup mezi názvy a hodnotami je čistě relativní vůči měřítku světa, takže se při zoomování nemění. Tlačítko `i` bylo zrušeno.  
- Hlavička misky: border a pozadí odstraněny (ikony a čas plují volně nad miskou). Vlevo v hlavičce je tlačítko pro nastavení misky (ikona ozubeného kola).  
- Chrome: flat dark panels (`CHROME_FILL` / `CHROME_EDGE`) pro modály, inspect: Info + Genom taby.

---

## Audio stav

- `LOAD_AUDIO = false` — startup nesmí syntetizovat dlouhé WAV (dříve freeze ~sekundy).  
- `Mix.master = 0.0` — záměrné ticho, dokud se mix nevytuneruje.  
- Kód SFX/music hub zůstává připravený (`Sfx` enum, `AudioHub`).

---

## Výkonnostní konvence (view/gfx)

- Preferovat **jeden** `begin_glow` / `end_glow` batch.  
- Neonemožit multi-pass redraw celé title colony „kvůli bluru“ (historicky hitchovalo).  
- Burst radial blur: omezený počet ghostů; hlavní efekt = geometrie flight.  
- Dev `opt-level = 1` je záměr.

---

## Fonty a assets

- Závislost na systémových Fira OTF cestách (Linux Pop!_OS / Debian layout).  
- Žádný `assets/` folder v repu; screenshot `shot.png` je v `.gitignore`.

---

## Co záměrně nedělat

- Nepřidávat skriptované FSM chování organismů „aby vypadaly chytře“.  
- Nerozbíjet lib Macroquadem.  
- Necommittovat `target/`, secrets, uživatelskou DB.  
- Neamendovat historii bez explicitní žádosti uživatele.

---

## Otevřené / budoucí směry (neblokující)

- [ ] Zapnout a vyvážit audio  
- [ ] Unit testy pro genome packing / brain prune / store roundtrip  
- [ ] Portable font loading (embed nebo relative path)  
- [ ] Refaktor `view.rs` na menší moduly (lobby / chrome / inspect)  
- [ ] Více field kanálů než jen Signal  
- [ ] Tutorial / first-run hints  

---

## Historie klíčových chat rozhodnutí

| Téma | Rozhodnutí |
|------|------------|
| Lobby roamers | Full-area free roam, no UI collision |
| DNA + cursor | Rotate in 3D, don't translate; soft intensity |
| Logo glass | 90% transparent, 34% blur, Heavy stretched font |
| Hue cycle | Sin seamless, not rem_euclid wrap |
| Letter motion | Per-glyph float; front least / back most |
| New game FX | Zoom + radial blur + shatter toward camera |
| Lobby water | Fullscreen; cursor clears green gel too |
| Dish & mlha | Miska je součástí pozadí; ohraničení nepustí okolní zelenou mlhu dovnitř |
| Vnitřek misky | Uvnitř misky není mlha ani tečky (čistá tekutina); vně zůstávají |
| EKOSYSTÉM & header | Trvalý census vpravo od misky (+50 %, bez karty); hlavička bez boxu, vlevo nastavení |

---

## Jak aktualizovat tuto paměť

1. Po větší feature: doplň řádek do tabulky rozhodnutí.  
2. Po změně invariantu: uprav sekci Architektura / Lobby.  
3. Po změně API: zkontroluj [SPEC.md](SPEC.md).  
4. AI agenti: čti `AGENTS.md` + tento soubor na začátku session.  
