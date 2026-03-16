//! `worker.rs` — background thread that feeds frames to ffmpeg's stdin.
//!
//! The render thread sends raw RGBA frames via a bounded channel.
//! This thread receives them and writes to ffmpeg's stdin pipe.
//!
//! Design principle from shadecore:
//! "When the system is overloaded, prefer dropping frames in recording
//!  rather than freezing the preview."

use std::{
    io::Write,
    process::{Child, ChildStdin, Command, Stdio},
    thread::{self, JoinHandle},
};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use crate::{FfmpegConfig, FfmpegError};

/// A raw RGBA frame ready to send to ffmpeg.
pub struct RawFrame {
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
}

/// Owns the ffmpeg process and the channel to the worker thread.
pub struct FfmpegWorker {
    sender:  Sender<RawFrame>,
    handle:  Option<JoinHandle<()>>,
    process: Option<Child>,
    dropped: u64,
    sent:    u64,
}

impl FfmpegWorker {
    /// Spawn the ffmpeg process and start the worker thread.
    pub fn start(config: &FfmpegConfig) -> Result<Self, FfmpegError> {
        config.validate()?;

        let args = config.build_args();
        log::info!("Starting ffmpeg: {} {}", config.ffmpeg_path, args.join(" "));

        let mut child = Command::new(&config.ffmpeg_path)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit()) // ffmpeg logs to stderr — let it through
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FfmpegError::NotFound { path: config.ffmpeg_path.clone() }
                } else {
                    FfmpegError::SpawnFailed(e)
                }
            })?;

        let stdin: ChildStdin = child.stdin.take()
            .expect("ffmpeg stdin handle missing — this is a bug");

        let (sender, receiver) = bounded::<RawFrame>(config.queue_depth);

        let handle = thread::Builder::new()
            .name("scheng-ffmpeg-worker".into())
            .spawn(move || run_worker(stdin, receiver))
            .map_err(FfmpegError::SpawnFailed)?;

        log::info!("ffmpeg worker started (queue depth: {})", config.queue_depth);

        Ok(Self {
            sender,
            handle: Some(handle),
            process: Some(child),
            dropped: 0,
            sent: 0,
        })
    }

    /// Send a frame to the worker. Non-blocking — drops the frame if the
    /// channel is full rather than stalling the render loop.
    pub fn send_frame(&mut self, frame: RawFrame) {
        match self.sender.try_send(frame) {
            Ok(()) => {
                self.sent += 1;
            }
            Err(TrySendError::Full(_)) => {
                self.dropped += 1;
                if self.dropped % 60 == 1 {
                    log::warn!(
                        "ffmpeg worker falling behind — {} frames dropped so far \
                         (sent: {}). Consider lowering framerate or bitrate.",
                        self.dropped, self.sent
                    );
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                log::error!("ffmpeg worker thread disconnected unexpectedly");
            }
        }
    }

    /// Gracefully stop the worker thread and ffmpeg process.
    pub fn stop(&mut self) {
        // Drop the sender — worker thread's recv() will return Err and exit.
        // We do this by replacing with a disconnected channel.
        let (dead_sender, _) = bounded::<RawFrame>(1);
        let _ = std::mem::replace(&mut self.sender, dead_sender);

        // Wait for the worker thread to drain and exit.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        // Wait for ffmpeg to finish encoding and exit cleanly.
        if let Some(mut child) = self.process.take() {
            let _ = child.wait();
        }

        log::info!(
            "ffmpeg worker stopped — frames sent: {}, dropped: {}",
            self.sent, self.dropped
        );
    }

    pub fn frames_sent(&self)    -> u64 { self.sent }
    pub fn frames_dropped(&self) -> u64 { self.dropped }
}

impl Drop for FfmpegWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Worker thread ─────────────────────────────────────────────────────────

fn run_worker(mut stdin: ChildStdin, receiver: Receiver<RawFrame>) {
    log::debug!("ffmpeg worker thread running");

    for frame in receiver.iter() {
        // Write the full raw RGBA frame to ffmpeg's stdin.
        // Expected size: width * height * 4 bytes.
        let expected = (frame.width * frame.height * 4) as usize;
        if frame.pixels.len() != expected {
            log::error!(
                "ffmpeg worker: frame size mismatch — got {} bytes, expected {} ({}×{}×4)",
                frame.pixels.len(), expected, frame.width, frame.height
            );
            continue;
        }

        if let Err(e) = stdin.write_all(&frame.pixels) {
            // ffmpeg closed its stdin (process ended or errored).
            log::error!("ffmpeg stdin write failed: {e}");
            break;
        }
    }

    // Flush any buffered data before exiting.
    let _ = stdin.flush();
    log::debug!("ffmpeg worker thread exiting");
}
