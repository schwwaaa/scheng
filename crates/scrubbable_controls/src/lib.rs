use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use rosc::{OscMessage, OscType};

/// Transport state that the video decoder will read each frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransportState {
    /// Playback speed multiplier. 1.0 = normal, 0.5 = half, -1.0 = reverse, etc.
    pub speed: f32,
    /// Whether playback is paused.
    pub paused: bool,
    /// Normalized position [0, 1] in the clip.
    pub norm_pos: f32,
    /// Per-frame scrub delta in normalized units.
    pub scrub_delta: f32,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            speed: 1.0,
            paused: false,
            norm_pos: 0.0,
            scrub_delta: 0.0,
        }
    }
}

/// Color correction state the graph can read as uniforms.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ColorState {
    /// Brightness offset, e.g. [-2, 2].
    pub brightness: f32,
    /// Contrast multiplier, e.g. [0, 4].
    pub contrast: f32,
    /// Saturation multiplier, e.g. [0, 4].
    pub saturation: f32,
}

impl Default for ColorState {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        }
    }
}

/// High-level control layer combining transport and color.
#[derive(Debug, Default)]
pub struct ControlLayer {
    pub transport: TransportState,
    pub color: ColorState,
    keymap: Keymap,
    oscmap: Oscmap,
}

/// Configuration for a single keyboard binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingConfig {
    /// A single character, e.g. " " or "j" or "K".
    pub key: String,
    pub action: KeyActionKind,
}

/// Configuration for a single OSC binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscBindingConfig {
    /// OSC address, e.g. "/transport/speed".
    pub addr: String,
    pub kind: OscActionKind,
}

/// JSON config for the whole layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlLayerConfig {
    #[serde(default)]
    pub keys: Vec<KeyBindingConfig>,
    #[serde(default)]
    pub osc: Vec<OscBindingConfig>,
}

/// Actions directly exposed in the keymap JSON.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum KeyActionKind {
    // Transport
    TogglePause,
    Pause,
    Play,
    SetSpeed(f32),
    NudgeSpeed(f32),
    ScrubDelta(f32),
    JumpNorm(f32),

    // Color
    BrightnessDelta(f32),
    ContrastDelta(f32),
    SaturationDelta(f32),
}

/// Actions directly exposed in the OSC map JSON.
/// These generally read 0 or 1 float arg and then become a ConcreteAction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OscActionKind {
    TogglePause,
    Pause,
    Play,

    SetSpeedFromArg,
    NudgeSpeedFromArg,
    JumpNormFromArg,
    ScrubDeltaFromArg,

    BrightnessDeltaFromArg,
    ContrastDeltaFromArg,
    SaturationDeltaFromArg,
}

/// The concrete action that mutates TransportState and ColorState.
#[derive(Debug, Clone, Copy)]
pub enum ConcreteActionKind {
    // Transport
    TogglePause,
    Pause,
    Play,
    SetSpeed { speed: f32 },
    NudgeSpeed { factor: f32 },
    ScrubDelta { delta: f32 },
    JumpNorm { t: f32 },

    // Color
    BrightnessDelta { delta: f32 },
    ContrastDelta { delta: f32 },
    SaturationDelta { delta: f32 },
}

#[derive(Debug, Clone, Copy)]
pub struct ConcreteAction {
    pub kind: ConcreteActionKind,
}

impl Default for ConcreteAction {
    fn default() -> Self {
        Self {
            kind: ConcreteActionKind::TogglePause,
        }
    }
}

/// Helper to clamp f32 into [min, max].
fn clamp_f32(x: f32, min: f32, max: f32) -> f32 {
    if x < min {
        min
    } else if x > max {
        max
    } else {
        x
    }
}

/// Helper to parse an OSC argument into f32, if possible.
fn parse_osc_f32(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(v) => Some(*v),
        OscType::Double(v) => Some(*v as f32),
        OscType::Int(v) => Some(*v as f32),
        OscType::Long(v) => Some(*v as f32),
        _ => None,
    }
}

/// Keymap: maps a char to a concrete action.
#[derive(Debug, Default)]
pub struct Keymap {
    bindings: HashMap<char, KeyActionKind>,
}

impl Keymap {
    pub fn from_config(cfgs: &[KeyBindingConfig]) -> Self {
        let mut bindings = HashMap::new();
        for cfg in cfgs {
            if let Some(ch) = cfg.key.chars().next() {
                bindings.insert(ch, cfg.action);
            }
        }
        Self { bindings }
    }

