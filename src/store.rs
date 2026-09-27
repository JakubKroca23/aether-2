//! Named simulation saves.
//!
//! Native builds keep the SQLite file in the user data directory.
//! The web build keeps the same records in memory and mirrors them to
//! `localStorage` when the page host provides it.

use crate::world::{World, WorldSnapshot};

#[cfg(not(target_arch = "wasm32"))]
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS saves (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sim_time REAL NOT NULL,
    population INTEGER NOT NULL,
    max_generation INTEGER NOT NULL,
    payload BLOB NOT NULL
);
";

#[derive(Clone, Debug)]
pub struct SaveMeta {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub sim_time: f32,
    pub population: i64,
    pub max_generation: i64,
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    Db(String),
    Codec(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) | Self::Db(s) | Self::Codec(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<bincode::Error> for StoreError {
    fn from(e: bincode::Error) -> Self {
        Self::Codec(e.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Open save database. Native: SQLite file. Web: memory + localStorage.
pub struct Db {
    #[cfg(not(target_arch = "wasm32"))]
    conn: rusqlite::Connection,
    #[cfg(target_arch = "wasm32")]
    inner: std::cell::RefCell<MemDb>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn default_db_path() -> StoreResult<std::path::PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| StoreError::Io("could not resolve user data directory".into()))?;
    Ok(base.join("aether").join("aether.db"))
}

pub fn open_default() -> StoreResult<Db> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        open_path(&default_db_path()?)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(Db {
            inner: std::cell::RefCell::new(MemDb::load()),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(path: &std::path::Path) -> StoreResult<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(Db { conn })
}

fn unix_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::host::unix_millis() / 1000
    }
}

fn now_iso() -> String {
    let secs = unix_secs();
    // Compact UTC-ish stamp without chrono dependency: YYYY-MM-DD HH:MM:SS approx from unix.
    let days = secs / 86400;
    let tod = secs % 86400;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    // Civil date from days since 1970-01-01 (Howard Hinnant algorithm).
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

pub fn encode_snapshot(snap: &WorldSnapshot) -> StoreResult<Vec<u8>> {
    Ok(bincode::serialize(snap)?)
}

pub fn decode_snapshot(bytes: &[u8]) -> StoreResult<WorldSnapshot> {
    Ok(bincode::deserialize(bytes)?)
}

fn world_from_payload(payload: &[u8]) -> StoreResult<World> {
    let snap = decode_snapshot(payload)?;
    if snap.version != 2 {
        return Err(StoreError::Codec(format!(
            "nepodporovaná verze snapshotu {}",
            snap.version
        )));
    }
    Ok(World::from_snapshot(snap))
}

/// Insert or replace a save by name. Returns the row id.
pub fn save_simulation(db: &Db, name: &str, world: &World) -> StoreResult<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Io("název nesmí být prázdný".into()));
    }
    let snap = world.to_snapshot();
    let payload = encode_snapshot(&snap)?;
    let census = world.census();
    let stamp = now_iso();

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rusqlite::{params, OptionalExtension};
        let existing: Option<i64> = db
            .conn
            .query_row(
                "SELECT id FROM saves WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            db.conn.execute(
                "UPDATE saves SET updated_at = ?1, sim_time = ?2, population = ?3,
             max_generation = ?4, payload = ?5 WHERE id = ?6",
                params![
                    stamp,
                    census.time as f64,
                    census.alive as i64,
                    census.max_generation as i64,
                    payload,
                    id
                ],
            )?;
            Ok(id)
        } else {
            db.conn.execute(
                "INSERT INTO saves (name, created_at, updated_at, sim_time, population, max_generation, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    name,
                    stamp,
                    stamp,
                    census.time as f64,
                    census.alive as i64,
                    census.max_generation as i64,
                    payload
                ],
            )?;
            Ok(db.conn.last_insert_rowid())
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let id = {
            let mut mem = db.inner.borrow_mut();
            if let Some(row) = mem.rows.iter_mut().find(|row| row.name == name) {
                row.updated_at = stamp;
                row.sim_time = census.time;
                row.population = census.alive as i64;
                row.max_generation = census.max_generation as i64;
                row.payload = payload;
                row.id
            } else {
                let id = mem.next_id;
                mem.next_id += 1;
                mem.rows.insert(
                    0,
                    MemSave {
                        id,
                        name: name.to_string(),
                        created_at: stamp.clone(),
                        updated_at: stamp,
                        sim_time: census.time,
                        population: census.alive as i64,
                        max_generation: census.max_generation as i64,
                        payload,
                    },
                );
                id
            }
        };
        db.persist();
        Ok(id)
    }
}

pub fn list_saves(db: &Db) -> StoreResult<Vec<SaveMeta>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut stmt = db.conn.prepare(
            "SELECT id, name, created_at, updated_at, sim_time, population, max_generation
         FROM saves ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SaveMeta {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                sim_time: row.get::<_, f64>(4)? as f32,
                population: row.get(5)?,
                max_generation: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let mem = db.inner.borrow();
        let mut rows: Vec<SaveMeta> = mem.rows.iter().map(MemSave::meta).collect();
        rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
        Ok(rows)
    }
}

pub fn load_simulation(db: &Db, id: i64) -> StoreResult<World> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rusqlite::params;
        let payload: Vec<u8> = db.conn.query_row(
            "SELECT payload FROM saves WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        world_from_payload(&payload)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let mem = db.inner.borrow();
        let row = mem
            .rows
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| StoreError::Db("uložení nenalezeno".into()))?;
        world_from_payload(&row.payload)
    }
}

