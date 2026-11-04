//! Seekable temporary storage for reusable PCM. Hosts can supply file or OPFS adapters.
use crate::{
    audio_io::{Result, Stereo},
    dsp,
    finishing::PeakStats,
};
use std::io::{Read, Seek, SeekFrom, Write};

/// Storage of a render can move to the thread that delivers it (D253).
pub trait Scratch: Read + Write + Seek + Send {}
impl<T: Read + Write + Seek + Send> Scratch for T {}
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

    /// The `count` frames from frame `start`: frames in memory in place,
    /// and other frames read into `buffer`.
    fn window<'b>(
        &'b mut self,
        start: usize,
        count: usize,
        buffer: &'b mut Vec<Stereo>,
    ) -> Result<&'b [Stereo]> {
        self.read(start, count, buffer)?;
        Ok(buffer)
    }
}

/// Each block of the frames of `audio` to `each`, in order, in blocks of
/// 65,536 frames.
pub(crate) fn for_each_block(
    audio: &mut impl Frames,
    mut each: impl FnMut(&[Stereo]) -> Result<()>,
) -> Result<()> {
    const BLOCK: usize = 65_536;
    let frames = audio.len();
    let mut buffer = Vec::new();
    for start in (0..frames).step_by(BLOCK) {
        each(audio.window(start, BLOCK.min(frames - start), &mut buffer)?)?;
    }
    Ok(())
}

/// Each sample of the frames of `audio` is finite.
pub(crate) fn all_finite(audio: &mut impl Frames) -> Result<bool> {
    let mut finite = true;
    for_each_block(audio, |block| {
        finite &= block.iter().flatten().all(|x| x.is_finite());
        Ok(())
    })?;
    Ok(finite)
}

/// The bytes of `inner` from byte `offset`: the frames of a file after its
/// header (D259).
pub(crate) struct Offset<S> {
    pub(crate) inner: S,
    pub(crate) offset: u64,
}
impl<S: Read> Read for Offset<S> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(bytes)
    }
}
impl<S: Write> Write for Offset<S> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
impl<S: Seek> Seek for Offset<S> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let position = match position {
            SeekFrom::Start(at) => SeekFrom::Start(self.offset + at),
            other => other,
        };
        self.inner
            .seek(position)?
            .checked_sub(self.offset)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek before the frames")
            })
    }
}

pub(crate) struct Pcm {
    io: Box<dyn Scratch>,
    frames: usize,
}
impl Pcm {
    pub(crate) fn new(io: Box<dyn Scratch>) -> Self {
        Self { io, frames: 0 }
    }
    /// The `frames` frames that `io` holds from its byte zero.
    pub(crate) fn holding(io: Box<dyn Scratch>, frames: usize) -> Self {
        Self { io, frames }
    }
    /// `audio` in new storage of `temporary`, written in blocks.
    pub(crate) fn temporary_of(audio: &[Stereo]) -> Result<Self> {
        let mut pcm = Self::new(temporary()?);
        for block in audio.chunks(65_536) {
            pcm.push(block)?;
        }
        Ok(pcm)
    }
    /// Store `audio` after the stored frames.
    pub(crate) fn push(&mut self, audio: &[Stereo]) -> Result<()> {
        // A read or a write moves the position of the storage.
        self.io.seek(SeekFrom::Start(self.frames as u64 * 8))?;
        self.write(audio)?;
        self.frames += audio.len();
        Ok(())
    }
    /// Add `audio` to the stored frames from frame `start`, and store the
    /// frames of `audio` after the stored frames. The sum of the stems
    /// takes each stem in blocks as the stem decodes.
    pub(crate) fn add_at(&mut self, start: usize, audio: &[Stereo]) -> Result<()> {
        if start > self.frames {
            return Err("PCM scratch addition starts after the stored frames".into());
        }
        let stored = audio.len().min(self.frames - start);
        let mut block = Vec::new();
        self.read(start, stored, &mut block)?;
        for (total, frame) in block.iter_mut().zip(audio) {
            for channel in 0..2 {
                total[channel] += frame[channel];
            }
        }
        block.extend_from_slice(&audio[stored..]);
        self.io.seek(SeekFrom::Start(start as u64 * 8))?;
        self.write(&block)?;
        self.frames = self.frames.max(start + audio.len());
        Ok(())
    }
    /// Replace the stored frames from frame `start` with `audio`.
    pub(crate) fn write_at(&mut self, start: usize, audio: &[Stereo]) -> Result<()> {
        if start > self.frames || audio.len() > self.frames - start {
            return Err("PCM scratch write exceeds frame count".into());
        }
        self.io.seek(SeekFrom::Start(start as u64 * 8))?;
        self.write(audio)
    }
    /// Write `audio` at the position of the storage, in pieces of 65,536
    /// frames: a batch of segments of peak control is 46 MB at 48 kHz, and
    /// a copy of the whole batch would double it.
    fn write(&mut self, audio: &[Stereo]) -> Result<()> {
        const PIECE: usize = 65_536;
        let mut bytes = vec![0u8; audio.len().min(PIECE) * 8];
        for piece in audio.chunks(PIECE) {
            let bytes = &mut bytes[..piece.len() * 8];
            for (target, x) in bytes
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(piece.as_flattened())
            {
                target.copy_from_slice(&x.to_le_bytes());
            }
            self.io.write_all(bytes)?;
        }
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
            self.write(&block)?;
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
        output.clear();
        // A span with its warm-up is a little longer than the span before
        // it, and `reserve` would double the buffer for it. The output takes
        // its room before the pieces: in the browser engine, a piece can
        // take part of the freed buffer that the output would fill, and the
        // output then grows the linear memory by one buffer (D245).
        output.reserve_exact(count);
        // Read in pieces: one 4x segment is 185 MB, and a kept buffer of
        // that size stays with each cached prefix.
        const PIECE: usize = 65_536;
        let mut bytes = vec![0u8; count.min(PIECE) * 8];
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

    fn window<'b>(
        &'b mut self,
        start: usize,
        count: usize,
        _: &'b mut Vec<Stereo>,
    ) -> Result<&'b [Stereo]> {
        Ok(&self.0[start..start + count])
    }
}

