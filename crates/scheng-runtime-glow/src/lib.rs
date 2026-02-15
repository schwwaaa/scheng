//! scheng runtime (glow/OpenGL backend)
//
// This crate intentionally contains **only** the shader machine runtime:
// - compile/link shaders
// - manage a render target (FBO + texture)
// - render full-screen passes (host provides timing/params/textures)
//
// It does NOT contain windowing, file IO, hot-reload policy, MIDI/OSC, recording, or sinks.
#![allow(clippy::missing_safety_doc)]

use glow::HasContext;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use scheng_graph::{Edge, Graph, NodeClass, NodeId, NodeKind, Plan, PortDir, PortId};
use scheng_input_video as input_video;
use scheng_runtime::{standard_op_for, MixerOp, StandardOp};

pub use scheng_core::EngineError;
#[derive(Debug, Clone)]
pub struct ShaderSource {
    pub vert: String,
    pub frag: String,
    /// Optional human-friendly origin (path/label) for logs.
    pub origin: Option<String>,
}

/// Offscreen render target (FBO + color texture).
#[derive(Debug)]
pub struct RenderTarget {
    pub fbo: glow::NativeFramebuffer,
    pub tex: glow::NativeTexture,
    pub w: i32,
    pub h: i32,
}

struct VideoNodeState {
    dec: input_video::VideoDecoder,
    tex: glow::NativeTexture,
    w: i32,
    h: i32,
    /// Nominal fps from the video config (used to map FrameCtx::time -> frame index).
    fps: f32,
    /// Last timeline frame index we uploaded into the texture, derived from FrameCtx::time.
    last_frame_index: i64,
}

impl std::fmt::Debug for VideoNodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `VideoDecoder` owns OS/process resources and does not implement Debug.
        // For Debug printing we omit internal decoder state.
        f.debug_struct("VideoNodeState")
            .field("tex", &self.tex)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("fps", &self.fps)
            .field("last_frame_index", &self.last_frame_index)
            .field("dec", &"<video decoder>")
            .finish()
    }
}


#[derive(Debug)]
struct PingPong {
    curr: RenderTarget,
    prev: RenderTarget,
}

impl PingPong {
    unsafe fn ensure_size(&mut self, gl: &glow::Context, w: i32, h: i32) {
        if self.curr.w != w || self.curr.h != h {
            self.curr.resize(gl, w, h);
        }
        if self.prev.w != w || self.prev.h != h {
            self.prev.resize(gl, w, h);
        }
    }

    /// Swap current/previous targets at the start of a new frame render for this node.
    fn swap(&mut self) {
        core::mem::swap(&mut self.curr, &mut self.prev);
    }
}

impl RenderTarget {
    /// Resize the render target (realloc texture storage). Keeps same FBO/texture ids.
    pub unsafe fn resize(&mut self, gl: &glow::Context, w: i32, h: i32) {
        self.w = w.max(1);
        self.h = h.max(1);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            self.w,
            self.h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            None,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
}

pub unsafe fn create_render_target(
    gl: &glow::Context,
    w: i32,
    h: i32,
) -> Result<RenderTarget, EngineError> {
    let fbo = gl
        .create_framebuffer()
        .map_err(|e| EngineError::GlCreate(format!("create_framebuffer failed: {e:?}")))?;
    let tex = gl
        .create_texture()
        .map_err(|e| EngineError::GlCreate(format!("create_texture failed: {e:?}")))?;

    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );

    let ww = w.max(1);
    let hh = h.max(1);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        ww,
        hh,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        None,
    );

    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );

    // Optional sanity check
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    if status != glow::FRAMEBUFFER_COMPLETE {
        // clean up
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.delete_framebuffer(fbo);
        gl.delete_texture(tex);
        return Err(EngineError::GlCreate(format!(
            "framebuffer incomplete: 0x{status:x}"
        )));
    }

    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    gl.bind_texture(glow::TEXTURE_2D, None);

    Ok(RenderTarget {
        fbo,
        tex,
        w: ww,
        h: hh,
    })
}


unsafe fn create_host_texture(gl: &glow::Context, w: i32, h: i32) -> glow::NativeTexture {
    let tex = gl.create_texture().expect("create_texture");
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        None,
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    tex
}


pub unsafe fn compile_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::NativeProgram, EngineError> {
    let vs = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| EngineError::GlCreate(format!("create_shader(VS) failed: {e:?}")))?;
    gl.shader_source(vs, vert_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        let log = gl.get_shader_info_log(vs);
        gl.delete_shader(vs);
        return Err(EngineError::VertexCompile(log));
    }

    let fs = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| EngineError::GlCreate(format!("create_shader(FS) failed: {e:?}")))?;
    gl.shader_source(fs, frag_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        let log = gl.get_shader_info_log(fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        return Err(EngineError::FragmentCompile(log));
    }

    let program = gl
        .create_program()
        .map_err(|e| EngineError::GlCreate(format!("create_program failed: {e:?}")))?;
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);

    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(EngineError::Link(log));
    }

    Ok(program)
}

#[derive(Debug)]
pub struct ShaderProgram {
    pub program: glow::NativeProgram,
}

