# Dlouhodobá paměť projektu (Aether)

Živý dokument: rozhodnutí, konvence a kontext, které mají přežít jednotlivé chaty.  
**Aktualizuj při větších změnách.** Poslední sync: 2026-09-26.

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
4. **Jediný hardcoded pud = přežití** — tlama + urgency k jídlu při hladu; pohyb/útok/signal/repro řídí NN.  
5. **Dish-local souřadnice** pro těla/jídlo; table transform přes `PetriDish::{to_table,to_local}`.  
6. **Kill-forward ≠ food mouth** — útok je záměrný actuator, ne kolize pusy s jídlem.  
7. **Save = bincode snapshot + SQLite meta** — payload opaque blob. Na webu stejný payload, ale bez SQLite: paměť stránky + `localStorage`.  
8. **Macroquad font rendering** — pro dynamické škálování (zoom kamery, burst animace loga) vždy používat fixní základní `font_size` a měřítko předávat přes `font_scale` (příp. `font_scale_aspect`). Dynamická změna `font_size` každým snímkem nutí CPU rastrovat glyfy do atlasu a způsobuje těžké propady FPS.

---

## Lobby / branding (stav po iteracích 2026-09)

### Pozadí
- Liquid musí jít **edge-to-edge** (`draw_water(0,0,sw,sh)`), ne přes `dish_fit_rect` (ten nechává černé okraje).  
- Cursor light: (a) zesvětlí floor dots, (b) **ztenčí zelený gel** (mix do clear water + ubrání G).  
- Rim shaderu: jemný teal, **ne** mix do near-black.

### Boot splash
- Při startu (ne `--shot`): horizontální **skleněná DNA dvojšroubovice** (průhledný rim) se postupně **zalévá barvou** zleva doprava podle progress loadu; pod ní „AETHER“; fade do Title. Mid-game loading screeny nepoužívat.

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
- **Burst LOD**: při new-game shatter méně glass vrstev / blur offsets / DNA depth passů; seed populace po dávkách pod veilem.

### Přechod Nová hra
- Zoom-in + postupný radial blur.  
- Burst: všechna písmena všech vrstev + DNA beads letí **ke kameře** v náhodných (hashovaných) směrech.  
- `logo_compose` park DNA zůstává pro **návrat na title**; pro new game používat `logo_burst`.

---

## Running / table

- Table floor = stejný liquid jako lobby, ale **world-space** s kamerou.  
- Vnitřek misky má dno z **tmavého skla**, které propouští jen minimum ambientního pozadí (91% krytí tmavého laboratorního skla), díky čemuž organismy a senzory výrazně vizuálně vystupují.  
- Ohraničení misky (skleněný lem) funguje jako fyzická bariéra: **uvnitř misky není ani zelená mlha, ani tečky/bubliny** (ty zůstávají na stole vně misek).  
- Přehled **EKOSYSTÉM** (census) je trvale zobrazen vpravo vedle misky, zvětšený o 50 %, bez rámečku a pozadí (čistá plovoucí typografie). Je pevně svázán s miskou (pohybuje se 1:1 s ní bez odskakování u horního okraje) a rozestup mezi názvy a hodnotami je čistě relativní vůči měřítku světa. Čas simulace byl přesunut z hlavičky přímo vedle nadpisu EKOSYSTÉM.  
- Hlavička misky: border a pozadí odstraněny; tlačítka (nastavení misky, pauza, rychlost, uložení) jsou vycentrovaná nad středem misky, zvětšená a s většími mezerami mezi sebou.  
- **Nástroje misky**: sloupec vpravo nahoře — průhledné ikony (*Jedinec* / *Krmítko* / *Kolonie*). Jedinec jen spawne (bez auto-inspect). Nová kolonie → modal → `seed_dish`. Hover krmítka: ZAP/−/+/ozubené (otevře nastavení). LMB na organismus = detail; **Shift+LMB** = mutace.  
- Chrome: flat dark panels (`CHROME_FILL` / `CHROME_EDGE`) pro modály, inspect: Info + Genom taby.
- **Glow batch**: v Running jeden `begin_glow`/`end_glow` pro food + organismy + FX (`soft_blob_cont`).
- Inspect: cache `Net` topologie + refresh aktivací; tenčí synapse křivky.

---

## Web

- Cíl `wasm32-unknown-unknown`, loader Macroquad (`web/mq_js_bundle.js`, gl.js version 2) + `web/aether_host.js`.
- Nativní `cargo run` zůstává. Web audio feature je vypnuté (mix je stejně `master = 0`); placeholder WAV by na webu mohly boot zaseknout, když `decodeAudioData` selže.
- `getrandom` na webu má feature `custom` (bez wasm-bindgen), jinak miniquad loader modul nenačte.
- `SystemTime::now` se na webu nevolá (panika). Čas jde z `Date.now()` (`aether_unix_ms`).
- Fonty: nativně dál systémové Fira OTF. Web vkládá `assets/fonts` (SIL OFL), protože v prohlížeči systémové cesty nejsou a výchozí font Macroquadu neumí češtinu.
- `?run=1` přeskočí lobby (seedovaná miska). Desktop `--shot` pořád navíc přetočí na t=18 a uloží `shot.png`.
- Nasazení: `.github/workflows/pages.yml` → GitHub Pages source **GitHub Actions**. URL: `https://jakubkroca23.github.io/aether-2/`.

