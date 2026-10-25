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

/// Read bounded byte chunks; retain only the PCM required by the current renderer.
pub fn read_source(path: &Path) -> Result<Source> {
    let file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    read_stream(file, bytes)
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

/// Decode a WAV stream block by block. `each` receives the advertised frame
/// count and the next decoded frames. Returns the sample rate.
pub(crate) fn read_blocks(
    mut reader: impl Read,
    byte_length: u64,
    mut each: impl FnMut(usize, &[Stereo]) -> Result<()>,
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
            each(total, &block_frames)?;
        }
    }
    decoder.finish()?;
    Ok(decoder.sampling_rate() as u32)
}

fn write_blocks(
    file: &mut impl Write,
    audio: &[Stereo],
    rate: u32,
    format: WavSampleFormat,
    mut encode: impl FnMut(&mut WavStreamEncoder, &[Stereo]) -> Result<Vec<u8>>,
) -> Result<()> {
    let mut encoder = WavStreamEncoder::new(format, rate, 2, audio.len() as u64)?;
    file.write_all(encoder.header())?;
    for block in audio.chunks(WRITE_FRAMES) {
        file.write_all(&encode(&mut encoder, block)?)?;
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
    audio: &[Stereo],
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