impl ShaderProgram {
    pub unsafe fn new(
        gl: &glow::Context,
        vert_src: &str,
        frag_src: &str,
    ) -> Result<Self, EngineError> {
        let program = compile_program(gl, vert_src, frag_src)?;
        Ok(Self { program })
    }

    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        gl.delete_program(self.program);
    }

    pub unsafe fn replace_frag(
        &mut self,
        gl: &glow::Context,
        vert_src: &str,
        frag_src: &str,
    ) -> Result<(), EngineError> {
        let new_prog = compile_program(gl, vert_src, frag_src)?;
        gl.delete_program(self.program);
        self.program = new_prog;
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// C3: graph → runtime-glow bridge (pull-based)
// -------------------------------------------------------------------------------------------------

/// Per-frame context supplied by the host (pull-based runtime).
#[derive(Clone, Copy, Debug)]
pub struct FrameCtx {
    pub width: i32,
    pub height: i32,
    pub time: f32,
    pub frame: u64,
}

/// Runtime-only properties keyed by graph NodeId.
///
/// v0 (C3b): we only support providing fragment sources for `NodeKind::ShaderSource`.
#[derive(Debug, Default, Clone)]
pub struct NodeProps {
    pub shader_sources: HashMap<NodeId, ShaderSource>,
    /// Parameters for 2-input mixers (e.g., Crossfade).
    pub mixer_params: HashMap<NodeId, scheng_runtime::MixerParams>,
    /// Parameters for matrix mixers (e.g., MatrixMix4).
    pub matrix_params: HashMap<NodeId, scheng_runtime::MatrixMixParams>,
    /// Optional explicit names for `NodeKind::PixelsOut` nodes (Step 5).
    ///
    /// `execute_plan_outputs` will expose each named PixelsOut as an additional entry in
    /// `ExecOutputs.named`. Unnamed PixelsOut nodes are ignored (explicit-only policy).
    pub output_names: HashMap<NodeId, String>,
    pub texture_inputs: HashMap<NodeId, glow::NativeTexture>,
    /// Per-node video decode source configuration loaded from a JSON file (see `scheng-input-video`).
    pub video_decode_json: std::collections::HashMap<scheng_graph::NodeId, std::path::PathBuf>,

    /// Per-node video decode source configuration provided directly.
    pub video_decode_cfg: std::collections::HashMap<scheng_graph::NodeId, input_video::VideoConfig>,

}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ProgramKey {
    vert_hash: u64,
    frag_hash: u64,
}

#[derive(Debug)]
struct ProgramEntry {
    program: glow::NativeProgram,
    key: ProgramKey,
}

/// Mutable runtime state that can persist across frames.
///
/// The host owns the GL context lifecycle. We keep GL object ids here and expose explicit
/// `destroy_*` patterns where needed.
#[derive(Debug)]
pub struct RuntimeState {
    pub fs_tri: FullscreenTriangle,
    programs: HashMap<NodeId, ProgramEntry>,
    program_cache: HashMap<ProgramKey, glow::NativeProgram>,
    targets: HashMap<NodeId, PingPong>,
    video_nodes: HashMap<NodeId, VideoNodeState>,
}

impl RuntimeState {
    /// Creates runtime state and allocates the fullscreen triangle VAO/VBO.
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, EngineError> {
        Ok(Self {
            fs_tri: FullscreenTriangle::new(gl)?,
            programs: HashMap::new(),
            program_cache: HashMap::new(),
            targets: HashMap::new(),
            video_nodes: HashMap::new(),
        })
    }

    /// Explicitly destroys GL objects owned by this state.
    ///
    /// Note: `RenderTarget` cleanup is intentionally conservative: we delete the FBO/texture
    /// for all cached targets. This is safe as long as nothing else is still using them.
    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        // Programs
        for (_, prog) in self.program_cache.drain() {
            gl.delete_program(prog);
        }
        self.programs.clear();
        // Targets
        for (_, pp) in self.targets.drain() {
            gl.delete_framebuffer(pp.curr.fbo);
            gl.delete_texture(pp.curr.tex);
            gl.delete_framebuffer(pp.prev.fbo);
            gl.delete_texture(pp.prev.tex);
        }

        // Video decode nodes (textures + decoder processes)
        for (_, vn) in self.video_nodes.drain() {
            gl.delete_texture(vn.tex);
            // `vn.dec` drops here, terminating ffmpeg reader thread.
        }

        self.fs_tri.destroy(gl);
    }
}

/// Output of executing a plan for one frame.
#[derive(Debug, Clone, Copy)]
pub struct ExecOutput {
    /// The texture containing the final output for the plan (v0 only supports PixelsOut).
    pub tex: glow::NativeTexture,
    /// The backing framebuffer (useful for blitting).
    pub fbo: glow::NativeFramebuffer,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct ExecOutputs {
    /// The traditional final output (backwards-compatible)
    pub primary: ExecOutput,

    /// Named outputs for routing to multiple sinks
    pub named: HashMap<String, ExecOutput>,
}

impl ExecOutputs {
    /// Get a named output (e.g. `"main"`).
    pub fn get(&self, name: &str) -> Option<&ExecOutput> {
        self.named.get(name)
    }

    /// The primary output (also available as `OUTPUT_MAIN`).
    pub fn primary(&self) -> &ExecOutput {
        &self.primary
    }