## Audio stav

- `LOAD_AUDIO = false` — startup nesmí syntetizovat dlouhé WAV (dříve freeze ~sekundy).  
- `Mix.master = 0.0` — záměrné ticho, dokud se mix nevytuneruje.  
- Kód SFX/music hub zůstává připravený (`Sfx` enum, `AudioHub`).

---

## Výkonnostní konvence (view/gfx)

- Preferovat **jeden** `begin_glow` / `end_glow` batch (Running: food+orgs+FX v jednom bindu).  
- Neonemožit multi-pass redraw celé title colony „kvůli bluru“ (historicky hitchovalo).  
- Burst radial blur: omezený počet ghostů; hlavní efekt = geometrie flight; během burst LOD méně passů.  
- Boot: warm-up běžných `font_size` do atlasu před lobby.  
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
| Maximalizovaná miska | Stůl smazán; miska je fullscreen (okraje misky = okraje okna); ekosystém vlevo nahoře, ovládací prvky nahoře uprostřed; 2 nástroje dole |
| Přepracování krmítek | Volné umisťování kamkoliv do misky kliknutím; výchozí stav VYPNUTO; konfigurační popup přímo vedle krmítka (typ potravy, vlastnosti potravy, množství/s, disperzní rádius zobrazený kruhem, zapnutí/vypnutí, smazání) |
| UI rozvržení a census | Census bez pozadí, menší text (15/11 px), hodnoty posunuté vlevo; tlačítka Nastavení a Uložit vpravo nahoře; čas, pauza a rychlost sloučeny do středového mini panelu |
| Čisté ikony & hover | Horní střed čistě plovoucí (čas 20 px + dělítko + 2 tlačítka bez rámečku/panelu a bez kruhů); ikony vpravo bez kruhu; hover všech ikon bez kruhů na pozadí, pouze outline vytažení ikony kontrastní barvou |
| Minimalistický header & census | Tlačítko „Nový život“ v misce odstraněno; přehled EKOSYSTÉM sbalitelný klikem do nadpisu (šipka ▸ / ▾); tlačítka nastavení a uložení vpravo nahoře jako text (NASTAVENÍ, ULOŽIT); pauza přesunuta vlevo od času simulace, vertikální dělítko odstraněno |
| Odlehčené pozadí & submenu | Fragmentový shader vody odlehčen od drahého 3-oktávového šumu na rychlé analytické proudění pro plynulý běh; odstraněna šipka u nadpisu EKOSYSTÉM (zůstal čistý text); submenu Nastavení zcela bez rámečku/pozadí s bílou typografií a sbalitelnými sekcemi |
| Horní lišta & zjednodušené nastavení | EKOSYSTÉM přejmenován na INFO; tlačítka ULOŽIT a NASTAVENÍ přesunuta doleva vedle INFO; v nastavení ponecháno pouze Prostředí (odpor a okraje) s čistým nadpisem bez šipky |
| Modrofialová miska & kolečko krmítka | Pozadí v misce přebarveno do tmavé modrofialové / indigové palety; při aktivním umisťování krmítka lze kolečkem myši plynule měnit oblast rozptylu s okamžitým ghost preview a bez zoomování kamery |
| Vzhled krmítek & mutace kliknutím | Krmítka mají zaoblený čtvercový uzel s ikonou potravy a textovým labelem; spodní tlačítko nese ikonu potravy, badge s počtem aktivních krmítek a hover zvýrazní všechna krmítka; kliknutí na organismus vyvolá novou mutaci bez otevření okna detailu |
| Rychlé hover ovládání & badge krmítek | Popisek pod krmítkem odstraněn (čistá plocha); při hoveru nad krmítkem se zobrazí vypínač ZAP/VYP a tlačítka −/+ pro rychlou změnu intervalu dávkování; spodní badge zobrazuje celkový počet krmítek bez ohledu na zapnutí |
| Hitch fix & side tools | Boot splash + font warm-up; Running glow batch; burst LOD + incremental seed; nástroje vpravo nahoře (Jedinec bez pin / Krmítko / Nová kolonie); Net cache; detail = Shift+klik |
| Survival pud vs NN | Jediný hardcoded pud = přežití druhu: tlama při hladu+vůni a urgency blend směru k jídlu. Chemotaxe/foraging jinak NN přes senzory `jídlo vpřed/stranou`. Bite 0.12, lehčí metabolismus, víc starter jídla (většinou zelené). |
| Web build | wasm32 + mq_js_bundle; SQLite jen desktop; web save = paměť + localStorage; audio feature na webu off; Fira embedded jen ve wasm; Pages workflow na `main` |

---

## Jak aktualizovat tuto paměť

1. Po větší feature: doplň řádek do tabulky rozhodnutí.  
2. Po změně invariantu: uprav sekci Architektura / Lobby.  
3. Po změně API: zkontroluj [SPEC.md](SPEC.md).  
4. AI agenti: čti `AGENTS.md` + tento soubor na začátku session.  
