//! Engine input policy and bounded SoundKit I/O.
use soundkit::{
    audio_pipeline::audio_to_f32_channels,
    wav::{WavSampleFormat, WavStreamEncoder, WavStreamProcessor},
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub type Stereo = [f32; 2];
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const IO_BYTES: usize = 65_536;
const WRITE_FRAMES: usize = 8192;

pub struct Source {
    pub audio: Vec<Stereo>,
    pub sample_rate: u32,
}

/// The rates at which the engine renders, in Hz.
pub const RENDER_RATES: [u32; 2] = [44_100, 48_000];

/// The render rate of a source at `source_rate`: 44.1 kHz for 44.1, 88.2
/// and 176.4 kHz, and 48 kHz for 48, 96 and 192 kHz. A source at a render
/// rate has no conversion; a higher rate converts by a factor of 2 or 4.
pub fn render_rate(source_rate: u32) -> Result<u32> {
    match source_rate {
        44_100 | 88_200 | 176_400 => Ok(44_100),
        48_000 | 96_000 | 192_000 => Ok(48_000),
        _ => Err("Stereo at 44.1, 48, 88.2, 96, 176.4, or 192 kHz is required".into()),
    }
}

/// An error unless `rate` is a render rate.
pub fn check_render_rate(rate: u32) -> Result<()> {
    if RENDER_RATES.contains(&rate) {
        Ok(())
    } else {
        Err("The engine renders at 44.1 or 48 kHz".into())
    }
}

impl Source {
    /// The source at its render rate (`render_rate`).
    pub fn at_render_rate(self) -> Result<Source> {
        let rate = render_rate(self.sample_rate)?;
        Ok(Source {
            audio: crate::resample::convert_owned(self.audio, self.sample_rate, rate)?,
            sample_rate: rate,
        })
    }
}

/// The WAV at `path` at its render rate.
pub fn read(path: &Path) -> Result<Source> {
    read_source(path)?.at_render_rate()
}

/// A source in scratch storage at its render rate: the frames of
/// `Source::at_render_rate` (D250).
pub struct StoredSource {
    pub(crate) audio: crate::scratch::Pcm,
    /// The render rate, in Hz.
    pub sample_rate: u32,
    /// The rate of the decoded source, in Hz.
    pub source_rate: u32,
    /// The frames of the decoded source.
    pub source_frames: usize,
}

/// Frames of each piece that `StoredSource` reads.
const STORED_PIECE: usize = 65_536;

impl StoredSource {
    /// The source whose blocks `decode` gives in order to its argument.
    /// `decode` returns the sample rate of the source. The frames go to
    /// scratch storage, and a source above a render rate converts in spans.
    pub fn from_blocks(
        decode: impl FnOnce(&mut dyn FnMut(&[Stereo]) -> Result<()>) -> Result<u32>,
    ) -> Result<Self> {
        Self::stored(decode, false)
    }

    /// `from_blocks`, with the check of `read_stream` for at least three
    /// seconds of audio when `minimum` is set.
    fn stored(
        decode: impl FnOnce(&mut dyn FnMut(&[Stereo]) -> Result<()>) -> Result<u32>,
        minimum: bool,
    ) -> Result<Self> {
        let mut stored = crate::scratch::Pcm::new(crate::scratch::temporary()?);
        let mut finite = true;
        let source_rate = decode(&mut |block| {
            finite &= block.iter().flatten().all(|x| x.is_finite());
            stored.push(block)
        })?;
        let source_frames = crate::scratch::Frames::len(&stored);
        if minimum && source_frames < source_rate as usize * 3 {
            return Err("At least three seconds of audio are required".into());
        }
        let rate = render_rate(source_rate)?;
        // As in `resample::convert_owned`.
        if !finite {
            return Err("Resampling requires finite stereo and a supported sample rate".into());
        }
        let audio = if source_rate == rate {
            stored
        } else {
            crate::resample::convert_stored(&mut stored, source_rate, rate)?
        };
        Ok(Self {
            audio,
            sample_rate: rate,
            source_rate,
            source_frames,
        })
    }

    /// The frames at the render rate.
    pub fn frames(&self) -> usize {
        crate::scratch::Frames::len(&self.audio)
    }

    /// `stem_evidence::fingerprint` of the frames at the render rate.
    pub fn fingerprint(&mut self) -> Result<String> {
        crate::stem_evidence::fingerprint_read(&mut self.audio)
    }

    /// `dsp::loudness` of the frames at the render rate.
    pub fn loudness(&mut self) -> Result<Option<f64>> {
        crate::dsp::loudness_read(&mut self.audio, self.sample_rate)
    }

    /// `waveform::analyze` of the frames at the render rate.
    pub fn waveform(&mut self) -> Result<crate::waveform::Waveform> {
        crate::waveform::analyze_read(&mut self.audio, self.sample_rate)
    }

    /// `write` of the frames at the render rate: a new float32 WAV file at
    /// `path`, read one piece at a time.
    pub fn write_wav(&mut self, path: &Path) -> Result<()> {
        let mut file = std::io::BufWriter::new(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?,
        );
        write_f32_to(&mut file, &mut self.audio, self.sample_rate)
    }
}

/// `read` of the WAV at `path` into scratch storage: the decoded blocks go
/// to a scratch file, and a source above a render rate converts in spans
/// (D250).
pub fn read_stored(path: &Path) -> Result<StoredSource> {
    let (file, bytes) = open(path)?;
    StoredSource::stored(
        |each| read_blocks(file, bytes, |_, block| each(block)),
        true,
    )
}

/// The float32 render at `path` at render rate `rate`, in the form of
/// `write`: its frames stay in the file, and the measures read them there
/// (D259). A file in another form, or at another rate, goes to scratch
/// storage (`read_stored`).
pub fn read_render(path: &Path, rate: u32) -> Result<StoredSource> {
    let (mut file, bytes) = open(path)?;
    let header_of = |frames| -> Result<Vec<u8>> {
        Ok(
            WavStreamEncoder::new(WavSampleFormat::F32, rate, 2, frames)?
                .header()
                .to_vec(),
        )
    };
    let empty = header_of(0)?.len() as u64;
    let mut frames = bytes.saturating_sub(empty) / 8;
    let mut header = header_of(frames)?;
    // The RF64 header of a large file is longer than the header of an
    // empty file.
    if header.len() as u64 != empty {
        frames = bytes.saturating_sub(header.len() as u64) / 8;
        header = header_of(frames)?;
    }
    let mut start = vec![0; header.len()];
    let in_place = render_rate(rate).is_ok_and(|r| r == rate)
        && header.len() as u64 + frames * 8 == bytes
        && file.read_exact(&mut start).is_ok()
        && start == header;
    if !in_place {
        return read_stored(path);
    }
    let frames = usize::try_from(frames)?;
    let mut audio = crate::scratch::Pcm::holding(
        Box::new(crate::scratch::Offset {
            inner: file,
            offset: header.len() as u64,
        }),
        frames,
    );
    // The checks of `read_stored`.
    if frames < rate as usize * 3 {
        return Err("At least three seconds of audio are required".into());
    }
    if !crate::scratch::all_finite(&mut audio)? {
        return Err("Non-finite input sample".into());
    }
    Ok(StoredSource {
        audio,
        sample_rate: rate,
        source_rate: rate,
        source_frames: frames,
    })
}

/// Read bounded byte chunks; retain only the PCM required by the current renderer.
pub fn read_source(path: &Path) -> Result<Source> {
    let (file, bytes) = open(path)?;
    read_stream(file, bytes)
}

/// The file at `path` and its length in bytes.
pub fn open(path: &Path) -> Result<(fs::File, u64)> {
    let file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    Ok((file, bytes))
}

pub fn read_stream(reader: impl Read, byte_length: u64) -> Result<Source> {
    let mut output = Vec::new();
    let sample_rate = read_blocks(reader, byte_length, |frames, block| {
        if output.is_empty() {
            output.try_reserve_exact(frames)?;
        }
        output.extend_from_slice(block);
        Ok(())
    })?;
    if output.len() < sample_rate as usize * 3 {
        return Err("At least three seconds of audio are required".into());
    }
    Ok(Source {
        audio: output,
        sample_rate,
    })
}

/// The frames of the WAV file at `path`, block by block, to `each`, with
/// the sample rate of the file.
pub fn read_source_blocks(
    path: &Path,
    each: &mut dyn FnMut(u32, &[Stereo]) -> Result<()>,
) -> Result<()> {
    let (file, bytes) = open(path)?;
    read_stream_blocks(file, bytes, each)
}

/// The frames of a WAV stream, block by block, to `each`, with the sample
/// rate of the stream.
pub fn read_stream_blocks(
    reader: impl Read,
    byte_length: u64,
    each: &mut dyn FnMut(u32, &[Stereo]) -> Result<()>,
) -> Result<()> {
    read_blocks_at_rate(reader, byte_length, |_, rate, block| each(rate, block))?;
    Ok(())
}

/// Decode a WAV stream block by block. `each` receives the advertised frame
/// count and the next decoded frames. Returns the sample rate.
pub(crate) fn read_blocks(
    reader: impl Read,
    byte_length: u64,
    mut each: impl FnMut(usize, &[Stereo]) -> Result<()>,
) -> Result<u32> {
    read_blocks_at_rate(reader, byte_length, |frames, _, block| each(frames, block))
}

/// `read_blocks`, with the sample rate of each block.
fn read_blocks_at_rate(
    mut reader: impl Read,
    byte_length: u64,
    mut each: impl FnMut(usize, u32, &[Stereo]) -> Result<()>,
) -> Result<u32> {
    let mut decoder = WavStreamProcessor::new();
    let mut bytes = [0u8; IO_BYTES];
    let mut frames = None;
    let mut block_frames = Vec::new();
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        if let Some(block) = decoder.add(&bytes[..count])? {
            let sample_rate = block.sampling_rate();
            if block.channel_count() != 2 || !crate::resample::RATES.contains(&sample_rate) {
                return Err("Stereo at 44.1, 48, 88.2, 96, 176.4, or 192 kHz is required".into());
            }
            if !matches!(block.bits_per_sample(), 16 | 24 | 32) {
                return Err("Supported samples: PCM16, PCM24, PCM32, float32".into());
            }
            let channels = audio_to_f32_channels(&block)?;
            let total = match frames {
                Some(total) => total,
                None => {
                    let frame_bytes = u64::from(block.bits_per_sample() / 8) * 2;
                    let total = match decoder.total_frames() {
                        Some(total) => {
                            // The physical file must contain the advertised samples before allocating PCM.
                            let data_bytes =
                                total.checked_mul(frame_bytes).ok_or("WAV size overflow")?;
                            if data_bytes > byte_length {
                                return Err("Truncated WAV samples".into());
                            }
                            total
                        }
                        // The data of a streamed WAV runs to the end of the
                        // file: the file length bounds its frames.
                        None => byte_length / frame_bytes,
                    };
                    let total = usize::try_from(total)?;
                    frames = Some(total);
                    total
                }
            };
            block_frames.clear();
            for (&left, &right) in channels[0].iter().zip(&channels[1]) {
                if !left.is_finite() || !right.is_finite() {
                    return Err("Non-finite input sample".into());
                }
                block_frames.push([left, right]);
            }
            each(total, sample_rate, &block_frames)?;
        }
    }
    decoder.finish()?;
    Ok(decoder.sampling_rate() as u32)
}