    /// Iterate named outputs in insertion order (currently just `main`).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ExecOutput)> {
        self.named.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// S3: Stable names for routed outputs (starting with a single `main` output).
///
/// This is an additive API surface that enables patchable routing (a "video synth" style patchbay)
/// without changing the existing `execute_plan` / `execute_plan_to_sink` entrypoints.
pub type OutputName = &'static str;

/// The default (and currently only) named output.
pub const OUTPUT_MAIN: OutputName = "main";

/// S3: Execute a frame and return named outputs (currently only `main`).
///
/// This is a thin wrapper around `execute_plan` to preserve backward compatibility while
/// introducing explicit output routing.
pub unsafe fn execute_plan_outputs(
    gl: &glow::Context,
    graph: &Graph,
    plan: &Plan,
    state: &mut RuntimeState,
    props: &NodeProps,
    frame: FrameCtx,
) -> Result<ExecOutputs, EngineError> {
    let primary = execute_plan(gl, graph, plan, state, props, frame)?;
    let main = primary; // `ExecOutput` is Copy (GL handles).

    let mut named = std::collections::HashMap::new();
    named.insert(OUTPUT_MAIN.to_string(), main);

    // Step 5 (explicit-only): expose additional named outputs backed by `PixelsOut` nodes.
    //
    // We do not re-execute the plan. We resolve each PixelsOut's upstream render-pass target from
    // `state.targets` (populated by `execute_plan` for this frame).
    let resolve_pixels_out = |pixels_out: NodeId| -> Result<ExecOutput, EngineError> {
        let out_edge = graph
            .edges()
            .iter()
            .find(|e| e.to.node == pixels_out && e.to.dir == PortDir::In)
            .ok_or_else(|| EngineError::other("execute_plan_outputs: PixelsOut has no input edge"))?;

        let from_node = graph.node(out_edge.from.node).ok_or_else(|| {
            EngineError::other("execute_plan_outputs: output edge references missing node")
        })?;

        let from_is_render_pass = from_node.kind == NodeKind::ShaderPass
            || from_node.kind.class() == NodeClass::Mixer;
        if !from_is_render_pass {
            return Err(EngineError::other(
                "execute_plan_outputs: PixelsOut input must come from a render pass (ShaderPass or Mixer)",
            ));
        }

        let pp = state.targets.get(&from_node.id).ok_or_else(|| {
            EngineError::other("execute_plan_outputs: missing render target for output pass")
        })?;

        Ok(ExecOutput {
            tex: pp.curr.tex,
            fbo: pp.curr.fbo,
            width: pp.curr.w,
            height: pp.curr.h,
        })
    };

    for nid in &plan.nodes {
        let Some(node) = graph.node(*nid) else { continue; };
        if node.kind != NodeKind::PixelsOut {
            continue;
        }
        let Some(name) = props.output_names.get(&node.id) else {
            continue; // explicit-only
        };

        if name == OUTPUT_MAIN {
            return Err(EngineError::other(
                "execute_plan_outputs: output name 'main' is reserved (use a different explicit name)",
            ));
        }

        if named.contains_key(name) {
            return Err(EngineError::other(format!(
                "execute_plan_outputs: duplicate output name '{name}'"
            )));
        }

        let out = resolve_pixels_out(node.id)?;
        named.insert(name.clone(), out);
    }

    Ok(ExecOutputs { primary, named })
}

/// S2: A consumer of the final rendered output for a frame.
///
/// This is intentionally defined in the glow backend first (most surgical).
/// Later (S4) we can lift a backend-agnostic sink interface into `scheng-runtime`
/// once output routing and portability contracts are finalized.
pub trait OutputSink {
    /// Consume the final output produced by `execute_plan` for this frame.
    ///
    /// Sinks should not delete GL resources they did not create.
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput);
}

/// A sink that does nothing (useful as a default during integration).
pub struct NoopSink;

impl OutputSink for NoopSink {
    #[inline]
    fn consume(&mut self, _gl: &glow::Context, _out: &ExecOutput) {}
}

/// S3b: Fan out the same output to two sinks (patch-cable style).
///
/// This is intentionally tiny and composable: you can nest FanoutSink to build trees.
pub struct FanoutSink<A, B> {
    pub a: A,
    pub b: B,
}

impl<A: OutputSink, B: OutputSink> OutputSink for FanoutSink<A, B> {
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput) {
        self.a.consume(gl, out);
        self.b.consume(gl, out);
    }
}

/// S6: Patchbay sink for named output routing.
///
/// This is intentionally minimal: it maps `OutputName` -> `Vec<Box<dyn OutputSink>>` and
/// calls each sink with the resolved output for that name.
///
/// This lives here (runtime-glow) for surgical iteration. Once the contract stabilizes,
/// we can lift the trait to `scheng-runtime` and keep glow/wgpu backends implementing it.
pub struct PatchbaySink {
    routes: HashMap<String, Vec<Box<dyn OutputSink>>>,
}

impl Default for PatchbaySink {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchbaySink {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn add_route<S: OutputSink + 'static>(&mut self, name: impl Into<String>, sink: S) {
        let name = name.into();
        self.routes.entry(name).or_default().push(Box::new(sink));
    }

    pub fn consume_named(
        &mut self,
        gl: &glow::Context,
        outs: &ExecOutputs,
    ) -> Result<(), EngineError> {
        for (name, sinks) in self.routes.iter_mut() {
            let out = outs.get(name).ok_or_else(|| {
                EngineError::other(format!("PatchbaySink: missing named output '{name}'"))
            })?;
            for s in sinks.iter_mut() {
                s.consume(gl, out);
            }
        }
        Ok(())
    }
}

/// S5a: A GPU-only temporal history tap (ring buffer) for video-synth style feedback.
///
/// This stores the last N frames as GL textures (render targets) entirely on the GPU.
pub struct HistoryTapSink {
    frames: Vec<RenderTarget>,
    write_idx: usize,
}

impl HistoryTapSink {
    /// Create a history tap with `len` frames. The actual GPU targets are lazily allocated
    /// on first use (or reallocated on resize).
    pub fn new(len: usize) -> Self {
        Self {
            frames: Vec::with_capacity(len.max(1)),
            write_idx: 0,
        }
    }

    /// Latest captured texture (age 0).
    #[inline]
    pub fn tex_latest(&self) -> Option<glow::NativeTexture> {
        self.tex_at(0)
    }

