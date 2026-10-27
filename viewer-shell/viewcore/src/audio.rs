//! Sounds, decoded for the page to play (SOUND.md, step 1: hear a file).
//!
//! The page plays with Web Audio but does not decode: what WebKitGTK can decode depends on the
//! machine's GStreamer plugins, so a sound is decoded here with Symphonia (WAV in PCM or
//! ADPCM, MP3, Ogg Vorbis, FLAC) and handed over as 16-bit PCM, which the page puts into an
//! `AudioBuffer` itself. Alongside: what the file is ([`Info`]) and a waveform ([`peaks`]).
//!
//! Where a sound comes from is the game's rule: a path through the VFS, and, as OpenMW does,
//! a `.wav` no folder or archive has is looked for as `.mp3` ([`candidates`]).

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// A decoded sound: interleaved 16-bit samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub rate: u32,
    pub channels: u16,
    /// Samples, `channels` to a frame.
    pub samples: Vec<i16>,
    /// The container (`wav`, `mp3`, `ogg`, `flac`), from the file's own magic.
    pub container: &'static str,
    /// The codec, as Symphonia names it (`pcm_s16le`, `adpcm_ms`, `mp3`, `vorbis` ...).
    pub codec: String,
    /// Bits a sample in the file, where the format says.
    pub bits: Option<u32>,
}

impl Decoded {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    pub fn seconds(&self) -> f64 {
        if self.rate == 0 { 0.0 } else { self.frames() as f64 / f64::from(self.rate) }
    }

    /// The loudest sample, 0-1.
    pub fn peak(&self) -> f64 {
        self.samples.iter().map(|s| f64::from(*s).abs()).fold(0.0, f64::max) / 32768.0
    }

    /// The root mean square level, 0-1.
    pub fn rms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|s| (f64::from(*s) / 32768.0).powi(2)).sum();
        (sum / self.samples.len() as f64).sqrt()
    }
}

/// The container a file's first bytes say it is.
pub fn container(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "wav"
    } else if bytes.starts_with(b"OggS") {
        "ogg"
    } else if bytes.starts_with(b"fLaC") {
        "flac"
    } else if bytes.starts_with(b"ID3") || (bytes.len() > 1 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
        "mp3"
    } else {
        ""
    }
}

/// Decode a whole sound. `Err` says why not: a format Symphonia does not read, or a file
/// that is broken before any audio comes out of it.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let kind = container(bytes);
    let mut hint = Hint::new();
    if !kind.is_empty() {
        hint.with_extension(kind);
    }
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("not a sound this reads ({e})"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("no audio in the file")?;
    let (id, params) = (track.id, track.codec_params.clone());
    let codec = symphonia::default::get_codecs().get_codec(params.codec).map(|d| d.short_name.to_string()).unwrap_or_default();
    let mut decoder =
        symphonia::default::get_codecs().make(&params, &DecoderOptions::default()).map_err(|e| format!("{codec}: cannot be decoded ({e})"))?;
    let mut rate = params.sample_rate.unwrap_or(0);
    let mut channels = params.channels.map(|c| c.count() as u16).unwrap_or(0);
    let mut samples: Vec<i16> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) if samples.is_empty() => return Err(format!("the file is broken ({e})")),
            Err(_) => break, // keep what played before the damage, as a player would
        };
        if packet.track_id() != id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                rate = spec.rate;
                channels = spec.channels.count() as u16;
                let mut sb = SampleBuffer::<i16>::new(buf.capacity() as u64, spec);
                sb.copy_interleaved_ref(buf);
                samples.extend_from_slice(sb.samples());
            }
            Err(Error::DecodeError(_)) => continue, // a bad frame is skipped
            Err(e) => return Err(format!("{codec}: {e}")),
        }
    }
    if channels == 0 || rate == 0 {
        return Err("no audio in the file".into());
    }
    Ok(Decoded { rate, channels, samples, container: kind, codec, bits: params.bits_per_sample })
}

/// The paths a sound is looked for under, in order: as given, then (as OpenMW does) a `.wav`
/// as `.mp3`.
pub fn candidates(path: &str) -> Vec<String> {
    let mut out = vec![path.to_string()];
    let low = path.to_ascii_lowercase();
    if let Some(stem) = low.strip_suffix(".wav") {
        out.push(format!("{}{}", &path[..stem.len()], ".mp3"));
    }
    out
}

/// What the page is handed to play: `WGS1`, a little-endian `u32` header length, the JSON
/// header `{rate, channels, frames}`, then the samples as little-endian `i16`, interleaved.
pub fn pcm_bytes(d: &Decoded) -> Vec<u8> {
    let head = format!("{{\"rate\":{},\"channels\":{},\"frames\":{}}}", d.rate, d.channels, d.frames());
    let mut out = Vec::with_capacity(8 + head.len() + d.samples.len() * 2);
    out.extend_from_slice(b"WGS1");
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(head.as_bytes());
    for s in &d.samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A waveform: `n` buckets of the channels mixed, each `(min, max)` in -1..1.
pub fn peaks(d: &Decoded, n: usize) -> Vec<(f32, f32)> {
    let frames = d.frames();
    let ch = usize::from(d.channels.max(1));
    if n == 0 || frames == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|b| {
            let (from, to) = (b * frames / n, ((b + 1) * frames / n).max(b * frames / n + 1).min(frames));
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for f in from..to {
                let v: f32 = d.samples[f * ch..f * ch + ch].iter().map(|s| f32::from(*s)).sum::<f32>() / (ch as f32 * 32768.0);
                lo = lo.min(v);
                hi = hi.max(v);
            }
            if from >= to { (0.0, 0.0) } else { (lo, hi) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A RIFF WAVE file of 16-bit PCM.
    fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = b"RIFF".to_vec();
        out.extend((36 + data.len() as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn a_wav_decodes_to_its_samples() {
        let s: Vec<i16> = (0..2000).map(|i| ((i % 100) as i16 - 50) * 300).collect();
        let d = decode(&wav(22050, 2, &s)).unwrap();
        assert_eq!((d.rate, d.channels, d.container, d.bits), (22050, 2, "wav", Some(16)));
        assert_eq!(d.samples, s);
        assert_eq!(d.frames(), 1000);
        assert!((d.peak() - 15000.0 / 32768.0).abs() < 1e-9);
        let p = pcm_bytes(&d);
        assert!(p.starts_with(b"WGS1"));
        let head = u32::from_le_bytes(p[4..8].try_into().unwrap()) as usize;
        assert_eq!(&p[8..8 + head], br#"{"rate":22050,"channels":2,"frames":1000}"#);
        assert_eq!(p.len(), 8 + head + 4000);
    }

    #[test]
    fn the_waveform_mixes_the_channels() {
        let d = Decoded { rate: 8, channels: 2, samples: vec![16384, 16384, -32768, 0, 0, 0, 32767, 32767], container: "wav", codec: String::new(), bits: None };
        let p = peaks(&d, 2);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], (-0.5, 0.5));
        assert!((p[1].1 - 32767.0 / 32768.0).abs() < 1e-6 && p[1].0 == 0.0);
    }

    #[test]
    fn what_is_not_a_sound_says_so() {
        assert!(decode(b"not audio at all").is_err());
        assert_eq!(container(b"OggS...."), "ogg");
        assert_eq!(candidates("Sound/Fx/Door.WAV"), vec!["Sound/Fx/Door.WAV".to_string(), "Sound/Fx/Door.mp3".to_string()]);
        assert_eq!(candidates("Music/a.mp3").len(), 1);
    }
}
