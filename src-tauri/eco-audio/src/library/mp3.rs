//! Pure-Rust MP3 encoder. VBR avoids the upstream whole-stream CBR reservoir path.
use std::fs::OpenOptions;
use std::io::{Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use rusty_mp3::{Mp3Encoder, Mp3EncoderConfig};
use crate::audio::resample::{Resampler, to_stereo};
const SUPPORTED_SAMPLE_RATES: &[u32] = &[8000,11025,12000,16000,22050,24000,32000,44100,48000];

/// A single-source recording goes straight from the capture ring to this file.
/// No WAV, temporary audio or second output copy is created.
pub(crate) struct StreamingMp3 {
    encoder: Mp3Encoder,
    output: std::fs::File,
    native: usize,
    rate: u32,
    resampler: Option<Resampler>,
}

impl StreamingMp3 {
    pub fn new(output: std::fs::File, sample_rate: u32, channels: u16) -> Result<Self, String> {
        if channels == 0 || channels > 64 || sample_rate == 0 {
            return Err("Недопустимый формат захвата MP3".into());
        }
        let rate = if SUPPORTED_SAMPLE_RATES.contains(&sample_rate) { sample_rate } else { 48000 };
        Ok(Self {
            encoder: Mp3Encoder::new(Mp3EncoderConfig {
                bitrate_kbps: 192, vbr_quality: Some(rusty_mp3::vbr_quality_index(2.0)),
            }),
            output, native: channels as usize, rate,
            resampler: (rate != sample_rate).then(|| Resampler::new(sample_rate, rate)),
        })
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        if samples.len() % self.native != 0 || samples.iter().any(|s| !s.is_finite()) {
            return Err("Неполные или нечисловые PCM-кадры".into());
        }
        for chunk in samples.chunks(2048 * self.native) {
            let mut downmix = Vec::new();
            let pcm = if self.native > 2 {
                // Same normalized channel policy as the post-recording WAV adapter.
                for frame in chunk.chunks_exact(self.native) {
                    let (mut l, mut r, mut weight) = (frame[0], frame[1], 1.0f32);
                    for (c, &value) in frame.iter().enumerate().skip(2) {
                        let gain = if c == 3 { 0.5 } else { std::f32::consts::FRAC_1_SQRT_2 };
                        if c < 4 { l += value * gain; r += value * gain; weight += gain; }
                        else if c % 2 == 0 { l += value * gain; weight += gain; }
                        else { r += value * gain; }
                    }
                    downmix.extend_from_slice(&[l / weight, r / weight]);
                }
                downmix.as_slice()
            } else { chunk };
            write_chunk(&mut self.encoder, pcm, self.native.min(2), self.rate, &mut self.resampler, &mut self.output)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), String> {
        self.output.flush().and_then(|_| self.output.sync_data()).map_err(|e| e.to_string())
    }

    pub fn finish(mut self) -> Result<(), String> {
        if let Some(resampler) = &mut self.resampler {
            let mut tail = Vec::new();
            resampler.finish(&mut tail);
            if self.native == 1 { tail = tail.chunks_exact(2).map(|f| f[0]).collect(); }
            write_pcm(&mut self.encoder, &tail, self.native.min(2), self.rate, &mut self.output)?;
        }
        self.encoder.finish();
        drain(&mut self.encoder, &mut self.output)?;
        // Persist the completed packets before updating the optional seek header.
        self.flush()?;
        if let Some(tag) = self.encoder.lame_tag_frame() {
            self.output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            self.output.write_all(tag).map_err(|e| e.to_string())?;
        }
        self.output.sync_all().map_err(|e| e.to_string())
    }
}

/// Never removes the WAV, on either success or failure. The finalizer owns cleanup.
pub fn encode_mp3(wav: &Path) -> Result<PathBuf, String> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let reader = hound::WavReader::open(wav).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        if reader.duration() == 0 { return Err("Пустой WAV".into()); }
        let out = wav.with_extension("mp3");
        let mut output = OpenOptions::new().write(true).create_new(true).open(&out)
            .map_err(|e| format!("MP3 не перезаписан: {e}"))?;
        let tag = match spec.sample_format {
            hound::SampleFormat::Float => encode_samples(reader.into_samples::<f32>(), spec, &mut output)?,
            hound::SampleFormat::Int => {
                let scale = 2.0f32.powi(spec.bits_per_sample as i32 - 1);
                encode_samples(reader.into_samples::<i32>().map(|s| s.map(|v| v as f32 / scale)), spec, &mut output)?
            }
        };
        // Complete the first Info/Xing frame after streaming the audio packets.
        if let Some(tag) = tag {
            output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            output.write_all(&tag).map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        if super::storage::mp3_track(crate::types::TrackKind::Input, &out).is_none() {
            return Err("Rust-кодировщик не создал корректный заголовок MP3".into());
        }
        Ok(out)
    })).unwrap_or_else(|_| Err("Внутренняя ошибка Rust-кодировщика".into()));
    result.map_err(|e: String| format!("Ошибка MP3: {e}. WAV сохранён: {}", wav.display()))
}

