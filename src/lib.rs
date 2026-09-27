mod brain;
mod dish;
mod field;
mod genome;
#[cfg(target_arch = "wasm32")]
mod host;
mod math;
mod organism;
mod spatial;
mod store;
mod tune;
mod world;

pub use dish::{PetriDish, Tube};
#[cfg(target_arch = "wasm32")]
pub use host::{auto_run as web_auto_run, unix_millis};
pub use math::Vec2;
pub use store::{
    delete_save, list_saves, load_simulation, open_default, rename_save, save_simulation, SaveMeta,
    StoreError,
};
pub use world::{
    Appearance, Census, EdgeEffect, EdgeZone, Feeder, Flash, Food, FoodKind, FoodSpec, Net, Spark,
    Stats, World, WorldSnapshot,
};