    pub fn lookup(&self, ch: char) -> Option<ConcreteAction> {
        let kind = self.bindings.get(&ch)?;
        Some(ConcreteAction {
            kind: match kind {
                KeyActionKind::TogglePause => ConcreteActionKind::TogglePause,
                KeyActionKind::Pause => ConcreteActionKind::Pause,
                KeyActionKind::Play => ConcreteActionKind::Play,
                KeyActionKind::SetSpeed(speed) => ConcreteActionKind::SetSpeed { speed: *speed },
                KeyActionKind::NudgeSpeed(factor) => {
                    ConcreteActionKind::NudgeSpeed { factor: *factor }
                }
                KeyActionKind::ScrubDelta(delta) => {
                    ConcreteActionKind::ScrubDelta { delta: *delta }
                }
                KeyActionKind::JumpNorm(t) => ConcreteActionKind::JumpNorm { t: *t },
                KeyActionKind::BrightnessDelta(delta) => {
                    ConcreteActionKind::BrightnessDelta { delta: *delta }
                }
                KeyActionKind::ContrastDelta(delta) => {
                    ConcreteActionKind::ContrastDelta { delta: *delta }
                }
                KeyActionKind::SaturationDelta(delta) => {
                    ConcreteActionKind::SaturationDelta { delta: *delta }
                }
            },
        })
    }
}

/// OSC map: maps an OSC address to an OSC action kind.
#[derive(Debug, Default)]
pub struct Oscmap {
    bindings: HashMap<String, OscActionKind>,
}

impl Oscmap {
    pub fn from_config(cfgs: &[OscBindingConfig]) -> Self {
        let mut bindings = HashMap::new();
        for cfg in cfgs {
            bindings.insert(cfg.addr.clone(), cfg.kind);
        }
        Self { bindings }
    }

    pub fn lookup(&self, msg: &OscMessage) -> Option<ConcreteAction> {
        let kind = self.bindings.get(&msg.addr)?;
        // Most OSC actions take a single float argument.
        let to_f32_arg =
            |msg: &OscMessage| msg.args.first().and_then(|arg| parse_osc_f32(arg));

        let kind = match kind {
            OscActionKind::TogglePause => ConcreteActionKind::TogglePause,
            OscActionKind::Pause => ConcreteActionKind::Pause,
            OscActionKind::Play => ConcreteActionKind::Play,

            OscActionKind::SetSpeedFromArg => {
                if let Some(speed) = to_f32_arg(msg) {
                    ConcreteActionKind::SetSpeed { speed }
                } else {
                    return None;
                }
            }
            OscActionKind::NudgeSpeedFromArg => {
                if let Some(factor) = to_f32_arg(msg) {
                    ConcreteActionKind::NudgeSpeed { factor }
                } else {
                    return None;
                }
            }
            OscActionKind::JumpNormFromArg => {
                if let Some(t) = to_f32_arg(msg) {
                    ConcreteActionKind::JumpNorm { t }
                } else {
                    return None;
                }
            }
            OscActionKind::ScrubDeltaFromArg => {
                if let Some(delta) = to_f32_arg(msg) {
                    ConcreteActionKind::ScrubDelta { delta }
                } else {
                    return None;
                }
            }
            OscActionKind::BrightnessDeltaFromArg => {
                if let Some(delta) = to_f32_arg(msg) {
                    ConcreteActionKind::BrightnessDelta { delta }
                } else {
                    return None;
                }
            }
            OscActionKind::ContrastDeltaFromArg => {
                if let Some(delta) = to_f32_arg(msg) {
                    ConcreteActionKind::ContrastDelta { delta }
                } else {
                    return None;
                }
            }
            OscActionKind::SaturationDeltaFromArg => {
                if let Some(delta) = to_f32_arg(msg) {
                    ConcreteActionKind::SaturationDelta { delta }
                } else {
                    return None;
                }
            }
        };

        Some(ConcreteAction { kind })
    }
}