    /// Texture at a given age: 0 = newest, 1 = previous, ...
    pub fn tex_at(&self, age: usize) -> Option<glow::NativeTexture> {
        if self.frames.is_empty() {
            return None;
        }
        let n = self.frames.len();
        let age = age % n;
        // write_idx points to the slot to write next, so newest is write_idx-1.
        let newest = (self.write_idx + n - 1) % n;
        let idx = (newest + n - age) % n;
        Some(self.frames[idx].tex)
    }

    /// Clears all GPU resources owned by this history tap.
    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        for rt in self.frames.drain(..) {
            gl.delete_framebuffer(rt.fbo);
            gl.delete_texture(rt.tex);
        }
        self.write_idx = 0;
    }

    unsafe fn ensure_allocated(
        &mut self,
        gl: &glow::Context,
        want_len: usize,
        w: i32,
        h: i32,
    ) -> Result<(), EngineError> {
        let want_len = want_len.max(1);

        // Allocate if empty or wrong length.
        if self.frames.len() != want_len {
            for rt in self.frames.drain(..) {
                gl.delete_framebuffer(rt.fbo);
                gl.delete_texture(rt.tex);
            }
            self.frames = Vec::with_capacity(want_len);
            for _ in 0..want_len {
                self.frames.push(create_render_target(gl, w, h)?);
            }
            self.write_idx = 0;
            return Ok(());
        }

        // Recreate on resize mismatch.
        if let Some(rt0) = self.frames.first() {
            if rt0.w != w || rt0.h != h {
                for rt in self.frames.drain(..) {
                    gl.delete_framebuffer(rt.fbo);
                    gl.delete_texture(rt.tex);
                }
                self.frames = Vec::with_capacity(want_len);
                for _ in 0..want_len {
                    self.frames.push(create_render_target(gl, w, h)?);
                }
                self.write_idx = 0;
            }
        }

        Ok(())
    }
}

impl OutputSink for HistoryTapSink {
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput) {
        unsafe {
            let want_len = self.frames.capacity().max(1);
            if let Err(e) = self.ensure_allocated(gl, want_len, out.width, out.height) {
                eprintln!("[HistoryTapSink] ensure_allocated error: {e:?}");
                return;
            }
            if self.frames.is_empty() {
                return;
            }
            let dst = &self.frames[self.write_idx % self.frames.len()];

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(dst.fbo));
            gl.blit_framebuffer(
                0,
                0,
                out.width,
                out.height,
                0,
                0,
                dst.w,
                dst.h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);

            self.write_idx = (self.write_idx + 1) % self.frames.len();
        }
    }
}


/// S6b: A tiny sink that reads back pixels to CPU (debugging / file output later).
///
/// NOTE: This is not optimized. It is intended as a correctness/bring-up tool for routing.
/// S5b: Read back pixels (RGBA8) from the output into CPU memory.
///
/// This performs a GPU->CPU transfer and can stall; use sparingly (snapshots, recording mode,
/// every N frames, etc.).
pub struct ReadbackSink {
    enabled: bool,
    stride: u64,
    frame_counter: u64,
    last_w: i32,
    last_h: i32,
    last_rgba: Vec<u8>,
}

impl ReadbackSink {
    /// `stride` controls how often to read back (1 = every frame, 2 = every other frame, etc.).
    pub fn new(stride: u64) -> Self {
        Self {
            enabled: true,
            stride: stride.max(1),
            frame_counter: 0,
            last_w: 0,
            last_h: 0,
            last_rgba: Vec::new(),
        }
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    #[inline]
    pub fn set_stride(&mut self, stride: u64) {
        self.stride = stride.max(1);
    }

    /// Returns the last captured (w, h, rgba) if any.
    #[inline]
    pub fn last(&self) -> Option<(i32, i32, &[u8])> {
        if self.last_w > 0 && self.last_h > 0 && !self.last_rgba.is_empty() {
            Some((self.last_w, self.last_h, &self.last_rgba))
        } else {
            None
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.last_w = 0;
        self.last_h = 0;
        self.last_rgba.clear();
    }
}

impl Default for ReadbackSink {
    fn default() -> Self {
        Self::new(1)
    }
}

impl OutputSink for ReadbackSink {
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput) {
        if !self.enabled {
            return;
        }
        self.frame_counter += 1;
        if (self.frame_counter % self.stride) != 0 {
            return;
        }

        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out.fbo));
            let mut buf = vec![0u8; (out.width * out.height * 4) as usize];
            gl.read_pixels(
                0,
                0,
                out.width,
                out.height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut buf),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            self.last_w = out.width;
            self.last_h = out.height;
            self.last_rgba = buf;
        }
    }
}




/// S6c: A sink that blits the output into the default framebuffer (screen preview).
pub struct BlitToScreenSink;

