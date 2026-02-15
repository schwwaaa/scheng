#![forbid(unsafe_code)]

//! Backend-agnostic runtime "standard library".
//!
//! This crate defines semantic operations and parameter blocks that backends implement.
//! `scheng-graph` stays declarative; runtimes/backends decide how to realize these ops.
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_debug_implementations)]

use scheng_graph::NodeKind;
pub mod runtime_contract;
// -------------------------------------------------------------------------------------------------
// Standard ops
// -------------------------------------------------------------------------------------------------


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixerOp {
    /// 2-input crossfade.
    Crossfade,
    /// 2-input additive blend.
    Add,
    /// 2-input multiplicative blend.
    Multiply,
    /// Weighted sum of up to 4 inputs (iChannel0..3).
    ///
    /// This is the minimal "matrix mixer" primitive: higher-level matrix/routing tools can be
    /// expressed as weights over stable input ports.
    MatrixMix4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardOp {
    Mixer(MixerOp),
}

// -------------------------------------------------------------------------------------------------
// Parameter blocks
// -------------------------------------------------------------------------------------------------

/// Parameters for 2-input mixers (e.g., Crossfade).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerParams {
    /// Crossfade amount: 0.0 = A, 1.0 = B.
    pub mix: f32,
}

impl Default for MixerParams {
    fn default() -> Self {
        Self { mix: 0.5 }
    }
}

/// Parameters for MatrixMix4.
///
/// Output = Σ texture(iChannelN) * weights[N]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatrixMixParams {
    pub weights: [f32; 4],
}