impl ConcreteAction {
    pub fn apply(self, tr: &mut TransportState, col: &mut ColorState) {
        match self.kind {
            // Transport
            ConcreteActionKind::TogglePause => {
                tr.paused = !tr.paused;
            }
            ConcreteActionKind::Pause => {
                tr.paused = true;
            }
            ConcreteActionKind::Play => {
                tr.paused = false;
                if tr.speed == 0.0 {
                    tr.speed = 1.0;
                }
            }
            ConcreteActionKind::SetSpeed { speed } => {
                tr.speed = speed;
                if speed != 0.0 {
                    tr.paused = false;
                }
            }
            ConcreteActionKind::NudgeSpeed { factor } => {
                tr.speed *= factor;
            }
            ConcreteActionKind::ScrubDelta { delta } => {
                tr.scrub_delta += delta;
            }
            ConcreteActionKind::JumpNorm { t } => {
                tr.norm_pos = clamp_f32(t, 0.0, 1.0);
            }

            // Color
            ConcreteActionKind::BrightnessDelta { delta } => {
                col.brightness = clamp_f32(col.brightness + delta, -2.0, 2.0);
            }
            ConcreteActionKind::ContrastDelta { delta } => {
                col.contrast = clamp_f32(col.contrast + delta, 0.0, 4.0);
            }
            ConcreteActionKind::SaturationDelta { delta } => {
                col.saturation = clamp_f32(col.saturation + delta, 0.0, 4.0);
            }
        }
    }
}

impl From<ConcreteActionKind> for ConcreteAction {
    fn from(kind: ConcreteActionKind) -> Self {
        Self { kind }
    }
}

impl ControlLayer {
    pub fn from_config(cfg: &ControlLayerConfig) -> Self {
        Self {
            transport: TransportState::default(),
            color: ColorState::default(),
            keymap: Keymap::from_config(&cfg.keys),
            oscmap: Oscmap::from_config(&cfg.osc),
        }
    }

    /// Call this from your winit keyboard handler.
    pub fn on_key(&mut self, ch: char) {
        if let Some(act) = self.keymap.lookup(ch) {
            act.apply(&mut self.transport, &mut self.color);
        }
    }

    /// Call this from your OSC handler.
    pub fn on_osc(&mut self, msg: OscMessage) {
        if let Some(act) = self.oscmap.lookup(&msg) {
            act.apply(&mut self.transport, &mut self.color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosc::{OscMessage, OscType};

    fn layer_with_basic_keymap() -> ControlLayer {
        let cfg = ControlLayerConfig {
            keys: vec![
                KeyBindingConfig {
                    key: " ".to_string(),
                    action: KeyActionKind::TogglePause,
                },
                KeyBindingConfig {
                    key: "f".to_string(),
                    action: KeyActionKind::SetSpeed(0.5),
                },
                KeyBindingConfig {
                    key: "b".to_string(),
                    action: KeyActionKind::BrightnessDelta(0.1),
                },
            ],
            osc: vec![
                OscBindingConfig {
                    addr: "/transport/speed".to_string(),
                    kind: OscActionKind::SetSpeedFromArg,
                },
                OscBindingConfig {
                    addr: "/color/brightness_delta".to_string(),
                    kind: OscActionKind::BrightnessDeltaFromArg,
                },
            ],
        };

        ControlLayer::from_config(&cfg)
    }

    #[test]
    fn key_toggle_pause_and_speed() {
        let mut layer = layer_with_basic_keymap();

        // initial state
        assert_eq!(layer.transport.paused, false);
        assert!((layer.transport.speed - 1.0).abs() < 1e-6);

        // press space → toggle pause
        layer.on_key(' ');
        assert_eq!(layer.transport.paused, true);

        // press space again → back to play
        layer.on_key(' ');
        assert_eq!(layer.transport.paused, false);

        // press 'f' → set speed 0.5
        layer.on_key('f');
        assert!((layer.transport.speed - 0.5).abs() < 1e-6);
    }

    #[test]
    fn key_brightness_delta_clamps() {
        let mut layer = layer_with_basic_keymap();

        // press 'b' a bunch of times
        for _ in 0..50 {
            layer.on_key('b');
        }

        // brightness should be clamped to <= 2.0
        assert!(layer.color.brightness <= 2.0 + 1e-6);
    }

    #[test]
    fn osc_set_speed_and_brightness() {
        let mut layer = layer_with_basic_keymap();

        // OSC: /transport/speed 0.25
        let msg = OscMessage {
            addr: "/transport/speed".to_string(),
            args: vec![OscType::Float(0.25)],
        };
        layer.on_osc(msg);
        assert!((layer.transport.speed - 0.25).abs() < 1e-6);

        // OSC: /color/brightness_delta 0.5
        let msg = OscMessage {
            addr: "/color/brightness_delta".to_string(),
            args: vec![OscType::Float(0.5)],
        };
        layer.on_osc(msg);
        assert!((layer.color.brightness - 0.5).abs() < 1e-6);
    }
}
