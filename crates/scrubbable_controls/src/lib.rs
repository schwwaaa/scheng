use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use rosc::{OscMessage, OscType};
use scheng_param_store::ParamStore;

// ── Transport state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransportState {
    pub speed:       f32,
    pub paused:      bool,
    pub norm_pos:    f32,
    pub scrub_delta: f32,
}

impl Default for TransportState {
    fn default() -> Self {
        Self { speed: 1.0, paused: false, norm_pos: 0.0, scrub_delta: 0.0 }
    }
}

// ── Color state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ColorState {
    pub brightness: f32,
    pub contrast:   f32,
    pub saturation: f32,
}

impl Default for ColorState {
    fn default() -> Self {
        Self { brightness: 0.0, contrast: 1.0, saturation: 1.0 }
    }
}

// ── Key action kinds ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    // ParamStore — set a named shader param to an absolute value
    SetParam { name: String, value: f32 },
    // ParamStore — add a delta to a named shader param each keypress
    NudgeParam { name: String, delta: f32 },
}

// ── OSC action kinds ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OscActionKind {
    // Transport
    TogglePause,
    Pause,
    Play,
    SetSpeedFromArg,
    NudgeSpeedFromArg,
    JumpNormFromArg,
    ScrubDeltaFromArg,

    // Color
    BrightnessDeltaFromArg,
    ContrastDeltaFromArg,
    SaturationDeltaFromArg,

    // ParamStore — set a named shader param to the OSC float arg value
    SetParamFromArg { name: String },
    // ParamStore — multiply the OSC float arg by scale before writing
    SetParamScaled  { name: String, scale: f32 },
}

// ── Concrete actions ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ConcreteActionKind {
    // Transport
    TogglePause,
    Pause,
    Play,
    SetSpeed   { speed: f32 },
    NudgeSpeed { factor: f32 },
    ScrubDelta { delta: f32 },
    JumpNorm   { t: f32 },

    // Color
    BrightnessDelta { delta: f32 },
    ContrastDelta   { delta: f32 },
    SaturationDelta { delta: f32 },

    // ParamStore
    SetParam   { name: String, value: f32 },
    NudgeParam { name: String, delta: f32 },
}

#[derive(Debug, Clone)]
pub struct ConcreteAction {
    pub kind: ConcreteActionKind,
}

impl ConcreteAction {
    /// Apply transport and color actions to their respective state structs.
    pub fn apply(&self, tr: &mut TransportState, col: &mut ColorState) {
        match &self.kind {
            ConcreteActionKind::TogglePause         => { tr.paused = !tr.paused; }
            ConcreteActionKind::Pause               => { tr.paused = true; }
            ConcreteActionKind::Play                => { tr.paused = false; if tr.speed == 0.0 { tr.speed = 1.0; } }
            ConcreteActionKind::SetSpeed   { speed }  => { tr.speed = *speed; if *speed != 0.0 { tr.paused = false; } }
            ConcreteActionKind::NudgeSpeed { factor }  => { tr.speed *= factor; }
            ConcreteActionKind::ScrubDelta { delta }   => { tr.scrub_delta += delta; }
            ConcreteActionKind::JumpNorm   { t }       => { tr.norm_pos = t.clamp(0.0, 1.0); }
            ConcreteActionKind::BrightnessDelta { delta } => { col.brightness = (col.brightness + delta).clamp(-2.0, 2.0); }
            ConcreteActionKind::ContrastDelta   { delta } => { col.contrast   = (col.contrast   + delta).clamp(0.0, 4.0); }
            ConcreteActionKind::SaturationDelta { delta } => { col.saturation = (col.saturation + delta).clamp(0.0, 4.0); }
            // Param actions handled separately in apply_to_store()
            ConcreteActionKind::SetParam   { .. } => {}
            ConcreteActionKind::NudgeParam { .. } => {}
        }
    }

    /// Apply ParamStore actions. Call this after apply() each event.
    ///
    /// NudgeParam reads the current smoothed value and adds the delta,
    /// then writes the result back as a new target. This gives keyboard
    /// keys an incremental scrubbing feel.
    pub fn apply_to_store(&self, store: &mut ParamStore) {
        match &self.kind {
            ConcreteActionKind::SetParam { name, value } => {
                if let Err(e) = store.set_by_name(name, *value) {
                    log::warn!("scrubbable_controls SetParam '{}': {}", name, e);
                }
            }
            ConcreteActionKind::NudgeParam { name, delta } => {
                let current = store.get(name).unwrap_or(0.0);
                if let Err(e) = store.set_by_name(name, current + delta) {
                    log::warn!("scrubbable_controls NudgeParam '{}': {}", name, e);
                }
            }
            _ => {}
        }
    }
}