impl OutputSink for BlitToScreenSink {
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);

            gl.blit_framebuffer(
                0,
                0,
                out.width,
                out.height,
                0,
                0,
                out.width,
                out.height,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Syphon sink (macOS feature-gated)
// -------------------------------------------------------------------------------------------------

#[cfg(all(feature = "syphon", target_os = "macos"))]
pub struct SyphonSink {
    server: *mut std::ffi::c_void,
    flipped: bool,
}

#[cfg(all(feature = "syphon", target_os = "macos"))]
impl SyphonSink {
    pub fn new(name: &str) -> Result<Self, EngineError> {
        use std::ffi::CString;
        let cname = CString::new(name).map_err(|_| EngineError::Other("invalid syphon name".into()))?;
        let server = unsafe { syphon_server_create(cname.as_ptr()) };
        if server.is_null() {
            return Err(EngineError::Other("Syphon server creation failed".into()));
        }
        Ok(Self {
            server,
            flipped: false,
        })
    }

    pub fn set_flipped(&mut self, flipped: bool) {
        self.flipped = flipped;
    }
}

#[cfg(all(feature = "syphon", target_os = "macos"))]
impl Drop for SyphonSink {
    fn drop(&mut self) {
        unsafe {
            if !self.server.is_null() {
                syphon_server_destroy(self.server);
                self.server = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(all(feature = "syphon", target_os = "macos"))]
impl OutputSink for SyphonSink {
    fn consume(&mut self, _gl: &glow::Context, out: &ExecOutput) {
        unsafe {
            if self.server.is_null() {
                return;
            }
            // glow::NativeTexture is a u32 on OpenGL backends.
            syphon_server_publish_texture(
                self.server,
                out.tex.0.get(),
                out.width,
                out.height,
                self.flipped,
            );
        }
    }
}

#[cfg(all(feature = "syphon", target_os = "macos"))]
extern "C" {
    fn syphon_server_create(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn syphon_server_publish_texture(
        server: *mut std::ffi::c_void,
        tex: u32,
        width: i32,
        height: i32,
        flipped: bool,
    );
    fn syphon_server_destroy(server: *mut std::ffi::c_void);
}

/// Non-macOS stub: keeps builds working if someone enables the feature on another target.
#[cfg(all(feature = "syphon", not(target_os = "macos")))]
pub struct SyphonSink;

#[cfg(all(feature = "syphon", not(target_os = "macos")))]
impl SyphonSink {
    pub fn new(_name: &str) -> Result<Self, EngineError> {
        Err(EngineError::Other("Syphon is only supported on macOS".to_string()))
    }
    pub fn set_flipped(&mut self, _flipped: bool) {}
}

#[cfg(all(feature = "syphon", not(target_os = "macos")))]
impl OutputSink for SyphonSink {
    fn consume(&mut self, _gl: &glow::Context, _out: &ExecOutput) {}
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[allow(dead_code)]
fn find_single_input(plan: &Plan, node: NodeId) -> Option<NodeId> {
    // For v0 we only need: "who drives this node's single input?"
    for e in &plan.edges {
        if e.to.node == node && e.to.dir == PortDir::In {
            return Some(e.from.node);
        }
    }
    None
}

/// Execute a compiled `Plan` for a single frame (pull-based).
///
/// v0 (C3b): supports exactly the minimal chain:
/// `ShaderSource → ShaderPass → PixelsOut`.
///
/// - `ShaderSource` provides fragment source via `props.shader_sources[node_id]`.
/// - `ShaderPass` compiles (cached) and renders into an offscreen `RenderTarget`.
/// - `PixelsOut` returns the final render target handles.
pub unsafe fn execute_plan(
    gl: &glow::Context,
    graph: &Graph,
    plan: &Plan,
    state: &mut RuntimeState,
    props: &NodeProps,
    frame: FrameCtx,
) -> Result<ExecOutput, EngineError> {
    // Pull-based execution v1:
    // - Execute all ShaderPass nodes in plan order.
    // - Route textures along edges (ShaderPass -> ShaderPass / PixelsOut).
    // - Shader sources are resolved either from NodeProps keyed by the ShaderPass node,
    //   or (back-compat) via an incoming edge from a ShaderSource node.

    // Find the (first) PixelsOut node.
    let out_node = plan
        .nodes
        .iter()
        .copied()
        .find(|nid| {
            graph
                .node(*nid)
                .map(|n| n.kind == NodeKind::PixelsOut)
                .unwrap_or(false)
        })
        .ok_or_else(|| EngineError::other("execute_plan: missing PixelsOut node in plan"))?;

    // Helper: find all incoming edges to a node.
    let incoming_edges = |nid: NodeId| -> Vec<&Edge> {
        graph
            .edges()
            .iter()
            .filter(|e| e.to.node == nid && e.to.dir == PortDir::In)
            .collect()
    };

    // Helper: map a node-local input port id to a stable channel index.
    // v0 contract (Option A): the port name defines semantic ordering.
    // - Processor: "in" => channel 0
    // - Mixer: "a" => channel 0, "b" => channel 1
    let port_channel_index = |node_id: NodeId, port: PortId| -> Option<u32> {
        let n = graph.node(node_id)?;
        let p = n
            .ports
            .iter()
            .find(|pp| pp.id == port && pp.dir == PortDir::In)?;

        scheng_runtime::runtime_contract::input_channel_for(n.kind.clone(), p.name)
    };

    // Helper: resolve shader sources for a render-pass node.
    //
    // Resolution order (live-performance friendly):
    // 1) NodeProps override for the pass node (always wins)
    // 2) Built-in standard ops (mixers) via scheng-runtime mapping table
    // 3) Back-compat: incoming edge from a ShaderSource node (props keyed by that node)
    let resolve_shader = |pass_node: NodeId| -> Result<ShaderSource, EngineError> {
        // 1) Direct override
        if let Some(s) = props.shader_sources.get(&pass_node) {
            return Ok(s.clone());
        }

        // 2) Built-in mixers (semantic node kinds)
        let pass = graph
            .node(pass_node)
            .ok_or_else(|| EngineError::other("execute_plan: resolve_shader missing node"))?;

        if let Some(stdop) = standard_op_for(pass.kind.clone()) {
            let StandardOp::Mixer(op) = stdop;
            {
                return Ok(ShaderSource {
                    vert: FULLSCREEN_VERT.to_string(),
                    frag: builtin_mixer_frag(op).to_string(),
                    origin: Some(format!("builtin:{op:?}")),
                });
            }
        }

        // 3) Back-compat: if there's an incoming edge from a ShaderSource node, use that.
        if let Some(e) = incoming_edges(pass_node).into_iter().next() {
            let from = graph
                .node(e.from.node)
                .ok_or_else(|| EngineError::other("execute_plan: edge references missing node"))?;
            if from.kind == NodeKind::ShaderSource {
                return props.shader_sources.get(&from.id).cloned().ok_or_else(|| {
                    EngineError::other("execute_plan: missing ShaderSource in NodeProps")
                });
            }
        }

        Err(EngineError::other(
            "execute_plan: missing shader source (provide NodeProps for pass node, or connect ShaderSource -> ShaderPass)",
        ))
    };

    // We'll store outputs for pass nodes here.
    // (We use the RenderTarget cache in RuntimeState; this map is just for quick lookup.)
    use std::collections::HashMap;
    let mut outputs: HashMap<NodeId, (glow::NativeTexture, glow::NativeFramebuffer, i32, i32)> =
        HashMap::new();

    // Source outputs (Step 11.1): textures produced by Source-class nodes (e.g., TextureInputPass).
    // These do not allocate render targets and do not run shaders.
    let mut source_outputs: HashMap<NodeId, (glow::NativeTexture, i32, i32)> = HashMap::new();

    // Execute passes in plan order.
    for nid in &plan.nodes {
        let node = graph
            .node(*nid)
            .ok_or_else(|| EngineError::other("execute_plan: plan references missing node"))?;
        // Step 11.1: Source nodes are resolved without rendering.
        if node.kind == NodeKind::TextureInputPass {
            let tex = *props.texture_inputs.get(&node.id).ok_or_else(|| {
                EngineError::Other(format!("TextureInputPass missing host texture for node {node:?}"))
            })?;
            // v0: assume source texture is frame-sized (host is responsible for providing a matching size).
            source_outputs.insert(node.id, (tex, frame.width, frame.height));
            continue;
        }


        if node.kind == NodeKind::VideoDecodeSource {
            // Engine-integrated video decode: ffmpeg -> RGBA -> host texture.
            let vn = if let Some(vn) = state.video_nodes.get_mut(&node.id) {
                vn
            } else {
                // Resolve configuration for this node.
                let dec = if let Some(p) = props.video_decode_json.get(&node.id) {
                    input_video::VideoDecoder::from_json_path(p)
                        .map_err(|e| EngineError::Other(format!("video decode (json): {e}")))?
                } else if let Some(cfg) = props.video_decode_cfg.get(&node.id) {
                        input_video::VideoDecoder::from_config(cfg.clone())
                        .map_err(|e| EngineError::Other(format!("video decode (cfg): {e}")))?
                } else {
                    return Err(EngineError::Other(
                        "VideoDecodeSource node requires NodeProps.video_decode_json or video_decode_cfg entry".to_string(),
                    ));
                };

                // Resolve video dimensions and fps from the decoder config once.
                let cfg = dec.config().clone();
                let w = cfg.width as i32;
                let h = cfg.height as i32;
                let tex = create_host_texture(gl, w, h);
                // Clamp to at least 1.0 to avoid division by zero if someone passes 0.
                let fps = cfg.fps.max(1) as f32;

                state.video_nodes.insert(
                    node.id,
                    VideoNodeState {
                        dec,
                        tex,
                        w,
                        h,
                        fps,
                        // No frame uploaded yet.
                        last_frame_index: -1,
                    },
                );
                state.video_nodes.get_mut(&node.id).unwrap()
            };

// Map the engine's timeline into a nominal video-frame index.
// This lets the app's FrameCtx::time (driven by keyboard transport)
// control when we pick up a new decoded frame.
let timeline_time = frame.time.max(0.0);
let timeline_index = if vn.fps > 0.0 {
    (timeline_time * vn.fps).floor() as i64
} else {
    -1
};

        // Only sample a new decoded frame when the timeline advances past
        // the last index we uploaded. If time is paused (no change in
        // FrameCtx::time), this keeps the texture frozen (visual pause).
        if timeline_index < 0 || timeline_index > vn.last_frame_index {
            if let Ok(vf) = vn.dec.poll_rgba() {
                if vf.width as i32 != vn.w || vf.height as i32 != vn.h {
                    // Resolution changed (rare). Reallocate texture.
                    gl.delete_texture(vn.tex);
                    vn.w = vf.width as i32;
                    vn.h = vf.height as i32;
                    vn.tex = create_host_texture(gl, vn.w, vn.h);
                }

                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(vn.tex));
                    gl.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        0,
                        vn.w,
                        vn.h,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(&vf.bytes),
                    );
                    gl.bind_texture(glow::TEXTURE_2D, None);
                }

                vn.last_frame_index = timeline_index.max(0);
            }
        }


            source_outputs.insert(node.id, (vn.tex, vn.w, vn.h));
            continue;
        }

        let is_render_pass =
            node.kind == NodeKind::ShaderPass || node.kind.class() == NodeClass::Mixer;
        if !is_render_pass {
            continue;
        }


        // Ensure ping-pong targets exist for this node and match frame size.
        // We use `pp.curr` as the "previous frame" texture for the `history` input.
        let history_tex: glow::NativeTexture = {
            if let std::collections::hash_map::Entry::Vacant(e) = state.targets.entry(node.id) {
                let curr = create_render_target(gl, frame.width, frame.height)?;
                let prev = create_render_target(gl, frame.width, frame.height)?;
                e.insert(PingPong { curr, prev });
            }
            let pp = state
                .targets
                .get_mut(&node.id)
                .expect("just inserted ping-pong targets");
            pp.ensure_size(gl, frame.width, frame.height);
            pp.curr.tex
        };

        // Determine input textures via incoming edges (Option A): bind by port semantics.
        // - Processor "in" => channel 0
        // - Mixer "a" => channel 0, "b" => channel 1
        // - Upstream must be a render-pass node (ShaderPass or Mixer) to contribute a texture.
        let mut inputs: Vec<(u32, glow::NativeTexture)> = Vec::new();
        for e in incoming_edges(node.id) {
            // Only map known input ports.
            let Some(ch) = port_channel_index(node.id, e.to.port) else {
                continue;
            };

            // Feedback: if this edge targets the `history` input, bind previous-frame texture for this node.
            if let Some(n) = graph.node(node.id) {
                if let Some(p) = n.ports.iter().find(|p| p.id == e.to.port && p.dir == PortDir::In)
                {
                    if p.name == "history" {
                        inputs.push((ch, history_tex));
                        continue;
                    }
                }
            }

            let from_node = graph
                .node(e.from.node)
                .ok_or_else(|| EngineError::other("execute_plan: edge references missing node"))?;
            let from_is_render_pass =
                from_node.kind == NodeKind::ShaderPass || from_node.kind.class() == NodeClass::Mixer;

            if from_is_render_pass {
                let tex = if let Some((t, _f, _w, _h)) = outputs.get(&from_node.id) {
                    *t
                } else if let Some(pp) = state.targets.get(&from_node.id) {
                    pp.curr.tex
                } else {
                    continue;
                };
                inputs.push((ch, tex));
                continue;
            }

            // Step 11.1: TextureInputPass is a Source node that provides a texture directly.
            if from_node.kind == NodeKind::TextureInputPass {
                let (tex, _w, _h) = source_outputs
                    .get(&from_node.id)
                    .copied()
                    .or_else(|| props.texture_inputs.get(&from_node.id).copied().map(|t| (t, frame.width, frame.height)))
                    .ok_or_else(|| EngineError::other("TextureInputPass missing source texture"))?;
                inputs.push((ch, tex));
                continue;
            }

        // Engine-integrated VideoDecodeSource is also a Source node, backed by an engine-managed host texture.
        if from_node.kind == NodeKind::VideoDecodeSource {
            let (tex, _w, _h) = source_outputs
                .get(&from_node.id)
                .copied()
                .ok_or_else(|| EngineError::other("VideoDecodeSource missing decoded texture"))?;
            inputs.push((ch, tex));
            continue;
        }
            // ShaderSource edges are allowed only for shader resolution; they don't produce textures.
            continue;
        }
        // Ensure deterministic binding order.
        inputs.sort_by_key(|(ch, _)| *ch);
        // Ensure program cached and up-to-date (shared across nodes).
        let shader = resolve_shader(node.id)?;
        let key = ProgramKey {
            vert_hash: hash_str(&shader.vert),
            frag_hash: hash_str(&shader.frag),
        };

        let cached_prog = if let Some(p) = state.program_cache.get(&key) {
            *p
        } else {
            let p = compile_program(gl, &shader.vert, &shader.frag)?;
            state.program_cache.insert(key, p);
            p
        };

        let needs_rebind = match state.programs.get(&node.id) {
            Some(stored) => stored.key != key,
            None => true,
        };
        if needs_rebind {
            state.programs.insert(
                node.id,
                ProgramEntry {
                    program: cached_prog,
                    key,
                },
            );
        }

        let prog = state
            .programs
            .get(&node.id)
            .ok_or_else(|| EngineError::other("execute_plan: program missing after build"))?
            .program;

        // Ensure render target cached for this pass.
        // (RenderTarget construction can fail, so we can't use or_insert_with here.)
        let pp = state
            .targets
            .get_mut(&node.id)
            .expect("ping-pong targets exist");
        pp.swap();
        let tgt = &pp.curr;

        // Render.
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(tgt.fbo));
        gl.viewport(0, 0, tgt.w, tgt.h);
        gl.disable(glow::DEPTH_TEST);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.use_program(Some(prog));

        // Standard-op uniforms (if this node maps to a standard op).
        if let Some(StandardOp::Mixer(op)) = standard_op_for(node.kind.clone()) {
            match op {
                MixerOp::Crossfade => {
                    let p = props.mixer_params.get(&node.id).copied().unwrap_or_default();
                    if let Some(loc) = gl.get_uniform_location(prog, "uMix") {
                        gl.uniform_1_f32(Some(&loc), p.mix);
                    }
                }
                MixerOp::MatrixMix4 => {
                    let p = props.matrix_params.get(&node.id).copied().unwrap_or_default();
                    if let Some(loc) = gl.get_uniform_location(prog, "uWeights") {
                        gl.uniform_4_f32(
                            Some(&loc),
                            p.weights[0],
                            p.weights[1],
                            p.weights[2],
                            p.weights[3],
                        );
                    }
                }
                _ => {}
            }
        }

        // Common uniforms (set if present).
        if let Some(loc) = gl.get_uniform_location(prog, "uTime") {
            gl.uniform_1_f32(Some(&loc), frame.time);
        }
        if let Some(loc) = gl.get_uniform_location(prog, "uResolution") {
            gl.uniform_2_f32(Some(&loc), frame.width as f32, frame.height as f32);
        }
        if let Some(loc) = gl.get_uniform_location(prog, "u_time") {
            gl.uniform_1_f32(Some(&loc), frame.time);
        }
        if let Some(loc) = gl.get_uniform_location(prog, "u_resolution") {
            gl.uniform_2_f32(Some(&loc), frame.width as f32, frame.height as f32);
        }
        // Shadertoy-ish names (optional).
        if let Some(loc) = gl.get_uniform_location(prog, "iTime") {
            gl.uniform_1_f32(Some(&loc), frame.time);
        }
        if let Some(loc) = gl.get_uniform_location(prog, "iResolution") {
            gl.uniform_3_f32(Some(&loc), frame.width as f32, frame.height as f32, 1.0);
        }

        // Bind input textures by semantic port order (Option A).
        for (ch, tex) in &inputs {
            let unit = *ch;
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, Some(*tex));

            let candidates = [
                format!("iChannel{unit}"),
                format!("uInput{unit}"),
                format!("uTex{unit}"),
                format!("u_tex{unit}"),
            ];
            for name in candidates {
                if let Some(loc) = gl.get_uniform_location(prog, &name) {
                    gl.uniform_1_i32(Some(&loc), unit as i32);
                }
            }
        }

        state.fs_tri.draw(gl);

        // Record output.
        outputs.insert(node.id, (tgt.tex, tgt.fbo, tgt.w, tgt.h));
    }

    // Resolve final output texture from PixelsOut's incoming edge.
    let out_edge = graph
        .edges()
        .iter()
        .find(|e| e.to.node == out_node && e.to.dir == PortDir::In)
        .ok_or_else(|| EngineError::other("execute_plan: PixelsOut has no input edge"))?;

    let from_node = graph
        .node(out_edge.from.node)
        .ok_or_else(|| EngineError::other("execute_plan: output edge references missing node"))?;

    let from_is_render_pass =
        from_node.kind == NodeKind::ShaderPass || from_node.kind.class() == NodeClass::Mixer;
    if !from_is_render_pass {
        return Err(EngineError::other(
            "execute_plan: PixelsOut input must come from a render pass (ShaderPass or Mixer) in v1",
        ));
    }

    let (tex, fbo, w, h) = outputs
        .get(&from_node.id)
        .copied()
        .or_else(|| {
            state
                .targets
                .get(&from_node.id)
                .map(|pp| (pp.curr.tex, pp.curr.fbo, pp.curr.w, pp.curr.h))
        })
        .ok_or_else(|| EngineError::other("execute_plan: missing output texture for final pass"))?;

    Ok(ExecOutput {
        tex,
        fbo,
        width: w,
        height: h,
    })
}

/// S2: Execute a frame and immediately route the final output into a sink.
pub unsafe fn execute_plan_to_sink<S: OutputSink>(
    gl: &glow::Context,
    graph: &Graph,
    plan: &Plan,
    state: &mut RuntimeState,
    props: &NodeProps,
    frame: FrameCtx,
    sink: &mut S,
) -> Result<ExecOutput, EngineError> {
    let out = execute_plan(gl, graph, plan, state, props, frame)?;
    sink.consume(gl, &out);
    Ok(out)
}

// --- PASS2: Fullscreen draw helper ---
#[derive(Debug)]
pub struct FullscreenTriangle {
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
}

impl FullscreenTriangle {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, EngineError> {
        let verts: [f32; 12] = [
            -1.0, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
        ];

        let vao = gl
            .create_vertex_array()
            .map_err(|e| EngineError::GlCreate(format!("create_vertex_array: {e}")))?;
        let vbo = gl
            .create_buffer()
            .map_err(|e| EngineError::GlCreate(format!("create_buffer: {e}")))?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));

        let bytes = core::slice::from_raw_parts(
            verts.as_ptr() as *const u8,
            verts.len() * core::mem::size_of::<f32>(),
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);

        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 4 * 4, 0);

        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 4 * 4, 2 * 4);

        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_vertex_array(None);

        Ok(Self { vao, vbo })
    }

    pub unsafe fn draw(&self, gl: &glow::Context) {
        gl.bind_vertex_array(Some(self.vao));
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        gl.bind_vertex_array(None);
    }

    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        gl.delete_vertex_array(self.vao);
        gl.delete_buffer(self.vbo);
    }
}

