//! RDP-Mikrofon (MS-RDPEAI, Kanal AUDIO_INPUT): Ton vom Standard-Eingang dieses Geräts (cpal) als PCM 16 Bit an den
//! Server. Nur wenn es für die Verbindung eingeschaltet ist; das System fragt beim ersten Mal nach dem Mikrofon.

use crate::rdp::Input;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ironrdp_rdpeai::client::{AudioPacketSink, RdpeaiCaptureHandler, RdpeaiClient};
use ironrdp_rdpsnd::pdu::{AudioFormat, WaveFormat};
use tokio::sync::mpsc::UnboundedSender;

const E_FAIL: i32 = 0x8000_4005_u32 as i32;

fn pcm(rate: u32, channels: u16) -> AudioFormat {
    AudioFormat {
        format: WaveFormat::PCM,
        n_channels: channels,
        n_samples_per_sec: rate,
        n_avg_bytes_per_sec: rate * u32::from(channels) * 2,
        n_block_align: channels * 2,
        bits_per_sample: 16,
        data: None,
    }
}

/// Wandelt den Ton des Geräts (f32, beliebige Rate und Kanäle) in PCM 16 Bit der verhandelten Art und schneidet
/// Pakete fester Grösse. Umrechnung der Rate linear; für Sprache genügt das.
pub struct Pack {
    from: (u32, u16),
    to: (u32, u16),
    /// Lesestelle in Eingangs-Frames (Bruchteil), relativ zu `last`.
    pos: f64,
    /// Letzter Frame des vorigen Blocks, für die Umrechnung über die Blockgrenze.
    last: Vec<f32>,
    buf: Vec<u8>,
    size: usize,
}

impl Pack {
    pub fn new(from: (u32, u16), to: (u32, u16), size: usize) -> Self {
        Self { from, to, pos: 0.0, last: vec![0.0; from.1.max(1).into()], buf: Vec::with_capacity(size * 2), size: size.max(2) }
    }

    pub fn push(&mut self, data: &[f32], out: &mut impl FnMut(Vec<u8>)) {
        let ch = usize::from(self.from.1.max(1));
        let frames = data.len() / ch;
        let step = f64::from(self.from.0) / f64::from(self.to.0);
        // Frame i: i = 0 ist `last`, danach die neuen Frames.
        let frame = |i: usize, c: usize| -> f32 { if i == 0 { self.last[c.min(ch - 1)] } else { data[(i - 1) * ch + c.min(ch - 1)] } };
        while self.pos + 1.0 <= frames as f64 {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            for c in 0..usize::from(self.to.1) {
                let v = frame(i, c) * (1.0 - t) + frame(i + 1, c) * t;
                self.buf.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
            }
            self.pos += step;
        }
        if frames > 0 {
            self.last = data[(frames - 1) * ch..frames * ch].to_vec();
            self.pos -= frames as f64;
        }
        while self.buf.len() >= self.size {
            let rest = self.buf.split_off(self.size);
            out(std::mem::replace(&mut self.buf, rest));
        }
    }
}

struct Mic {
    formats: Vec<AudioFormat>,
    stop: Option<std::sync::mpsc::Sender<()>>,
}

impl RdpeaiCaptureHandler for Mic {
    fn supported_formats(&self) -> &[AudioFormat] {
        &self.formats
    }

    fn open(&mut self, _capture: &AudioFormat, encode: &AudioFormat, packet_size: usize, mut sink: AudioPacketSink) -> i32 {
        self.close();
        if encode.format != WaveFormat::PCM || encode.bits_per_sample != 16 {
            return E_FAIL;
        }
        let Some(dev) = cpal::default_host().default_input_device() else {
            tracing::info!("RDP-Mikrofon: kein Eingang");
            return E_FAIL;
        };
        let Ok(cfg) = dev.default_input_config() else { return E_FAIL };
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let to = (encode.n_samples_per_sec, encode.n_channels);
        // cpal::Stream ist nicht überall Send: eigener Thread hält ihn, bis `close`.
        std::thread::spawn(move || {
            let from = (cfg.sample_rate(), cfg.channels());
            let mut pack = Pack::new(from, to, packet_size);
            let mut send = move |d: &[f32]| pack.push(d, &mut sink);
            let on_err = |e| tracing::warn!("RDP-Mikrofon: {e}");
            let conf = cfg.config();
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => dev.build_input_stream(&conf, move |d: &[f32], _: &_| send(d), on_err, None),
                cpal::SampleFormat::I16 => {
                    dev.build_input_stream(&conf, move |d: &[i16], _: &_| send(&d.iter().map(|&s| f32::from(s) / 32768.0).collect::<Vec<_>>()), on_err, None)
                }
                cpal::SampleFormat::I32 => {
                    dev.build_input_stream(&conf, move |d: &[i32], _: &_| send(&d.iter().map(|&s| s as f32 / 2_147_483_648.0).collect::<Vec<_>>()), on_err, None)
                }
                cpal::SampleFormat::U16 => {
                    dev.build_input_stream(&conf, move |d: &[u16], _: &_| send(&d.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0).collect::<Vec<_>>()), on_err, None)
                }
                f => {
                    tracing::warn!("RDP-Mikrofon: Format {f} nicht unterstützt");
                    return;
                }
            };
            match stream.map_err(|e| e.to_string()).and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => {
                    tracing::info!(rate = from.0, channels = from.1, "RDP-Mikrofon offen");
                    let _ = stopped.recv();
                    drop(s);
                }
                Err(e) => tracing::warn!("RDP-Mikrofon: {e}"),
            }
        });
        self.stop = Some(stop);
        0
    }

    fn set_format(&mut self, encode: &AudioFormat, _packet_size: usize) -> bool {
        // Nur PCM 16 Bit; ein Wechsel der Rate bräuchte einen Neustart der Aufnahme.
        encode.format == WaveFormat::PCM && encode.bits_per_sample == 16
    }

    fn close(&mut self) {
        // Fallenlassen des Senders beendet den Aufnahme-Thread.
        self.stop.take();
    }
}

/// Kanal fürs Mikrofon; die Nachrichten gehen über die Sitzungsschleife (`Input::Dvc`) an den Server.
pub fn client(tx: UnboundedSender<Input>) -> RdpeaiClient {
    let formats = [(44100, 2), (44100, 1), (48000, 2), (48000, 1), (22050, 2), (22050, 1), (16000, 1)].into_iter().map(|(r, c)| pcm(r, c)).collect();
    RdpeaiClient::new(
        Box::new(Mic { formats, stop: None }),
        Box::new(move |_, msgs| {
            let _ = tx.send(Input::Dvc(msgs));
            Ok(())
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn pakete_mit_fester_groesse_und_rate() {
        // 48 kHz Stereo → 16 kHz Mono, Pakete zu 64 Byte (32 Frames).
        let mut p = super::Pack::new((48000, 2), (16000, 1), 64);
        let mut got: Vec<Vec<u8>> = Vec::new();
        let block: Vec<f32> = (0..480).flat_map(|i| [i as f32 / 960.0, 0.0]).collect(); // 10 ms
        for _ in 0..10 {
            p.push(&block, &mut |b| got.push(b));
        }
        // 100 ms bei 16 kHz = 1600 Frames = 3200 Byte = 50 Pakete (± Rundung am Ende)
        assert!((49..=50).contains(&got.len()), "{}", got.len());
        assert!(got.iter().all(|b| b.len() == 64));
        // erstes Paket steigt an (linke Spur), Werte im Bereich
        let s: Vec<i16> = got[0].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert!(s.windows(2).all(|w| w[1] >= w[0]) && s[31] > 0);
    }
}
