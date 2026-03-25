//! `store.rs` — ParamStore: live parameter values with smoothing.
//!
//! # Update model (per frame)
//!
//! ```text
//! External input (MIDI/OSC/keyboard) → set_target(name, value)
//!                                              │
//!                                       targets HashMap
//!                                              │
//!                                    step_frame() each frame
//!                                              │
//!                              values = lerp(values, targets, 1.0 - smooth)
//!                                              │
//!                                    read_value(name) → f32
//! ```
//!
//! # Thread safety
//!
//! `set_target` and `set_target_by_midi_cc` take `&mut self` — wrap in
//! `Arc<Mutex<ParamStore>>` for multi-threaded use (MIDI on its own thread,
//! render loop on main thread).
//!
//! # "Latest value wins" semantics
//!
//! Multiple sources can write the same parameter. The last write before
//! `step_frame()` is the one that takes effect — no queuing, no merging.
//! This matches shadecore's model exactly.

use std::collections::HashMap;
use crate::{ParamError, ParamSchema};

/// Live parameter state for one instrument.
///
/// Owns both the schema (for lookup and MIDI mapping) and the live values.
pub struct ParamStore {
    schema:      ParamSchema,
    /// Raw target values — set by MIDI/OSC/keyboard.
    targets:     HashMap<String, f32>,
    /// Smoothed display values — step toward targets each frame.
    values:      HashMap<String, f32>,
    /// MIDI CC → param name index (built once from schema).
    midi_index:  HashMap<u8, String>,
    /// OSC address → param name index.
    osc_index:   HashMap<String, String>,
}

impl ParamStore {
    /// Create a new store from a schema.
    /// Seeds all targets and values to their `default` from the schema.
    pub fn new(schema: ParamSchema) -> Self {
        let mut targets = HashMap::new();
        let mut values  = HashMap::new();
        for p in &schema.params {
            targets.insert(p.name.clone(), p.default);
            values.insert(p.name.clone(), p.default);
        }
        let midi_index = schema.midi_cc_index();
        let osc_index  = schema.osc_addr_index();
        Self { schema, targets, values, midi_index, osc_index }
    }

    /// Load schema from a params.json file and create a store.
    pub fn from_json_file(path: &str) -> Result<Self, ParamError> {
        Ok(Self::new(ParamSchema::load(path)?))
    }

    /// Create an empty store with no params (useful for testing).
    pub fn empty() -> Self {
        Self::new(ParamSchema::empty())
    }

    // ── Write API (called by MIDI/OSC/keyboard threads) ───────────────────

    /// Set a parameter target by name. Value is clamped to [min, max].
    ///
    /// Call from any thread that can take `&mut self` (wrap in Mutex for
    /// multi-threaded access).
    pub fn set_by_name(&mut self, name: &str, value: f32) -> Result<(), ParamError> {
        let clamped = if let Some(def) = self.schema.get(name) {
            def.clamp(value)
        } else {
            return Err(ParamError::UnknownParam(name.into()));
        };
        self.targets.insert(name.to_owned(), clamped);
        Ok(())
    }

    /// Set a parameter target by MIDI CC number and raw value [0, 127].
    /// Maps CC value through the param's [min, max] range.
    pub fn set_by_midi_cc(&mut self, cc: u8, raw: u8) -> Result<(), ParamError> {
        let name = self.midi_index.get(&cc)
            .ok_or(ParamError::UnknownMidiCc(cc))?.clone();
        let def = self.schema.get(&name)
            .ok_or_else(|| ParamError::UnknownParam(name.clone()))?;
        let value = def.map_midi(raw);
        self.targets.insert(name, value);
        Ok(())
    }

    /// Set a parameter target by OSC address and pre-scaled f32 value.
    ///
    /// The value is expected to already be in the param's natural range.
    /// If you receive a normalized [0,1] OSC value, scale it before calling.
    pub fn set_by_osc_addr(&mut self, addr: &str, value: f32) -> Result<(), ParamError> {
        let name = self.osc_index.get(addr)
            .ok_or_else(|| ParamError::UnknownParam(addr.into()))?.clone();
        self.set_by_name(&name, value)
    }

    /// Force-set the smoothed value directly (bypasses targets).
    /// Used for preset loading where you want instant snaps.
    pub fn snap_to(&mut self, name: &str, value: f32) -> Result<(), ParamError> {
        let clamped = if let Some(def) = self.schema.get(name) {
            def.clamp(value)
        } else {
            return Err(ParamError::UnknownParam(name.into()));
        };
        self.targets.insert(name.to_owned(), clamped);
        self.values.insert(name.to_owned(), clamped);
        Ok(())
    }

    // ── Frame tick ────────────────────────────────────────────────────────