/// Frames in memory, or in the scratch file that `P` gives: a render or a
/// source in either place (D253, D254).
pub(crate) enum Place<'a, P> {
    Memory(Slice<'a>),
    File(P),
}
impl<P: std::ops::DerefMut<Target = Pcm>> Frames for Place<'_, P> {
    fn len(&self) -> usize {
        match self {
            Place::Memory(audio) => audio.len(),
            Place::File(pcm) => pcm.len(),
        }
    }

    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        match self {
            Place::Memory(audio) => audio.read(start, count, output),
            Place::File(pcm) => pcm.read(start, count, output),
        }
    }

    fn window<'b>(
        &'b mut self,
        start: usize,
        count: usize,
        buffer: &'b mut Vec<Stereo>,
    ) -> Result<&'b [Stereo]> {
        match self {
            Place::Memory(audio) => audio.window(start, count, buffer),
            Place::File(pcm) => pcm.window(start, count, buffer),
        }
    }
}

/// Frames that several readers take in turn: each read borrows the frames
/// (D251).
#[derive(Clone, Copy)]
pub(crate) struct Shared<'a>(pub(crate) &'a std::cell::RefCell<dyn Frames + 'a>);
impl Frames for Shared<'_> {
    fn len(&self) -> usize {
        self.0.borrow().len()
    }
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        self.0.borrow_mut().read(start, count, output)
    }
}

/// A read of `count` frames from frame `start` of the frames of a `serve`
/// call, into `buffer`.
pub(crate) struct Request {
    start: usize,
    count: usize,
    buffer: Vec<Stereo>,
    reply: std::sync::mpsc::Sender<std::result::Result<Vec<Stereo>, String>>,
}

/// The `len` frames from frame `start` of the frames of a `serve` call on
/// another thread. After each read, a request asks for the next block of
/// the same length, so a reader in order waits only for its first block.
pub(crate) struct Remote {
    start: usize,
    len: usize,
    requests: std::sync::mpsc::Sender<Request>,
    reply: std::sync::mpsc::Sender<std::result::Result<Vec<Stereo>, String>>,
    replies: std::sync::mpsc::Receiver<std::result::Result<Vec<Stereo>, String>>,
    /// The start and the count of the read that the last request asked for.
    next: Option<(usize, usize)>,
}
impl Remote {
    pub(crate) fn new(
        requests: std::sync::mpsc::Sender<Request>,
        start: usize,
        len: usize,
    ) -> Self {
        let (reply, replies) = std::sync::mpsc::channel();
        Self {
            start,
            len,
            requests,
            reply,
            replies,
            next: None,
        }
    }
    fn ask(&mut self, start: usize, count: usize, buffer: Vec<Stereo>) -> Result<()> {
        let request = Request {
            start: self.start + start,
            count,
            buffer,
            reply: self.reply.clone(),
        };
        self.requests
            .send(request)
            .map_err(|_| "The frames of a remote read are closed")?;
        self.next = Some((start, count));
        Ok(())
    }
}
impl Frames for Remote {
    fn len(&self) -> usize {
        self.len
    }
    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        if start > self.len || count > self.len - start {
            return Err("Remote read exceeds its frame count".into());
        }
        let closed = "The frames of a remote read are closed";
        if self.next != Some((start, count)) {
            if self.next.is_some() {
                // The frames of another read: the reader left its order.
                let _ = self.replies.recv().map_err(|_| closed)?;
            }
            self.ask(start, count, Vec::new())?;
        }
        let read = self.replies.recv().map_err(|_| closed)??;
        let previous = std::mem::replace(output, read);
        self.next = None;
        let after = start + count;
        if after < self.len {
            self.ask(after, count.min(self.len - after), previous)?;
        }
        Ok(())
    }
}