impl Default for MatrixMixParams {
    fn default() -> Self {
        Self {
            // default passthrough channel 0
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Presets (C4d)
// -------------------------------------------------------------------------------------------------

/// Named presets for `MatrixMix4`.
///
/// These are intended for live-performance usability: you can switch routings/weights without
/// re-authoring the graph, and without relying on edge insertion order. Presets are deterministic
/// and backend-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatrixPreset {
    Solo0,
    Solo1,
    Solo2,
    Solo3,
    /// Equal weights across all 4 inputs.
    Quad,
    /// A/B split (0,1) vs (2,3).
    Sum01,
    Sum23,
}

impl MatrixPreset {
    pub const ALL: [MatrixPreset; 7] = [
        MatrixPreset::Solo0,
        MatrixPreset::Solo1,
        MatrixPreset::Solo2,
        MatrixPreset::Solo3,
        MatrixPreset::Quad,
        MatrixPreset::Sum01,
        MatrixPreset::Sum23,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MatrixPreset::Solo0 => "solo0",
            MatrixPreset::Solo1 => "solo1",
            MatrixPreset::Solo2 => "solo2",
            MatrixPreset::Solo3 => "solo3",
            MatrixPreset::Quad => "quad",
            MatrixPreset::Sum01 => "sum01",
            MatrixPreset::Sum23 => "sum23",
        }
    }

    pub fn params(self) -> MatrixMixParams {
        match self {
            MatrixPreset::Solo0 => MatrixMixParams {
                weights: [1.0, 0.0, 0.0, 0.0],
            },
            MatrixPreset::Solo1 => MatrixMixParams {
                weights: [0.0, 1.0, 0.0, 0.0],
            },
            MatrixPreset::Solo2 => MatrixMixParams {
                weights: [0.0, 0.0, 1.0, 0.0],
            },
            MatrixPreset::Solo3 => MatrixMixParams {
                weights: [0.0, 0.0, 0.0, 1.0],
            },
            MatrixPreset::Quad => MatrixMixParams {
                weights: [0.25, 0.25, 0.25, 0.25],
            },
            MatrixPreset::Sum01 => MatrixMixParams {
                weights: [0.5, 0.5, 0.0, 0.0],
            },
            MatrixPreset::Sum23 => MatrixMixParams {
                weights: [0.0, 0.0, 0.5, 0.5],
            },
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Mapping: graph NodeKind -> standard runtime op
// -------------------------------------------------------------------------------------------------

/// Maps a graph `NodeKind` to a standard runtime operation (if any).
///
/// Backends should use this mapping table to decide which built-in implementation to use when
/// no shader is provided for a node.
pub fn standard_op_for(kind: NodeKind) -> Option<StandardOp> {
    use NodeKind::*;
    match kind {
        Crossfade => Some(StandardOp::Mixer(MixerOp::Crossfade)),
        Add => Some(StandardOp::Mixer(MixerOp::Add)),
        Multiply => Some(StandardOp::Mixer(MixerOp::Multiply)),
        MatrixMix4 => Some(StandardOp::Mixer(MixerOp::MatrixMix4)),
        _ => None,
    }
}

// -------------------------------------------------------------------------------------------------
// Bank/scene helpers (portable performance data)
// -------------------------------------------------------------------------------------------------

/// A named scene that selects a preset (can be extended later with keyer params, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDef {
    pub name: String,
    pub preset: MatrixPreset,
}

/// A named bank (collection of scenes).
#[derive(Debug, Clone, PartialEq)]
pub struct BankDef {
    pub name: String,
    pub scenes: Vec<SceneDef>,
}

/// A collection of banks.
#[derive(Debug, Clone, PartialEq)]
pub struct BankSet {
    pub banks: Vec<BankDef>,
}

impl BankSet {
    /// Built-in banks for the matrix mixer examples (safe fallback when no JSON is provided).
    pub fn builtin_matrix_banks() -> Self {
        let basic = BankDef {
            name: "Basic".to_string(),
            scenes: vec![
                SceneDef {
                    name: "solo_0".to_string(),
                    preset: MatrixPreset::Solo0,
                },
                SceneDef {
                    name: "solo_1".to_string(),
                    preset: MatrixPreset::Solo1,
                },
                SceneDef {
                    name: "solo_2".to_string(),
                    preset: MatrixPreset::Solo2,
                },
                SceneDef {
                    name: "solo_3".to_string(),
                    preset: MatrixPreset::Solo3,
                },
                SceneDef {
                    name: "quad".to_string(),
                    preset: MatrixPreset::Quad,
                },
                SceneDef {
                    name: "sum01".to_string(),
                    preset: MatrixPreset::Sum01,
                },
                SceneDef {
                    name: "sum23".to_string(),
                    preset: MatrixPreset::Sum23,
                },
            ],
        };

        let dj_cuts = BankDef {
            name: "DJ Cuts".to_string(),
            scenes: vec![
                SceneDef {
                    name: "A".to_string(),
                    preset: MatrixPreset::Solo0,
                },
                SceneDef {
                    name: "B".to_string(),
                    preset: MatrixPreset::Solo1,
                },
                SceneDef {
                    name: "C".to_string(),
                    preset: MatrixPreset::Solo2,
                },
                SceneDef {
                    name: "D".to_string(),
                    preset: MatrixPreset::Solo3,
                },
                SceneDef {
                    name: "AB".to_string(),
                    preset: MatrixPreset::Sum01,
                },
                SceneDef {
                    name: "CD".to_string(),
                    preset: MatrixPreset::Sum23,
                },
                SceneDef {
                    name: "ALL".to_string(),
                    preset: MatrixPreset::Quad,
                },
            ],
        };

        BankSet {
            banks: vec![basic, dj_cuts],
        }
    }

    #[cfg(feature = "serde")]
    pub fn from_json_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        use std::fs;

        #[derive(serde::Deserialize)]
        struct JsonScene {
            name: String,
            preset: String,
        }
        #[derive(serde::Deserialize)]
        struct JsonBank {
            name: String,
            scenes: Vec<JsonScene>,
        }
        #[derive(serde::Deserialize)]
        struct JsonRoot {
            banks: Vec<JsonBank>,
        }

        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let root: JsonRoot =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse json: {e}"))?;

        if root.banks.is_empty() {
            return Err("json has no banks".to_string());
        }

        let mut banks = Vec::new();
        for b in root.banks {
            if b.scenes.is_empty() {
                continue;
            }
            let mut scenes = Vec::new();
            for s in b.scenes {
                let Some(p) = preset_from_str(&s.preset) else {
                    return Err(format!(
                        "unknown preset '{}' in scene '{}'",
                        s.preset, s.name
                    ));
                };
                scenes.push(SceneDef {
                    name: s.name,
                    preset: p,
                });
            }
            banks.push(BankDef {
                name: b.name,
                scenes,
            });
        }

        if banks.is_empty() {
            return Err("json banks had no valid scenes".to_string());
        }

        Ok(BankSet { banks })
    }
}

/// Convert user-facing strings to a known preset name.
///
/// Accepts common aliases: `solo0`, `solo_0`, `Solo0`, etc.
pub fn preset_from_str(s: &str) -> Option<MatrixPreset> {
    match s {
        "solo0" | "solo_0" | "Solo0" => Some(MatrixPreset::Solo0),
        "solo1" | "solo_1" | "Solo1" => Some(MatrixPreset::Solo1),
        "solo2" | "solo_2" | "Solo2" => Some(MatrixPreset::Solo2),
        "solo3" | "solo_3" | "Solo3" => Some(MatrixPreset::Solo3),
        "quad" | "Quad" => Some(MatrixPreset::Quad),
        "sum01" | "Sum01" => Some(MatrixPreset::Sum01),
        "sum23" | "Sum23" => Some(MatrixPreset::Sum23),
        _ => None,
    }
}