fn write_chunk(encoder: &mut Mp3Encoder, pcm: &[f32], channels: usize, rate: u32, resampler: &mut Option<Resampler>, output: &mut impl Write) -> Result<(), String> {
    let mut converted = Vec::new();
    let data = if let Some(resampler) = resampler {
        let mut stereo = Vec::new();
        to_stereo(pcm, channels, &mut stereo);
        resampler.process(&stereo, &mut converted);
        if channels == 1 { converted = converted.chunks_exact(2).map(|f| f[0]).collect(); }
        converted.as_slice()
    } else { pcm };
    write_pcm(encoder, data, channels, rate, output)
}

fn write_pcm(encoder: &mut Mp3Encoder, pcm: &[f32], channels: usize, rate: u32, output: &mut impl Write) -> Result<(), String> {
    if pcm.is_empty() { return Ok(()); }
    encoder.push_pcm_f32(pcm, channels as u16, rate).map_err(|e| e.to_string())?;
    drain(encoder, output)
}

fn drain(encoder: &mut Mp3Encoder, output: &mut impl Write) -> Result<(), String> {
    loop {
        match encoder.next_packet() {
            Ok(packet) => output.write_all(&packet).map_err(|e| e.to_string())?,
            Err(rusty_mp3::Error::Again | rusty_mp3::Error::Eof) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn encode_samples(samples: impl Iterator<Item = Result<f32, hound::Error>>, spec: hound::WavSpec, output: &mut impl Write) -> Result<Option<Vec<u8>>, String> {
    if spec.channels == 0 || spec.channels > 64 || spec.sample_rate == 0 { return Err("Недопустимый формат WAV".into()); }
    let native = spec.channels as usize;
    let channels = native.min(2);
    let rate = if SUPPORTED_SAMPLE_RATES.contains(&spec.sample_rate) { spec.sample_rate } else { 48000 };
    // VBR is a public API configuration and unconditionally skips the upstream
    // CBR reservoir's full-file buffering, even if MP3_RESERVOIR is set by the host.
    let mut encoder = Mp3Encoder::new(Mp3EncoderConfig {
        bitrate_kbps: 192, vbr_quality: Some(rusty_mp3::vbr_quality_index(2.0)),
    });
    let mut resampler = if rate != spec.sample_rate { Some(Resampler::new(spec.sample_rate, rate)) } else { None };
    let mut pcm = Vec::with_capacity(2048 * channels);
    let mut frame = Vec::with_capacity(native);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let mut frames = 0usize;
    for sample in samples {
        let sample = sample.map_err(|e| format!("Ошибка чтения WAV: {e}"))?;
        if !sample.is_finite() { return Err("WAV содержит нечисловые отсчёты".into()); }
        frame.push(sample);
        if frame.len() != native { continue; }
        if native <= 2 { pcm.extend_from_slice(&frame); }
        else {
            // Standard WAV order: front L/R, center, LFE, surround pairs.
            // Keep every channel and normalize the sum rather than clip or drop surrounds.
            let (mut l, mut r, mut weight) = (frame[0], frame[1], 1.0f32);
            for (c, &value) in frame.iter().enumerate().skip(2) {
                let gain = if c == 3 { 0.5 } else { std::f32::consts::FRAC_1_SQRT_2 };
                if c < 4 { l += value * gain; r += value * gain; weight += gain; }
                else if c % 2 == 0 { l += value * gain; weight += gain; }
                else { r += value * gain; }
            }
            pcm.extend_from_slice(&[l / weight, r / weight]);
        }
        frame.clear();
        frames += 1;
        if pcm.len() == 2048 * channels {
            if std::time::Instant::now() >= deadline { return Err("Кодирование превысило 5 минут".into()); }
            write_chunk(&mut encoder, &pcm, channels, rate, &mut resampler, output)?;
            pcm.clear();
        }
    }
    if !frame.is_empty() || frames == 0 { return Err("Неполный или пустой WAV".into()); }
    if !pcm.is_empty() { write_chunk(&mut encoder, &pcm, channels, rate, &mut resampler, output)?; }
    if let Some(resampler) = &mut resampler {
        let mut tail = Vec::new();
        resampler.finish(&mut tail);
        if channels == 1 { tail = tail.chunks_exact(2).map(|f| f[0]).collect(); }
        write_pcm(&mut encoder, &tail, channels, rate, output)?;
    }
    encoder.finish();
    drain(&mut encoder, output)?;
    output.flush().map_err(|e| e.to_string())?;
    Ok(encoder.lame_tag_frame().map(<[u8]>::to_vec))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_mp3_matrix_independently_decodes() {
        struct Temp(PathBuf);
        impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let root = Temp(std::env::temp_dir().join(format!("eco5-codec-{}-{}", std::process::id(), chrono::Local::now().timestamp_nanos_opt().unwrap())));
        std::fs::create_dir(&root.0).unwrap();
        // Independent decoder is test-only. Eco's production encoder never searches for FFmpeg.
        let decoder = PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("ffmpeg.exe");
        let cases = [(8000,1),(22050,2),(32000,2),(44100,1),(44100,2),(48000,1),(48000,2),(96000,1),(96000,2),(192000,2),(48000,6)];
        let mut frequency_failures = Vec::new();
        for (case_index, (rate, channels)) in cases.into_iter().enumerate() {
            let wav = root.0.join(format!("{rate}-{channels}.wav"));
            let format = match case_index % 3 { 0 => crate::types::Format::Wav32f, 1 => crate::types::Format::Wav16, _ => crate::types::Format::Wav24 };
            let mut writer = crate::audio::wav::TrackWriter::create(&wav, rate, channels, format).unwrap();
            let frames = rate as usize * 2 / 5;
            let mut pcm = Vec::with_capacity(frames * channels as usize);
            for i in 0..frames { for c in 0..channels {
                let hz = if c % 2 == 0 { 440.0 } else { 660.0 };
                pcm.push((i as f32 * hz * std::f32::consts::TAU / rate as f32).sin() * 0.1);
            } }
            writer.write(&pcm).unwrap(); writer.finalize().unwrap();
            let original = std::fs::read(&wav).unwrap();
            let mp3 = encode_mp3(&wav).unwrap();
            assert_eq!(std::fs::read(&wav).unwrap(), original);
            let expected_rate = if SUPPORTED_SAMPLE_RATES.contains(&rate) { rate } else { 48000 };
            let track = super::super::storage::mp3_track(crate::types::TrackKind::Input, &mp3).unwrap();
            assert_eq!((track.sample_rate,track.channels),(expected_rate,channels.min(2)));
            let decoded = root.0.join(format!("decoded-{rate}-{channels}.wav"));
            let mut cmd = std::process::Command::new(&decoder);
            #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
            let mut child = cmd.args(["-nostdin","-v","error","-xerror","-err_detect","explode","-i"]).arg(&mp3)
                .args(["-c:a","pcm_f32le"]).arg(&decoded).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
            let deadline = std::time::Instant::now()+std::time::Duration::from_secs(15);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() { break status; }
                if std::time::Instant::now() >= deadline { let _=child.kill(); let _=child.wait(); panic!("decoder timeout"); }
                std::thread::sleep(std::time::Duration::from_millis(20));
            };
            assert!(status.success(), "independent decode failed: {rate} Hz {channels} channels");
            let decoded = hound::WavReader::open(decoded).unwrap();
            let spec = decoded.spec();
            assert_eq!((spec.sample_rate,spec.channels),(expected_rate,channels.min(2)));
            let data: Vec<f32> = decoded.into_samples::<f32>().map(Result::unwrap).collect();
            let duration = data.len() as f64 / spec.channels as f64 / expected_rate as f64;
            assert!((duration-0.4).abs()<0.15, "duration {duration}, {rate}");
            for c in 0..spec.channels as usize {
                let samples: Vec<f32> = data.chunks_exact(spec.channels as usize).map(|f|f[c]).collect();
                let middle = &samples[samples.len()/4..samples.len()*3/4];
                assert!(middle.iter().all(|s|s.is_finite()));
                let rms = (middle.iter().map(|s|s*s).sum::<f32>()/middle.len() as f32).sqrt();
                assert!(rms>0.02 && rms<0.3, "RMS {rms}: {rate}/{channels}");
                if channels<=2 {
                    let expected = if c==0 {440.0} else {660.0};
                    // Zero-crossing counts mistake codec noise near zero for pitch.
                    // Project onto the known sine/cosine pair instead, phase independently.
                    let (mut re, mut im) = (0.0f64, 0.0f64);
                    for (i, &sample) in middle.iter().enumerate() {
                        let phase = i as f64 * expected * std::f64::consts::TAU / expected_rate as f64;
                        re += sample as f64 * phase.cos(); im += sample as f64 * phase.sin();
                    }
                    let amplitude = 2.0 * (re*re+im*im).sqrt() / middle.len() as f64;
                    let fraction = amplitude*amplitude / (2.0*rms as f64*rms as f64);
                    if fraction<0.7 { frequency_failures.push(format!("tone power fraction {fraction:.4} at {expected} Hz: {rate}/{channels}, channel {c}")); }
                }
            }
        }
        assert!(frequency_failures.is_empty(), "{}", frequency_failures.join("; "));
    }
    #[test]
    fn output_failure_is_reported() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> { Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated full disk")) }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let spec = hound::WavSpec { channels: 1, sample_rate: 48000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        assert!(encode_samples((0..48000).map(|_| Ok(0.1)), spec, &mut Broken).is_err());
    }
}
