//! Seekable temporary storage for reusable PCM. Hosts can supply file or OPFS adapters.
use crate::{
    audio_io::{Result, Stereo},
    dsp,
    finishing::PeakStats,
};
use soundkit::audio_pipeline::f32s_to_le_bytes;
use std::io::{Read, Seek, SeekFrom, Write};

pub trait Scratch: Read + Write + Seek {}
impl<T: Read + Write + Seek> Scratch for T {}
/// Each call must return independent, empty storage positioned at byte zero.
/// The handle owns cleanup. Hosts may use disk, OPFS, or memory.
pub type Factory = Box<dyn FnMut() -> Result<Box<dyn Scratch>>>;

pub fn memory() -> Result<Box<dyn Scratch>> {
    Ok(Box::new(std::io::Cursor::new(Vec::<u8>::new())))
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The storage of `temporary` on wasm32 (`set_host`).
    static HOST: std::cell::RefCell<Option<Factory>> = const { std::cell::RefCell::new(None) };
}

/// On wasm32, make `temporary` take its storage from `factory`, the storage
/// of the host, or from memory with `None`. A 4x prefix of a 5-minute
/// recording is 460 MB, and the linear memory of a module never shrinks.
#[cfg(target_arch = "wasm32")]
pub fn set_host(factory: Option<Factory>) {
    HOST.with(|host| *host.borrow_mut() = factory);
}