pub const FULLSCREEN_VERT: &str = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

pub const TEX_INPUT_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main(){ o = texture(iChannel0, v_uv); }
"#;


pub fn builtin_mixer_frag(op: MixerOp) -> &'static str {
    match op {
        MixerOp::Crossfade => CROSSFADE_FRAG,
        MixerOp::MatrixMix4 => MATRIXMIX4_FRAG,
        _ => CROSSFADE_FRAG,
    }
}

pub const CROSSFADE_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 FragColor;

uniform sampler2D uInput0;
uniform sampler2D uInput1;
uniform float uMix;

void main() {
    vec4 a = texture(uInput0, v_uv);
    vec4 b = texture(uInput1, v_uv);
    FragColor = mix(a, b, uMix);
}
"#;

pub const MATRIXMIX4_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 FragColor;

uniform sampler2D uInput0;
uniform sampler2D uInput1;
uniform sampler2D uInput2;
uniform sampler2D uInput3;
uniform vec4 uWeights;

void main() {
    vec4 a = texture(uInput0, v_uv);
    vec4 b = texture(uInput1, v_uv);
    vec4 c = texture(uInput2, v_uv);
    vec4 d = texture(uInput3, v_uv);
    FragColor = a * uWeights.x + b * uWeights.y + c * uWeights.z + d * uWeights.w;
}
"#;

// -------------------------------------------------------------------------------------------------
// Misc helpers used by examples (keep small + stable)
// -------------------------------------------------------------------------------------------------

/// Blit `src` into `dst` using a simple fullscreen draw (no filtering).
///
/// This is a helper for hosts that want to display a render target at an arbitrary viewport
/// without relying on glBlitFramebuffer semantics.
pub unsafe fn blit_fullscreen(
    gl: &glow::Context,
    fs_tri: &FullscreenTriangle,
    program: glow::NativeProgram,
    target: &RenderTarget,
    viewport_w: i32,
    viewport_h: i32,
) -> Result<(), EngineError> {
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
    gl.viewport(0, 0, viewport_w, viewport_h);

    gl.use_program(Some(program));
    fs_tri.draw(gl);

    gl.use_program(None);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    Ok(())
}