pub fn delete_save(db: &Db, id: i64) -> StoreResult<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rusqlite::params;
        let n = db
            .conn
            .execute("DELETE FROM saves WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::Db("uložení nenalezeno".into()));
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    {
        let removed = {
            let mut mem = db.inner.borrow_mut();
            let before = mem.rows.len();
            mem.rows.retain(|row| row.id != id);
            before != mem.rows.len()
        };
        if !removed {
            return Err(StoreError::Db("uložení nenalezeno".into()));
        }
        db.persist();
        Ok(())
    }
}

pub fn rename_save(db: &Db, id: i64, name: &str) -> StoreResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Io("název nesmí být prázdný".into()));
    }
    let stamp = now_iso();

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rusqlite::params;
        let n = db.conn.execute(
            "UPDATE saves SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, stamp, id],
        )?;
        if n == 0 {
            return Err(StoreError::Db("uložení nenalezeno".into()));
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    {
        let found = {
            let mut mem = db.inner.borrow_mut();
            if mem.rows.iter().any(|row| row.id != id && row.name == name) {
                return Err(StoreError::Db("název už existuje".into()));
            }
            if let Some(row) = mem.rows.iter_mut().find(|row| row.id == id) {
                row.name = name.to_string();
                row.updated_at = stamp;
                true
            } else {
                false
            }
        };
        if !found {
            return Err(StoreError::Db("uložení nenalezeno".into()));
        }
        db.persist();
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
const WEB_MAGIC: &[u8] = b"AETHSAV1";

#[cfg(target_arch = "wasm32")]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct MemSave {
    id: i64,
    name: String,
    created_at: String,
    updated_at: String,
    sim_time: f32,
    population: i64,
    max_generation: i64,
    payload: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
impl MemSave {
    fn meta(&self) -> SaveMeta {
        SaveMeta {
            id: self.id,
            name: self.name.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            sim_time: self.sim_time,
            population: self.population,
            max_generation: self.max_generation,
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct MemDb {
    next_id: i64,
    rows: Vec<MemSave>,
}

#[cfg(target_arch = "wasm32")]
impl MemDb {
    fn load() -> Self {
        let Some(bytes) = crate::host::read_blob() else {
            return Self::empty();
        };
        if bytes.len() < WEB_MAGIC.len() || &bytes[..WEB_MAGIC.len()] != WEB_MAGIC {
            return Self::empty();
        }
        match bincode::deserialize::<Vec<MemSave>>(&bytes[WEB_MAGIC.len()..]) {
            Ok(rows) => {
                let next_id = rows.iter().map(|row| row.id).max().unwrap_or(0) + 1;
                Self { next_id, rows }
            }
            Err(err) => {
                eprintln!("aether: web saves ignored ({err})");
                Self::empty()
            }
        }
    }

    fn empty() -> Self {
        Self {
            next_id: 1,
            rows: Vec::new(),
        }
    }

    fn encode(&self) -> StoreResult<Vec<u8>> {
        let mut out = WEB_MAGIC.to_vec();
        out.extend(bincode::serialize(&self.rows)?);
        Ok(out)
    }
}

#[cfg(target_arch = "wasm32")]
impl Db {
    fn persist(&self) {
        let encoded = {
            let mem = self.inner.borrow();
            match mem.encode() {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("aether: web save encode failed ({err})");
                    return;
                }
            }
        };
        if !crate::host::write_blob(&encoded) {
            eprintln!("aether: web save was kept in memory only");
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::math::Vec2;
    use crate::world::FoodKind;

    #[test]
    fn round_trip_snapshot_through_sqlite() {
        let dir = std::env::temp_dir().join(format!(
            "aether-store-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let db = open_path(&path).unwrap();

        let mut world = World::new_with(42, 8, 4);
        world.drop_food_kind(Vec2::new(0.1, -0.2), FoodKind::Amber);
        // Advance a bit so brains/state diverge from birth defaults.
        for _ in 0..30 {
            world.step(1.0 / 60.0);
        }
        let before = world.to_snapshot();
        let energy0 = before.organisms[0].energy;
        let w0 = before.organisms[0].brain.first_weight();

        let id = save_simulation(&db, "test-run", &world).unwrap();
        let loaded = load_simulation(&db, id).unwrap();
        let after = loaded.to_snapshot();

        assert_eq!(after.organisms.len(), before.organisms.len());
        assert_eq!(after.dishes.len(), before.dishes.len());
        assert_eq!(
            after.dishes.iter().map(|d| d.foods.len()).sum::<usize>(),
            before.dishes.iter().map(|d| d.foods.len()).sum::<usize>()
        );
        assert!((after.time - before.time).abs() < 1e-4);
        assert!((after.organisms[0].energy - energy0).abs() < 1e-5);
        if before.organisms[0].brain.has_weights() {
            assert!((after.organisms[0].brain.first_weight() - w0).abs() < 1e-6);
        }

        let list = list_saves(&db).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-run");

        // Overwrite same name.
        save_simulation(&db, "test-run", &loaded).unwrap();
        assert_eq!(list_saves(&db).unwrap().len(), 1);

        delete_save(&db, id).unwrap();
        assert!(list_saves(&db).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