#[cfg(target_arch = "wasm32")]
pub fn temporary() -> Result<Box<dyn Scratch>> {
    HOST.with(|host| match host.borrow_mut().as_mut() {
        Some(factory) => factory(),
        None => memory(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn temporary() -> Result<Box<dyn Scratch>> {
    use std::{
        fs::OpenOptions,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..1000 {
        let path = std::env::temp_dir().join(format!(
            "mastering-pcm-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => {
                #[cfg(unix)]
                {
                    std::fs::remove_file(path)?;
                    return Ok(Box::new(file));
                }
                #[cfg(not(unix))]
                {
                    return Ok(Box::new(TemporaryFile {
                        file: Some(file),
                        path,
                    }));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err("Cannot allocate a unique PCM scratch file".into())
}

#[cfg(all(not(unix), not(target_arch = "wasm32")))]
struct TemporaryFile {
    file: Option<std::fs::File>,
    path: std::path::PathBuf,
}
#[cfg(all(not(unix), not(target_arch = "wasm32")))]
impl Read for TemporaryFile {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.file.as_mut().unwrap().read(b)
    }
}
#[cfg(all(not(unix), not(target_arch = "wasm32")))]
impl Write for TemporaryFile {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.file.as_mut().unwrap().write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.as_mut().unwrap().flush()
    }
}
#[cfg(all(not(unix), not(target_arch = "wasm32")))]
impl Seek for TemporaryFile {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.file.as_mut().unwrap().seek(p)
    }
}
#[cfg(all(not(unix), not(target_arch = "wasm32")))]
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) trait Frames {
    fn len(&self) -> usize;
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()>;
}

pub(crate) struct Pcm {
    io: Box<dyn Scratch>,
    frames: usize,
}
impl Pcm {
    pub(crate) fn new(io: Box<dyn Scratch>) -> Self {
        Self { io, frames: 0 }
    }
    pub(crate) fn push(&mut self, audio: &[Stereo]) -> Result<()> {
        let flat: Vec<_> = audio.iter().flatten().copied().collect();
        self.io.write_all(&f32s_to_le_bytes(&flat))?;
        self.frames += audio.len();
        Ok(())
    }
    pub(crate) fn color(&mut self, amount: f64, peak: f64) -> Result<PeakStats> {
        if !(0.0..=0.05).contains(&amount) {
            return Err("Invalid harmonic color input".into());
        }
        let mut stats = PeakStats::default();
        if amount == 0.0 || peak == 0.0 {
            return Ok(stats);
        }
        let mut affected = 0usize;
        let mut block = Vec::new();
        for start in (0..self.frames).step_by(16_384) {
            self.read(start, 16_384.min(self.frames - start), &mut block)?;
            for frame in &mut block {
                let mut changed = false;
                for x in frame {
                    let factor = 1.0 - amount * (*x as f64 / peak).powi(2);
                    stats.maximum_reduction_db = stats.maximum_reduction_db.max(-dsp::db(factor));
                    changed |= factor < 1.0 - 1e-6;
                    *x = (*x as f64 * factor) as f32;
                }
                affected += usize::from(changed);
            }
            self.io.seek(SeekFrom::Start(start as u64 * 8))?;
            let flat: Vec<_> = block.iter().flatten().copied().collect();
            self.io.write_all(&f32s_to_le_bytes(&flat))?;
        }
        stats.affected_frames_percent = 100.0 * affected as f64 / self.frames.max(1) as f64;
        Ok(stats)
    }
}
impl Frames for Pcm {
    fn len(&self) -> usize {
        self.frames
    }
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        if start > self.frames || count > self.frames - start {
            return Err("PCM scratch read exceeds frame count".into());
        }
        self.io.seek(SeekFrom::Start(start as u64 * 8))?;
        // Read in pieces: one 4x segment is 185 MB, and a kept buffer of
        // that size stays with each cached prefix.
        const PIECE: usize = 65_536;
        let mut bytes = vec![0u8; count.min(PIECE) * 8];
        output.clear();
        output.reserve(count);
        let mut remaining = count;
        while remaining > 0 {
            let piece = &mut bytes[..remaining.min(PIECE) * 8];
            self.io.read_exact(piece)?;
            output.extend(piece.as_chunks::<8>().0.iter().map(|x| {
                [
                    f32::from_le_bytes([x[0], x[1], x[2], x[3]]),
                    f32::from_le_bytes([x[4], x[5], x[6], x[7]]),
                ]
            }));
            remaining -= piece.len() / 8;
        }
        Ok(())
    }
}

/// The `len` frames of `frames` from frame `start`, read from `frames` in
/// place.
pub(crate) struct Window<'a, F: Frames> {
    pub(crate) frames: &'a mut F,
    pub(crate) start: usize,
    pub(crate) len: usize,
}
impl<F: Frames> Frames for Window<'_, F> {
    fn len(&self) -> usize {
        self.len
    }
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        if start > self.len || count > self.len - start {
            return Err("Window read exceeds its frame count".into());
        }
        self.frames.read(self.start + start, count, output)
    }
}

pub(crate) struct Slice<'a>(pub &'a [Stereo]);
impl Frames for Slice<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        output.clear();
        output.extend_from_slice(&self.0[start..start + count]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_preserves_bits_and_global_color_across_blocks() {
        let mut audio: Vec<_> = (0..40_123)
            .map(|i| [(i as f32 * 0.013).sin(), (i as f32 * 0.017).cos() * 0.7])
            .collect();
        audio[0] = [-0.0, f32::from_bits(1)];
        audio[16_384] = [1.7, -2.3];
        for factory in [memory, temporary] {
            for amount in [0.0, 0.013, 0.05] {
                let mut pcm = Pcm::new(factory().unwrap());
                for block in audio.chunks(997) {
                    pcm.push(block).unwrap();
                }
                let mut got = Vec::new();
                for (start, count) in [(16_381, 23), (40_120, 3), (0, 11), (40_123, 0)] {
                    pcm.read(start, count, &mut got).unwrap();
                    assert_eq!(
                        got.iter()
                            .flatten()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>(),
                        audio[start..start + count]
                            .iter()
                            .flatten()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>()
                    );
                }
                assert!(pcm.read(40_122, 2, &mut got).is_err());
                let peak = audio
                    .iter()
                    .flatten()
                    .map(|x| x.abs() as f64)
                    .fold(0.0, f64::max);
                let actual = pcm.color(amount, peak).unwrap();
                let mut expected = audio.clone();
                let expected_stats = crate::finishing::color(&mut expected, amount).unwrap();
                pcm.read(0, audio.len(), &mut got).unwrap();
                assert_eq!(
                    got.iter()
                        .flatten()
                        .map(|x| x.to_bits())
                        .collect::<Vec<_>>(),
                    expected
                        .iter()
                        .flatten()
                        .map(|x| x.to_bits())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    serde_json::to_value(expected_stats).unwrap()
                );
            }
        }
    }
}
