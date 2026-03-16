//! `schema.rs` — JSON schema types matching shadecore's params.json format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::ParamError;

/// A single parameter definition from params.json.
///
/// Every field except `name` is optional — the schema is forward-compatible
/// with any shadecore params.json file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    /// Uniform name in the shader (e.g. `"u_brightness"`).
    /// Also the key into the ParamStore.
    pub name: String,

    /// Type hint. Currently always `"float"`.
    /// Reserved for future int/bool/vec2 support.
    #[serde(default = "default_ty")]
    pub ty: String,

    /// Minimum value. Default: 0.0.
    #[serde(default)]
    pub min: f32,

    /// Maximum value. Default: 1.0.
    #[serde(default = "default_max")]
    pub max: f32,

    /// Starting value. Default: 0.0.
    #[serde(default)]
    pub default: f32,

    /// Smoothing coefficient [0.0, 1.0].
    /// 0.0 = no smoothing (instant); 1.0 = never moves (infinite smoothing).
    /// Typical live performance value: 0.05–0.15.
    /// Default: 0.0 (no smoothing, matches shadecore default).
    #[serde(default)]
    pub smooth: f32,

    /// MIDI CC number [0, 127]. `None` = not MIDI-controllable.
    #[serde(default)]
    pub midi_cc: Option<u8>,

    /// MIDI channel [1, 16]. `None` = any channel (omni).
    #[serde(default)]
    pub midi_channel: Option<u8>,

    /// OSC address (e.g. `"/scheng/brightness"`). `None` = not OSC-controllable.
    #[serde(default)]
    pub osc_addr: Option<String>,

    /// Which node label this param belongs to.
    /// Used by `NodeConfigBuilder` to route values to the right NodeConfig.
    /// If None, the param is not routed to any specific node (global).
    #[serde(default)]
    pub node_label: Option<String>,

    /// Human-readable description. Not used at runtime.
    #[serde(default)]
    pub description: Option<String>,
}

impl ParamDef {
    /// Clamp a value to this param's [min, max] range.
    pub fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.min, self.max)
    }

    /// Map a MIDI CC raw value [0, 127] into this param's [min, max] range.
    pub fn map_midi(&self, raw: u8) -> f32 {
        let t = raw as f32 / 127.0;
        self.min + t * (self.max - self.min)
    }
}

fn default_ty()  -> String { "float".into() }
fn default_max() -> f32    { 1.0 }

/// The complete params.json schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSchema {
    /// Schema version. Currently 1.
    #[serde(default = "default_version")]
    pub version: u32,

    /// All parameter definitions.
    pub params: Vec<ParamDef>,
}

fn default_version() -> u32 { 1 }

impl ParamSchema {
    /// Load from a params.json file.
    pub fn load(path: &str) -> Result<Self, ParamError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ParamError::Io { path: path.into(), source: e })?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Build an empty schema with no params.
    pub fn empty() -> Self {
        Self { version: 1, params: vec![] }
    }

    /// Look up a param by name.
    pub fn get(&self, name: &str) -> Option<&ParamDef> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Build a MIDI CC → param name index.
    pub fn midi_cc_index(&self) -> HashMap<u8, String> {
        self.params.iter()
            .filter_map(|p| p.midi_cc.map(|cc| (cc, p.name.clone())))
            .collect()
    }

    /// Build an OSC address → param name index.
    pub fn osc_addr_index(&self) -> HashMap<String, String> {
        self.params.iter()
            .filter_map(|p| p.osc_addr.as_ref().map(|a| (a.clone(), p.name.clone())))
            .collect()
    }

    /// Serialize back to pretty JSON.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_params_json() {
        let json = r#"{"version":1,"params":[
            {"name":"u_brightness","min":0.0,"max":2.0,"default":1.0,"midi_cc":14}
        ]}"#;
        let schema: ParamSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.params.len(), 1);
        assert_eq!(schema.params[0].name, "u_brightness");
        assert_eq!(schema.params[0].midi_cc, Some(14));
    }

    #[test]
    fn map_midi_full_range() {
        let p = ParamDef {
            name: "u_gain".into(), ty: "float".into(),
            min: 0.0, max: 2.0, default: 1.0, smooth: 0.0,
            midi_cc: Some(1), midi_channel: None,
            osc_addr: None, node_label: None, description: None,
        };
        assert!((p.map_midi(0)   - 0.0).abs() < 0.001);
        assert!((p.map_midi(127) - 2.0).abs() < 0.001);
        assert!((p.map_midi(64)  - 1.007).abs() < 0.01);
    }

    #[test]
    fn midi_cc_index() {
        let json = r#"{"params":[
            {"name":"u_a","midi_cc":1},
            {"name":"u_b","midi_cc":2},
            {"name":"u_c"}
        ]}"#;
        let schema: ParamSchema = serde_json::from_str(json).unwrap();
        let idx = schema.midi_cc_index();
        assert_eq!(idx.get(&1).unwrap(), "u_a");
        assert_eq!(idx.get(&2).unwrap(), "u_b");
        assert!(!idx.contains_key(&3));
    }
}