fn write_blocks(
    file: &mut impl Write,
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
    format: WavSampleFormat,
    mut encode: impl FnMut(&mut WavStreamEncoder, &[Stereo]) -> Result<Vec<u8>>,
) -> Result<()> {
    let frames = audio.len();
    let mut encoder = WavStreamEncoder::new(format, rate, 2, frames as u64)?;
    file.write_all(encoder.header())?;
    // Each piece holds whole blocks, so the blocks are those of the whole
    // frames (D253).
    let mut piece = Vec::new();
    for start in (0..frames).step_by(STORED_PIECE) {
        let piece = audio.window(start, STORED_PIECE.min(frames - start), &mut piece)?;
        for block in piece.chunks(WRITE_FRAMES) {
            file.write_all(&encode(&mut encoder, block)?)?;
        }
    }
    encoder.finish()?;
    file.flush()?;
    Ok(())
}

/// Write `audio`, at `rate`, as a new float32 WAV file at `path`.
pub fn write(path: &Path, audio: &[Stereo], rate: u32) -> Result<()> {
    if audio.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Refusing to write non-finite samples".into());
    }
    let mut file = std::io::BufWriter::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
    );
    write_to(&mut file, audio, rate)
}

/// Write `audio`, at `rate`, as a float32 WAV stream.
pub fn write_to(writer: &mut impl Write, audio: &[Stereo], rate: u32) -> Result<()> {
    if audio.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Refusing to write non-finite samples".into());
    }
    write_f32_to(writer, &mut crate::scratch::Slice(audio), rate)
}