    /// Advance all smoothed values one step toward their targets.
    ///
    /// Call exactly once per frame, before reading values for NodeConfig.
    ///
    /// Smoothing formula: `value = lerp(value, target, 1.0 - smooth)`
    /// - smooth = 0.0 → instant (value = target every frame)
    /// - smooth = 0.9 → very slow (10% of distance closed each frame)
    pub fn step_frame(&mut self) {
        for p in &self.schema.params {
            let target = *self.targets.get(&p.name).unwrap_or(&p.default);
            let value  = self.values.entry(p.name.clone()).or_insert(p.default);
            if p.smooth <= 0.0 {
                *value = target;
            } else {
                let rate = 1.0 - p.smooth.clamp(0.0, 0.999);
                *value += (*value - target).abs().min(1.0) * rate * (target - *value).signum();
                // Snap to target when very close (prevents infinite asymptote)
                if (*value - target).abs() < 0.0001 {
                    *value = target;
                }
            }
        }
    }

    // ── Read API (called by render loop) ──────────────────────────────────

    /// Read the current smoothed value for a parameter by name.
    /// Returns `None` if the name is not in the schema.
    pub fn get(&self, name: &str) -> Option<f32> {
        self.values.get(name).copied()
    }

    /// Read the raw target (unsmoothed) for a parameter.
    pub fn get_target(&self, name: &str) -> Option<f32> {
        self.targets.get(name).copied()
    }

    /// Return all smoothed values as a map (for NodeConfig building).
    pub fn all_values(&self) -> &HashMap<String, f32> {
        &self.values
    }

    /// Return the schema (for introspection, OSC address listing, etc.).
    pub fn schema(&self) -> &ParamSchema {
        &self.schema
    }

    /// Reload schema from a JSON file without losing current values.
    ///
    /// New params are seeded from their defaults.
    /// Existing params keep their current target and smoothed value.
    /// Removed params are dropped.
    pub fn reload_schema(&mut self, path: &str) -> Result<(), ParamError> {
        let new_schema = ParamSchema::load(path)?;
        let mut new_targets = HashMap::new();
        let mut new_values  = HashMap::new();
        for p in &new_schema.params {
            // Preserve existing value if param survived the reload
            new_targets.insert(p.name.clone(),
                self.targets.get(&p.name).copied().unwrap_or(p.default));
            new_values.insert(p.name.clone(),
                self.values.get(&p.name).copied().unwrap_or(p.default));
        }
        self.targets    = new_targets;
        self.values     = new_values;
        self.midi_index = new_schema.midi_cc_index();
        self.osc_index  = new_schema.osc_addr_index();
        self.schema     = new_schema;
        log::info!("ParamStore: schema reloaded from {}", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ParamDef;

    fn make_store() -> ParamStore {
        let schema = ParamSchema {
            version: 1,
            params: vec![
                ParamDef {
                    name: "u_bright".into(), ty: "float".into(),
                    min: 0.0, max: 2.0, default: 1.0, smooth: 0.0,
                    midi_cc: Some(14), midi_channel: None,
                    osc_addr: Some("/scheng/bright".into()),
                    node_label: Some("proc".into()), description: None,
                },
                ParamDef {
                    name: "u_speed".into(), ty: "float".into(),
                    min: 0.0, max: 5.0, default: 1.0, smooth: 0.5,
                    midi_cc: Some(15), midi_channel: None,
                    osc_addr: None, node_label: None, description: None,
                },
            ],
        };
        ParamStore::new(schema)
    }

    #[test]
    fn defaults_on_creation() {
        let store = make_store();
        assert_eq!(store.get("u_bright"), Some(1.0));
        assert_eq!(store.get("u_speed"),  Some(1.0));
    }

    #[test]
    fn set_by_name_clamped() {
        let mut store = make_store();
        store.set_by_name("u_bright", 5.0).unwrap(); // clamps to 2.0
        store.step_frame();
        assert_eq!(store.get("u_bright"), Some(2.0));
    }

    #[test]
    fn set_by_midi_cc() {
        let mut store = make_store();
        store.set_by_midi_cc(14, 127).unwrap(); // CC14 = u_bright → 2.0
        store.step_frame();
        assert!((store.get("u_bright").unwrap() - 2.0).abs() < 0.01);
    }

    #[test]
    fn set_by_osc_addr() {
        let mut store = make_store();
        store.set_by_osc_addr("/scheng/bright", 0.5).unwrap();
        store.step_frame();
        assert_eq!(store.get("u_bright"), Some(0.5));
    }

    #[test]
    fn unknown_param_returns_error() {
        let mut store = make_store();
        assert!(store.set_by_name("u_nonexistent", 1.0).is_err());
        assert!(store.set_by_midi_cc(99, 64).is_err());
    }

    #[test]
    fn smoothing_approaches_target() {
        let mut store = make_store();
        store.set_by_name("u_speed", 5.0).unwrap(); // target = 5.0
        // With smooth=0.5 it should approach but not reach in one frame
        store.step_frame();
        let v = store.get("u_speed").unwrap();
        assert!(v > 1.0 && v < 5.0, "smoothed value should be between 1 and 5, got {v}");
    }

    #[test]
    fn no_smoothing_snaps_immediately() {
        let mut store = make_store();
        store.set_by_name("u_bright", 1.5).unwrap(); // smooth=0.0
        store.step_frame();
        assert_eq!(store.get("u_bright"), Some(1.5));
    }
}