// ── Keymap ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingConfig {
    pub key:    String,
    pub action: KeyActionKind,
}

#[derive(Debug, Default)]
pub struct Keymap {
    bindings: HashMap<char, KeyActionKind>,
}

impl Keymap {
    pub fn from_config(cfgs: &[KeyBindingConfig]) -> Self {
        let mut bindings = HashMap::new();
        for cfg in cfgs {
            if let Some(ch) = cfg.key.chars().next() {
                bindings.insert(ch, cfg.action.clone());
            }
        }
        Self { bindings }
    }

    pub fn lookup(&self, ch: char) -> Option<ConcreteAction> {
        let kind = self.bindings.get(&ch)?;
        let ck = match kind {
            KeyActionKind::TogglePause           => ConcreteActionKind::TogglePause,
            KeyActionKind::Pause                 => ConcreteActionKind::Pause,
            KeyActionKind::Play                  => ConcreteActionKind::Play,
            KeyActionKind::SetSpeed(s)           => ConcreteActionKind::SetSpeed   { speed: *s },
            KeyActionKind::NudgeSpeed(f)         => ConcreteActionKind::NudgeSpeed { factor: *f },
            KeyActionKind::ScrubDelta(d)         => ConcreteActionKind::ScrubDelta { delta: *d },
            KeyActionKind::JumpNorm(t)           => ConcreteActionKind::JumpNorm   { t: *t },
            KeyActionKind::BrightnessDelta(d)    => ConcreteActionKind::BrightnessDelta { delta: *d },
            KeyActionKind::ContrastDelta(d)      => ConcreteActionKind::ContrastDelta   { delta: *d },
            KeyActionKind::SaturationDelta(d)    => ConcreteActionKind::SaturationDelta { delta: *d },
            KeyActionKind::SetParam   { name, value } => ConcreteActionKind::SetParam   { name: name.clone(), value: *value },
            KeyActionKind::NudgeParam { name, delta } => ConcreteActionKind::NudgeParam { name: name.clone(), delta: *delta },
        };
        Some(ConcreteAction { kind: ck })
    }
}

// ── OSC map ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscBindingConfig {
    pub addr: String,
    pub kind: OscActionKind,
}

#[derive(Debug, Default)]
pub struct Oscmap {
    bindings: HashMap<String, OscActionKind>,
}

impl Oscmap {
    pub fn from_config(cfgs: &[OscBindingConfig]) -> Self {
        let mut bindings = HashMap::new();
        for cfg in cfgs {
            bindings.insert(cfg.addr.clone(), cfg.kind.clone());
        }
        Self { bindings }
    }

    pub fn lookup(&self, msg: &OscMessage) -> Option<ConcreteAction> {
        let kind = self.bindings.get(&msg.addr)?;
        let f32_arg = || msg.args.first().and_then(osc_to_f32);

        let ck = match kind {
            OscActionKind::TogglePause    => ConcreteActionKind::TogglePause,
            OscActionKind::Pause          => ConcreteActionKind::Pause,
            OscActionKind::Play           => ConcreteActionKind::Play,
            OscActionKind::SetSpeedFromArg       => ConcreteActionKind::SetSpeed   { speed:  f32_arg()? },
            OscActionKind::NudgeSpeedFromArg     => ConcreteActionKind::NudgeSpeed { factor: f32_arg()? },
            OscActionKind::JumpNormFromArg       => ConcreteActionKind::JumpNorm   { t:      f32_arg()? },
            OscActionKind::ScrubDeltaFromArg     => ConcreteActionKind::ScrubDelta { delta:  f32_arg()? },
            OscActionKind::BrightnessDeltaFromArg => ConcreteActionKind::BrightnessDelta { delta: f32_arg()? },
            OscActionKind::ContrastDeltaFromArg   => ConcreteActionKind::ContrastDelta   { delta: f32_arg()? },
            OscActionKind::SaturationDeltaFromArg => ConcreteActionKind::SaturationDelta { delta: f32_arg()? },
            OscActionKind::SetParamFromArg { name } => ConcreteActionKind::SetParam {
                name:  name.clone(),
                value: f32_arg()?,
            },
            OscActionKind::SetParamScaled { name, scale } => ConcreteActionKind::SetParam {
                name:  name.clone(),
                value: f32_arg()? * scale,
            },
        };
        Some(ConcreteAction { kind: ck })
    }
}

