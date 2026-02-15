use scheng_runtime::MatrixPreset;

#[derive(Debug, Clone, Copy)]
pub struct QueuedStep {
    pub bank_idx: usize,
    pub scene_idx: usize,
    pub preset: MatrixPreset,
}