/// `write_to` of the frames of `audio`, whose samples the caller checked.
pub(crate) fn write_f32_to(
    writer: &mut impl Write,
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<()> {
    let mut planar = Vec::with_capacity(WRITE_FRAMES * 2);
    write_blocks(
        writer,
        audio,
        rate,
        WavSampleFormat::F32,
        |encoder, block| {
            planar.clear();
            for channel in 0..2 {
                planar.extend(block.iter().map(|frame| frame[channel]));
            }
            Ok(encoder.push_planar_f32(&planar, block.len())?)
        },
    )
}

pub(crate) fn write_quantized_to(
    writer: &mut impl Write,
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
    bits: u16,
    mut quantize: impl FnMut(f32) -> i32,
) -> Result<()> {
    let format = if bits == 16 {
        WavSampleFormat::I16
    } else {
        WavSampleFormat::I24
    };
    let mut planar = vec![0; WRITE_FRAMES * 2];
    let mut pcm16 = Vec::with_capacity(WRITE_FRAMES * 2);
    write_blocks(writer, audio, rate, format, |encoder, block| {
        // Dither advances in frame/channel order, independent of chunk boundaries.
        for (i, frame) in block.iter().enumerate() {
            planar[i] = quantize(frame[0]);
            planar[block.len() + i] = quantize(frame[1]);
        }
        let data = &planar[..block.len() * 2];
        Ok(if bits == 16 {
            pcm16.clear();
            pcm16.extend(data.iter().map(|&x| x as i16));
            encoder.push_planar_i16(&pcm16, block.len())?
        } else {
            encoder.push_planar_i24(data, block.len())?
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored source has the frames, the fingerprint, the loudness, the
    /// waveform and the WAV file of the source at its render rate (D250).
    #[test]
    fn a_stored_source_is_the_source_at_its_render_rate() {
        let file = |name: &str| {
            std::env::temp_dir().join(format!(
                "mastering-stored-source-{}-{name}.wav",
                std::process::id()
            ))
        };
        // Six seconds at 96 kHz span two conversion spans.
        for (rate, frames) in [(48_000, 3 * 48_000 + 70_001), (96_000, 6 * 96_000 + 11)] {
            let audio: Vec<Stereo> = (0..frames)
                .map(|n| {
                    let phase = n as f32 * std::f32::consts::TAU / rate as f32;
                    let level = 0.1 + 0.3 * ((n / 40_000) % 4) as f32 / 3.0;
                    [
                        level * (phase * 220.0).sin(),
                        0.7 * level * (phase * 331.0).sin(),
                    ]
                })
                .collect();
            let input = file(&format!("{rate}-input"));
            write(&input, &audio, rate).unwrap();
            let whole = read_source(&input).unwrap().at_render_rate().unwrap();
            let mut stored = read_stored(&input).unwrap();
            std::fs::remove_file(&input).unwrap();
            assert_eq!(
                (stored.sample_rate, stored.source_rate, stored.source_frames),
                (whole.sample_rate, rate, frames)
            );
            assert_eq!(stored.frames(), whole.audio.len());
            let mut frames_of = Vec::new();
            let count = stored.frames();
            crate::scratch::Frames::read(&mut stored.audio, 0, count, &mut frames_of).unwrap();
            let bits = |audio: &[Stereo]| -> Vec<[u32; 2]> {
                audio.iter().map(|f| f.map(f32::to_bits)).collect()
            };
            assert_eq!(bits(&frames_of), bits(&whole.audio));
            assert_eq!(
                stored.fingerprint().unwrap(),
                crate::stem_evidence::fingerprint(&whole.audio)
            );
            assert_eq!(
                stored.loudness().unwrap().map(f64::to_bits),
                crate::dsp::loudness(&whole.audio, whole.sample_rate).map(f64::to_bits)
            );
            assert_eq!(
                serde_json::to_value(stored.waveform().unwrap()).unwrap(),
                serde_json::to_value(crate::waveform::analyze(&whole.audio, whole.sample_rate))
                    .unwrap()
            );
            let (from_storage, from_memory) = (
                file(&format!("{rate}-stored")),
                file(&format!("{rate}-whole")),
            );
            stored.write_wav(&from_storage).unwrap();
            write(&from_memory, &whole.audio, whole.sample_rate).unwrap();
            let bytes = [&from_storage, &from_memory].map(|f| std::fs::read(f).unwrap());
            for f in [from_storage, from_memory] {
                std::fs::remove_file(f).unwrap();
            }
            assert!(bytes[0] == bytes[1]);
        }
        let short = file("short");
        write(&short, &vec![[0.0; 2]; 3 * 48_000 - 1], 48_000).unwrap();
        let result = read_stored(&short);
        std::fs::remove_file(&short).unwrap();
        assert!(result.is_err());
    }

    /// A float32 render is read in its file, with the frames and the
    /// fingerprint of the copy in scratch storage. A file in another form
    /// goes to scratch storage (D259).
    #[test]
    fn a_float32_render_is_read_in_its_file() {
        use std::io::{Seek, SeekFrom};
        let file = |name: &str| {
            std::env::temp_dir().join(format!(
                "mastering-render-{}-{name}.wav",
                std::process::id()
            ))
        };
        let frames = 3 * 48_000 + 70_001;
        let audio: Vec<Stereo> = (0..frames)
            .map(|n| {
                let phase = n as f32 * std::f32::consts::TAU / 48_000.0;
                [0.3 * (phase * 220.0).sin(), 0.2 * (phase * 331.0).sin()]
            })
            .collect();
        let render = file("float32");
        write(&render, &audio, 48_000).unwrap();
        let all = |stored: &mut StoredSource| {
            let mut output = Vec::new();
            let count = stored.frames();
            crate::scratch::Frames::read(&mut stored.audio, 0, count, &mut output).unwrap();
            output
                .iter()
                .map(|f| f.map(f32::to_bits))
                .collect::<Vec<_>>()
        };
        let mut in_place = read_render(&render, 48_000).unwrap();
        let mut copied = read_stored(&render).unwrap();
        assert_eq!(
            (
                in_place.sample_rate,
                in_place.source_rate,
                in_place.source_frames
            ),
            (48_000, 48_000, frames)
        );
        assert_eq!(all(&mut in_place), all(&mut copied));
        assert_eq!(
            in_place.fingerprint().unwrap(),
            copied.fingerprint().unwrap()
        );
        // The frames stay in the file: a change of its last sample reads
        // back.
        let mut writer = fs::OpenOptions::new().write(true).open(&render).unwrap();
        writer.seek(SeekFrom::End(-4)).unwrap();
        writer.write_all(&0.5f32.to_le_bytes()).unwrap();
        drop(writer);
        let mut last = Vec::new();
        crate::scratch::Frames::read(&mut in_place.audio, frames - 1, 1, &mut last).unwrap();
        assert_eq!(last[0][1], 0.5);
        // A non-finite sample in the file is an error, as in `read_stored`.
        let mut writer = fs::OpenOptions::new().write(true).open(&render).unwrap();
        writer.seek(SeekFrom::End(-4)).unwrap();
        writer.write_all(&f32::NAN.to_le_bytes()).unwrap();
        drop(writer);
        assert!(read_render(&render, 48_000).is_err());
        assert!(read_stored(&render).is_err());
        // A file at another rate, or with bytes after its frames, goes to
        // scratch storage.
        fs::remove_file(&render).unwrap();
        write(&render, &audio, 48_000).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&render)
            .unwrap()
            .write_all(&[0; 3])
            .unwrap();
        let mut longer = read_render(&render, 48_000).unwrap();
        assert_eq!(all(&mut longer), all(&mut copied));
        fs::remove_file(&render).unwrap();
        let high = file("float32-96k");
        let audio_96k: Vec<Stereo> = (0..6 * 96_000 + 11)
            .map(|n| [0.1 * ((n % 97) as f32 / 97.0), -0.1])
            .collect();
        write(&high, &audio_96k, 96_000).unwrap();
        let mut converted = read_render(&high, 96_000).unwrap();
        let mut stored = read_stored(&high).unwrap();
        fs::remove_file(&high).unwrap();
        assert_eq!(converted.sample_rate, 48_000);
        assert_eq!(all(&mut converted), all(&mut stored));
    }

    #[test]
    fn streamed_wav_is_read_to_the_end() {
        // A writer that cannot seek leaves the RIFF length and the data
        // size at 0xFFFFFFFF.
        let frames = 3 * 48000 + 7;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&u32::MAX.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for field in [16u32, 1 | 2 << 16, 48000, 48000 * 4, 4 | 16 << 16] {
            wav.extend_from_slice(&field.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&u32::MAX.to_le_bytes());
        for i in 0..frames {
            let sample = (i % 1000) as i16 * 16;
            wav.extend_from_slice(&sample.to_le_bytes());
            wav.extend_from_slice(&(-sample).to_le_bytes());
        }
        let length = wav.len() as u64;
        let source = read_stream(std::io::Cursor::new(wav), length).unwrap();
        assert_eq!(source.sample_rate, 48000);
        assert_eq!(source.audio.len(), frames);
        assert_eq!(
            source.audio[999],
            [999.0 * 16.0 / 32768.0, -999.0 * 16.0 / 32768.0]
        );
    }
}