fn osc_to_f32(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(v)  => Some(*v),
        OscType::Double(v) => Some(*v as f32),
        OscType::Int(v)    => Some(*v as f32),
        OscType::Long(v)   => Some(*v as f32),
        _ => None,
    }
}

// ── ControlLayerConfig ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlLayerConfig {
    #[serde(default)]
    pub keys: Vec<KeyBindingConfig>,
    #[serde(default)]
    pub osc:  Vec<OscBindingConfig>,
}

// ── ControlLayer ──────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ControlLayer {
    pub transport: TransportState,
    pub color:     ColorState,
    keymap:        Keymap,
    oscmap:        Oscmap,
}

impl ControlLayer {
    pub fn from_config(cfg: &ControlLayerConfig) -> Self {
        Self {
            transport: TransportState::default(),
            color:     ColorState::default(),
            keymap:    Keymap::from_config(&cfg.keys),
            oscmap:    Oscmap::from_config(&cfg.osc),
        }
    }

    /// Handle a keyboard character event.
    ///
    /// Transport and color actions are applied immediately.
    /// ParamStore actions (SetParam / NudgeParam) are applied to `store`.
    pub fn on_key(&mut self, ch: char, store: &mut ParamStore) {
        if let Some(act) = self.keymap.lookup(ch) {
            act.apply(&mut self.transport, &mut self.color);
            act.apply_to_store(store);
        }
    }

    /// Handle an OSC message.
    ///
    /// Transport and color actions are applied immediately.
    /// ParamStore actions are applied to `store`.
    pub fn on_osc(&mut self, msg: OscMessage, store: &mut ParamStore) {
        if let Some(act) = self.oscmap.lookup(&msg) {
            act.apply(&mut self.transport, &mut self.color);
            act.apply_to_store(store);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use scheng_param_store::{ParamStore, ParamSchema};
    use scheng_param_store::schema::ParamDef;

    fn make_store() -> ParamStore {
        let schema = ParamSchema {
            version: 1,
            params: vec![
                ParamDef {
                    name: "u_thresh".into(), ty: "float".into(),
                    min: 0.0, max: 1.0, default: 0.5, smooth: 0.0,
                    midi_cc: None, midi_channel: None,
                    osc_addr: None, node_label: None, description: None,
                },
            ],
        };
        ParamStore::new(schema)
    }

    fn make_layer() -> ControlLayer {
        let cfg = ControlLayerConfig {
            keys: vec![
                KeyBindingConfig {
                    key:    "t".into(),
                    action: KeyActionKind::SetParam {
                        name:  "u_thresh".into(),
                        value: 0.8,
                    },
                },
                KeyBindingConfig {
                    key:    "n".into(),
                    action: KeyActionKind::NudgeParam {
                        name:  "u_thresh".into(),
                        delta: 0.05,
                    },
                },
            ],
            osc: vec![
                OscBindingConfig {
                    addr: "/scheng/thresh".into(),
                    kind: OscActionKind::SetParamFromArg { name: "u_thresh".into() },
                },
            ],
        };
        ControlLayer::from_config(&cfg)
    }

    #[test]
    fn set_param_via_key() {
        let mut layer = make_layer();
        let mut store = make_store();
        layer.on_key('t', &mut store);
        store.step_frame();
        let v = store.get("u_thresh").unwrap();
        assert!((v - 0.8).abs() < 1e-6, "Expected 0.8, got {v}");
    }

    #[test]
    fn nudge_param_via_key() {
        let mut layer = make_layer();
        let mut store = make_store();
        // default is 0.5, nudge by 0.05 → 0.55
        layer.on_key('n', &mut store);
        store.step_frame();
        let v = store.get("u_thresh").unwrap();
        assert!((v - 0.55).abs() < 1e-5, "Expected 0.55, got {v}");
    }

    #[test]
    fn set_param_via_osc() {
        let mut layer = make_layer();
        let mut store = make_store();
        let msg = rosc::OscMessage {
            addr: "/scheng/thresh".into(),
            args: vec![rosc::OscType::Float(0.3)],
        };
        layer.on_osc(msg, &mut store);
        store.step_frame();
        let v = store.get("u_thresh").unwrap();
        assert!((v - 0.3).abs() < 1e-6, "Expected 0.3, got {v}");
    }
}