/// Read the frames of each request of `requests` from `frames`, until each
/// sender of requests is dropped.
pub(crate) fn serve(frames: &mut impl Frames, requests: std::sync::mpsc::Receiver<Request>) {
    for Request {
        start,
        count,
        mut buffer,
        reply,
    } in requests
    {
        let read = frames
            .read(start, count, &mut buffer)
            .map(|()| buffer)
            .map_err(|e| e.to_string());
        // A reader that stopped waits for no reply.
        let _ = reply.send(read);
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

    /// Each stem that adds to a stored sum in blocks as it decodes gives
    /// the sum of the stems in memory (D234, D245).
    #[test]
    fn an_added_stem_gives_the_sum_in_memory() {
        let frames = 150_001;
        let stems: Vec<Vec<Stereo>> = (0..3)
            .map(|role| {
                (0..frames)
                    .map(|i| {
                        let x = (i as f32 * (0.011 + role as f32 * 0.003)).sin();
                        [x, -0.6 * x]
                    })
                    .collect()
            })
            .collect();
        let mut sum = Pcm::temporary_of(&stems[0]).unwrap();
        let mut expected = vec![[-0.0f32; 2]; frames];
        for (role, stem) in stems.iter().enumerate() {
            if role > 0 {
                // Blocks of a decoder, across the blocks of the store.
                for (index, block) in stem.chunks(40_009 + role).enumerate() {
                    sum.add_at(index * (40_009 + role), block).unwrap();
                }
            }
            for (total, frame) in expected.iter_mut().zip(stem) {
                for channel in 0..2 {
                    total[channel] += frame[channel];
                }
            }
        }
        let mut got = Vec::new();
        sum.read(0, frames, &mut got).unwrap();
        let bits = |audio: &[Stereo]| -> Vec<u32> {
            audio.iter().flatten().map(|x| x.to_bits()).collect()
        };
        assert_eq!(bits(&got), bits(&expected));
        assert!(sum.add_at(frames + 1, &stems[0][..1]).is_err());
        // An addition past the stored frames stores the frames after them.
        sum.add_at(frames - 1, &stems[0][..2]).unwrap();
        sum.read(frames - 1, 2, &mut got).unwrap();
        let last = [0, 1].map(|c| expected[frames - 1][c] + stems[0][0][c]);
        assert_eq!(bits(&got), bits(&[last, stems[0][1]]));
        assert_eq!(sum.len(), frames + 1);
    }

    /// Readers on other threads read their frames of a stored file through
    /// the thread that holds it, in order and out of order (D237).
    #[test]
    fn remote_reads_equal_the_stored_frames() {
        let audio: Vec<Stereo> = (0..100_003)
            .map(|i| [(i as f32 * 0.011).sin(), (i as f32 * 0.019).cos()])
            .collect();
        let mut pcm = Pcm::temporary_of(&audio).unwrap();
        let bits = |frames: &[Stereo]| -> Vec<u32> {
            frames.iter().flatten().map(|x| x.to_bits()).collect()
        };
        let spans = [(0, 40_000), (35_000, 40_000), (70_000, 30_003)];
        let source = &audio;
        let (requests, served) = std::sync::mpsc::channel();
        let read: Vec<(Vec<Stereo>, Vec<Stereo>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = spans
                .iter()
                .map(|&(start, len)| {
                    let mut frames = Remote::new(requests.clone(), start, len);
                    scope.spawn(move || {
                        let (mut all, mut block) = (Vec::new(), Vec::new());
                        for at in (0..len).step_by(4_096) {
                            frames.read(at, 4_096.min(len - at), &mut block).unwrap();
                            all.extend_from_slice(&block);
                        }
                        assert!(frames.read(len - 1, 2, &mut block).is_err());
                        // A read out of order, while a read in order waits.
                        frames.read(3, 5, &mut block).unwrap();
                        let early = block.clone();
                        frames.read(5_000, 7, &mut block).unwrap();
                        assert_eq!(bits(&block), bits(&source[start + 5_000..start + 5_007]));
                        (all, early)
                    })
                })
                .collect();
            drop(requests);
            serve(&mut pcm, served);
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (&(start, len), (all, early)) in spans.iter().zip(&read) {
            assert_eq!(bits(all), bits(&audio[start..start + len]));
            assert_eq!(bits(early), bits(&audio[start + 3..start + 8]));
        }
    }
}
