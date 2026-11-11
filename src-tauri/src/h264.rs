//! H.264 für EGFX (AVC420/AVC444) über den Decoder des Betriebssystems, ohne eigene Codec-Bibliothek
//! (Lizenz- und Patentfragen in docs/architektur.md). Windows: Media Foundation (Microsoft H.264 Video Decoder MFT),
//! macOS und iPadOS: VideoToolbox. Ohne Decoder (Linux) kündigt EGFX kein AVC an und der Server nimmt andere Codecs.

use ironrdp_egfx::decode::{DecodedFrame, DecoderError, DecoderResult, H264Decoder, Yuv420Frame};

/// Decoder für die Graphics Pipeline, falls das System einen hat.
pub fn decoder() -> Option<Box<dyn H264Decoder>> {
    #[cfg(windows)]
    return Some(Box::new(mf::MfDecoder::default()));
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    return Some(Box::new(vt::VtDecoder::default()));
    #[cfg(not(any(windows, target_os = "macos", target_os = "ios")))]
    None
}

/// Annex B (Startcodes) unverändert; AVC-Format (4-Byte-Längen, wie frühere IronRDP-Server) in Annex B umschreiben.
pub fn to_annex_b(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if data.starts_with(&[0, 0, 1]) || data.starts_with(&[0, 0, 0, 1]) {
        return data.into();
    }
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut rest = data;
    while rest.len() >= 4 {
        let n = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let Some(nal) = rest.get(4..4 + n) else { return data.into() };
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        rest = &rest[4 + n..];
    }
    if rest.is_empty() && !out.is_empty() { out.into() } else { data.into() }
}

/// NV12 (Y-Ebene, dann U/V verschränkt; `rows` Zeilen bis zur UV-Ebene) in drei Ebenen für `Yuv420Frame`.
pub fn nv12_to_yuv420(buf: &[u8], width: u16, height: u16, stride: usize, rows: usize) -> DecoderResult<Yuv420Frame> {
    let (cw, ch) = (usize::from(width.div_ceil(2)), usize::from(height.div_ceil(2)));
    let y_len = stride * usize::from(height);
    let uv_start = stride * rows;
    if stride < usize::from(width) || rows < usize::from(height) || buf.len() < uv_start + stride * ch {
        return Err(DecoderError::msg("NV12-Puffer zu klein"));
    }
    let (mut u, mut v) = (Vec::with_capacity(cw * ch), Vec::with_capacity(cw * ch));
    for row in buf[uv_start..].chunks(stride).take(ch) {
        for p in row[..cw * 2].chunks_exact(2) {
            u.push(p[0]);
            v.push(p[1]);
        }
    }
    Yuv420Frame::new(width, height, buf[..y_len].to_vec(), stride, u, v, cw)
}

#[cfg(windows)]
mod mf {
    use super::*;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx};

    fn err(ctx: &str, e: windows::core::Error) -> DecoderError {
        DecoderError::msg(format!("Media Foundation: {ctx}: {e}"))
    }

    /// COM (MTA) und Media Foundation einmal pro Thread; die Sitzung läuft auf wechselnden Tokio-Threads.
    fn init_thread() {
        thread_local!(static INIT: () = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET);
        });
        INIT.with(|_| ());
    }

    #[derive(Default)]
    pub struct MfDecoder {
        mft: Option<IMFTransform>,
        /// Bildgrösse und Zeilenlänge der Ausgabe (nach jedem Formatwechsel neu).
        width: u32,
        height: u32,
        stride: u32,
        time: i64,
    }

    // SAFETY: Der H.264-Decoder-MFT ist free-threaded ("Both"); er wird nur aus MTA-Threads und nie gleichzeitig benutzt.
    unsafe impl Send for MfDecoder {}

    impl MfDecoder {
        fn start(&mut self) -> Result<(), windows::core::Error> {
            init_thread();
            unsafe {
                let mft: IMFTransform = CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
                // Ein Bild pro Eingabe, ohne Puffer für B-Frames: RDP schickt keine, AVC444 braucht jedes Bild sofort.
                let attrs = mft.GetAttributes()?;
                attrs.SetUINT32(&CODECAPI_AVLowLatencyMode, 1)?;
                let input = MFCreateMediaType()?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
                mft.SetInputType(0, &input, 0)?;
                self.mft = Some(mft);
                self.choose_output()?;
                let mft = self.mft.as_ref().unwrap();
                mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
                mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            }
            Ok(())
        }

        /// NV12 als Ausgabe wählen und Grösse/Zeilenlänge übernehmen.
        fn choose_output(&mut self) -> Result<(), windows::core::Error> {
            let mft = self.mft.as_ref().unwrap();
            unsafe {
                for i in 0.. {
                    let t = mft.GetOutputAvailableType(0, i)?;
                    if t.GetGUID(&MF_MT_SUBTYPE)? == MFVideoFormat_NV12 {
                        mft.SetOutputType(0, &t, 0)?;
                        let size = t.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                        (self.width, self.height) = ((size >> 32) as u32, size as u32);
                        // Negative Zeilenlänge (von unten nach oben) kommt bei NV12 nicht vor.
                        self.stride = t.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(self.width).max(self.width);
                        return Ok(());
                    }
                }
            }
            unreachable!()
        }

        fn feed(&mut self, data: &[u8]) -> Result<(), windows::core::Error> {
            let mft = self.mft.as_ref().unwrap();
            unsafe {
                let buf = MFCreateMemoryBuffer(data.len() as u32)?;
                let mut p = std::ptr::null_mut();
                buf.Lock(&mut p, None, None)?;
                std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                buf.Unlock()?;
                buf.SetCurrentLength(data.len() as u32)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buf)?;
                sample.SetSampleTime(self.time)?;
                sample.SetSampleDuration(1)?;
                self.time += 1;
                mft.ProcessInput(0, &sample, 0)
            }
        }

        /// Nächstes Bild als NV12-Bytes, `None` wenn der Decoder noch mehr Eingabe braucht.
        fn output(&mut self) -> Result<Option<Vec<u8>>, windows::core::Error> {
            loop {
                let mft = self.mft.as_ref().unwrap();
                unsafe {
                    let info = mft.GetOutputStreamInfo(0)?;
                    let provides = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32) != 0;
                    let sample = if provides {
                        None
                    } else {
                        let s = MFCreateSample()?;
                        s.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(self.stride * self.height * 3 / 2))?)?;
                        Some(s)
                    };
                    let mut out = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: std::mem::ManuallyDrop::new(sample), dwStatus: 0, pEvents: Default::default() }];
                    let mut status = 0;
                    let r = mft.ProcessOutput(0, &mut out, &mut status);
                    let sample = std::mem::ManuallyDrop::take(&mut out[0].pSample);
                    drop(std::mem::ManuallyDrop::take(&mut out[0].pEvents));
                    match r {
                        Ok(()) => {
                            let buf = sample.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_UNEXPECTED))?.ConvertToContiguousBuffer()?;
                            let (mut p, mut len) = (std::ptr::null_mut(), 0u32);
                            buf.Lock(&mut p, None, Some(&mut len))?;
                            let bytes = std::slice::from_raw_parts(p, len as usize).to_vec();
                            buf.Unlock()?;
                            return Ok(Some(bytes));
                        }
                        Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.choose_output()?,
                        Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    impl H264Decoder for MfDecoder {
        fn decode(&mut self, data: &[u8]) -> DecoderResult<DecodedFrame> {
            self.decode_yuv420(data).map(|f| f.to_rgba())
        }

        fn decode_yuv420(&mut self, data: &[u8]) -> DecoderResult<Yuv420Frame> {
            init_thread();
            if self.mft.is_none() {
                self.start().map_err(|e| err("Start", e))?;
            }
            self.feed(&to_annex_b(data)).map_err(|e| err("Eingabe", e))?;
            let nv12 = self.output().map_err(|e| err("Ausgabe", e))?.ok_or_else(|| DecoderError::msg("H.264: kein Bild"))?;
            let (w, h) = (u16::try_from(self.width).unwrap_or(0), u16::try_from(self.height).unwrap_or(0));
            // Zeilen bis zur UV-Ebene: aus der Puffergrösse (Höhe ggf. auf 16 aufgerundet).
            let stride = self.stride as usize;
            let rows = if stride == 0 { 0 } else { nv12.len() * 2 / 3 / stride };
            nv12_to_yuv420(&nv12, w, h, stride, rows)
        }

        fn supports_yuv420(&self) -> bool {
            true
        }

        fn wait_until_ready(&mut self) -> bool {
            match self.start() {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(error = %e, "H.264: Media Foundation nicht verfügbar, EGFX ohne AVC");
                    self.mft = None;
                    false
                }
            }
        }

        fn reset(&mut self) {
            if let Some(mft) = &self.mft {
                let _ = unsafe { mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0) };
            }
        }
    }

    impl Drop for MfDecoder {
        fn drop(&mut self) {
            init_thread();
            self.mft = None;
        }
    }
}

/// NAL-Einheiten eines Annex-B-Stroms (ohne Startcodes).
#[cfg(any(test, target_os = "macos", target_os = "ios"))]
pub fn nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &a)| {
            let mut b = starts.get(k + 1).map_or(data.len(), |&n| n - 3);
            while b > a && data[b - 1] == 0 {
                b -= 1; // Null vor dem nächsten 4-Byte-Startcode bzw. trailing_zero_8bits
            }
            &data[a..b]
        })
        .filter(|n| !n.is_empty())
        .collect()
}

/// macOS und iPadOS: VideoToolbox über die C-Schnittstelle (CoreMedia/CoreVideo), ohne zusätzliche Pakete.
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod vt {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    type Ref = *mut c_void;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CMTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }
    #[repr(C)]
    struct CallbackRecord {
        cb: extern "C" fn(*mut c_void, *mut c_void, i32, u32, Ref, CMTime, CMTime),
        refcon: *mut c_void,
    }

    #[link(name = "VideoToolbox", kind = "framework")]
    #[link(name = "CoreMedia", kind = "framework")]
    #[link(name = "CoreVideo", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CMVideoFormatDescriptionCreateFromH264ParameterSets(a: Ref, n: usize, p: *const *const u8, s: *const usize, nal_len: i32, out: *mut Ref) -> i32;
        fn VTDecompressionSessionCreate(a: Ref, fmt: Ref, spec: Ref, attrs: Ref, cb: *const CallbackRecord, out: *mut Ref) -> i32;
        fn VTDecompressionSessionDecodeFrame(s: Ref, sample: Ref, flags: u32, frame: *mut c_void, info: *mut u32) -> i32;
        fn VTDecompressionSessionWaitForAsynchronousFrames(s: Ref) -> i32;
        fn VTDecompressionSessionInvalidate(s: Ref);
        fn CMBlockBufferCreateWithMemoryBlock(a: Ref, mem: *mut c_void, len: usize, ba: Ref, custom: *const c_void, off: usize, dl: usize, flags: u32, out: *mut Ref) -> i32;
        fn CMBlockBufferReplaceDataBytes(src: *const c_void, dst: Ref, off: usize, len: usize) -> i32;
        fn CMSampleBufferCreateReady(a: Ref, data: Ref, fmt: Ref, n: isize, nt: isize, t: *const c_void, ns: isize, sizes: *const usize, out: *mut Ref) -> i32;
        fn CVPixelBufferLockBaseAddress(pb: Ref, f: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(pb: Ref, f: u64) -> i32;
        fn CVPixelBufferGetBaseAddressOfPlane(pb: Ref, plane: usize) -> *const u8;
        fn CVPixelBufferGetBytesPerRowOfPlane(pb: Ref, plane: usize) -> usize;
        fn CVPixelBufferGetHeightOfPlane(pb: Ref, plane: usize) -> usize;
        fn CVPixelBufferGetWidth(pb: Ref) -> usize;
        fn CVPixelBufferGetHeight(pb: Ref) -> usize;
        fn CFRelease(r: *const c_void);
        fn CFNumberCreate(a: Ref, ty: isize, v: *const c_void) -> Ref;
        fn CFDictionaryCreate(a: Ref, k: *const *const c_void, v: *const *const c_void, n: isize, kc: *const c_void, vc: *const c_void) -> Ref;
        static kCVPixelBufferPixelFormatTypeKey: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
    }

    /// Bild aus dem Rückruf: NV12 zusammenhängend, Breite, Höhe, Zeilenlänge, Zeilen bis zur UV-Ebene.
    type Out = Option<(Vec<u8>, u16, u16, usize, usize)>;

    extern "C" fn on_frame(_: *mut c_void, frame: *mut c_void, status: i32, _: u32, img: Ref, _: CMTime, _: CMTime) {
        if status != 0 || img.is_null() {
            return;
        }
        let out = unsafe { &mut *(frame as *mut Out) };
        unsafe {
            CVPixelBufferLockBaseAddress(img, 1); // nur lesen
            let (sy, suv) = (CVPixelBufferGetBytesPerRowOfPlane(img, 0), CVPixelBufferGetBytesPerRowOfPlane(img, 1));
            let (hy, huv) = (CVPixelBufferGetHeightOfPlane(img, 0), CVPixelBufferGetHeightOfPlane(img, 1));
            let (y, uv) = (CVPixelBufferGetBaseAddressOfPlane(img, 0), CVPixelBufferGetBaseAddressOfPlane(img, 1));
            if sy == suv && !y.is_null() && !uv.is_null() {
                let mut buf = Vec::with_capacity(sy * (hy + huv));
                buf.extend_from_slice(std::slice::from_raw_parts(y, sy * hy));
                buf.extend_from_slice(std::slice::from_raw_parts(uv, suv * huv));
                let (w, h) = (CVPixelBufferGetWidth(img), CVPixelBufferGetHeight(img));
                *out = Some((buf, u16::try_from(w).unwrap_or(0), u16::try_from(h).unwrap_or(0), sy, hy));
            }
            CVPixelBufferUnlockBaseAddress(img, 1);
        }
    }

    #[derive(Default)]
    pub struct VtDecoder {
        session: Option<Ref>,
        fmt: Option<Ref>,
        sps: Vec<u8>,
        pps: Vec<u8>,
    }
    // Die Sitzung gehört genau einem Decoder; VideoToolbox ist nicht an einen Thread gebunden.
    unsafe impl Send for VtDecoder {}

    fn fail(ctx: &str, status: i32) -> DecoderError {
        DecoderError::msg(format!("H.264 (VideoToolbox) {ctx}: Status {status}"))
    }

    impl VtDecoder {
        fn close(&mut self) {
            unsafe {
                if let Some(s) = self.session.take() {
                    VTDecompressionSessionInvalidate(s);
                    CFRelease(s);
                }
                if let Some(f) = self.fmt.take() {
                    CFRelease(f);
                }
            }
        }

        fn open(&mut self) -> DecoderResult<()> {
            self.close();
            let sets = [self.sps.as_ptr(), self.pps.as_ptr()];
            let sizes = [self.sps.len(), self.pps.len()];
            let mut fmt = null_mut();
            let st = unsafe { CMVideoFormatDescriptionCreateFromH264ParameterSets(null_mut(), 2, sets.as_ptr(), sizes.as_ptr(), 4, &mut fmt) };
            if st != 0 {
                return Err(fail("Format", st));
            }
            self.fmt = Some(fmt);
            // NV12 in Videobereich ('420v'), wie Media Foundation unter Windows liefert.
            let nv12: i32 = 0x3432_3076;
            let mut session = null_mut();
            let st = unsafe {
                let num = CFNumberCreate(null_mut(), 3 /* kCFNumberSInt32Type */, (&nv12 as *const i32).cast());
                let (k, v) = ([kCVPixelBufferPixelFormatTypeKey], [num as *const c_void]);
                let attrs = CFDictionaryCreate(null_mut(), k.as_ptr(), v.as_ptr(), 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
                let cb = CallbackRecord { cb: on_frame, refcon: null_mut() };
                let st = VTDecompressionSessionCreate(null_mut(), fmt, null_mut(), attrs, &cb, &mut session);
                CFRelease(attrs);
                CFRelease(num);
                st
            };
            if st != 0 {
                return Err(fail("Sitzung", st));
            }
            self.session = Some(session);
            Ok(())
        }

        /// Ein Zugriffseinheit (alle Bild-NALs eines Aufrufs) im AVC-Format dekodieren, synchron.
        fn frame(&mut self, avcc: &[u8]) -> DecoderResult<Out> {
            let (Some(session), Some(fmt)) = (self.session, self.fmt) else { return Err(DecoderError::msg("H.264: kein SPS/PPS")) };
            let mut out: Out = None;
            unsafe {
                let mut block = null_mut();
                let st = CMBlockBufferCreateWithMemoryBlock(null_mut(), null_mut(), avcc.len(), null_mut(), null(), 0, avcc.len(), 1 /* AssureMemoryNow */, &mut block);
                if st != 0 {
                    return Err(fail("Puffer", st));
                }
                CMBlockBufferReplaceDataBytes(avcc.as_ptr().cast(), block, 0, avcc.len());
                let mut sample = null_mut();
                let size = avcc.len();
                let st = CMSampleBufferCreateReady(null_mut(), block, fmt, 1, 0, null(), 1, &size, &mut sample);
                CFRelease(block);
                if st != 0 {
                    return Err(fail("Probe", st));
                }
                let mut info = 0u32;
                let st = VTDecompressionSessionDecodeFrame(session, sample, 0, (&mut out as *mut Out).cast(), &mut info);
                VTDecompressionSessionWaitForAsynchronousFrames(session);
                CFRelease(sample);
                if st != 0 {
                    return Err(fail("Dekodieren", st));
                }
            }
            Ok(out)
        }
    }

    impl H264Decoder for VtDecoder {
        fn decode(&mut self, data: &[u8]) -> DecoderResult<DecodedFrame> {
            self.decode_yuv420(data).map(|f| f.to_rgba())
        }

        fn decode_yuv420(&mut self, data: &[u8]) -> DecoderResult<Yuv420Frame> {
            let data = to_annex_b(data);
            let (mut sps, mut pps, mut avcc) = (None, None, Vec::with_capacity(data.len() + 16));
            for nal in nal_units(&data) {
                match nal[0] & 0x1f {
                    7 => sps = Some(nal),
                    8 => pps = Some(nal),
                    9 | 6 => {} // Trennzeichen, SEI: nicht nötig
                    _ => {
                        avcc.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                        avcc.extend_from_slice(nal);
                    }
                }
            }
            // Neue Parametersätze (Auflösung geändert, neuer Strom): Sitzung neu aufbauen.
            let changed = sps.is_some_and(|s| s != self.sps) || pps.is_some_and(|p| p != self.pps);
            if changed || self.session.is_none() {
                if let Some(s) = sps {
                    self.sps = s.to_vec();
                }
                if let Some(p) = pps {
                    self.pps = p.to_vec();
                }
                if self.sps.is_empty() || self.pps.is_empty() {
                    return Err(DecoderError::msg("H.264: kein SPS/PPS"));
                }
                self.open()?;
            }
            if avcc.is_empty() {
                return Err(DecoderError::msg("H.264: kein Bild"));
            }
            let (nv12, w, h, stride, rows) = self.frame(&avcc)?.ok_or_else(|| DecoderError::msg("H.264: kein Bild"))?;
            nv12_to_yuv420(&nv12, w, h, stride, rows)
        }

        fn supports_yuv420(&self) -> bool {
            true
        }

        fn wait_until_ready(&mut self) -> bool {
            true // VideoToolbox gibt es auf jedem unterstützten Mac
        }

        fn reset(&mut self) {
            self.close();
        }
    }

    impl Drop for VtDecoder {
        fn drop(&mut self) {
            self.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annex_b_passthrough_and_avc_conversion() {
        let annex = [0, 0, 0, 1, 0x67, 1, 2];
        assert_eq!(&*to_annex_b(&annex), &annex);
        let avc = [0, 0, 0, 2, 0x67, 9, 0, 0, 0, 1, 0x68];
        assert_eq!(&*to_annex_b(&avc), &[0, 0, 0, 1, 0x67, 9, 0, 0, 0, 1, 0x68]);
        // Länge passt nicht: unverändert lassen, der Decoder meldet den Fehler.
        assert_eq!(&*to_annex_b(&[0, 0, 0, 9, 1]), &[0, 0, 0, 9, 1]);
    }

    #[test]
    fn nv12_split_with_padding() {
        // 4x2 Bild, Zeilenlänge 6, Y-Ebene auf 4 Zeilen aufgefüllt.
        let (w, h, stride, rows) = (4u16, 2u16, 6usize, 4usize);
        let mut buf = vec![0u8; stride * rows + stride];
        buf[..6].copy_from_slice(&[1, 2, 3, 4, 0, 0]);
        buf[6..12].copy_from_slice(&[5, 6, 7, 8, 0, 0]);
        buf[24..30].copy_from_slice(&[10, 20, 11, 21, 0, 0]);
        let f = nv12_to_yuv420(&buf, w, h, stride, rows).unwrap();
        let rgba = f.to_rgba();
        assert_eq!(rgba.width(), 4);
        assert!(nv12_to_yuv420(&buf[..20], w, h, stride, rows).is_err());
    }

    /// Kleinster gültiger H.264-Strom: SPS, PPS und ein IDR-Bild nur aus I_PCM-Makroblöcken (Rohwerte, verlustfrei).
    #[cfg(any(windows, target_os = "macos"))]
    fn ipcm_stream(mbs_w: u32, mbs_h: u32, idr_id: u32, (y, u, v): (u8, u8, u8)) -> Vec<u8> {
        #[derive(Default)]
        struct Bits(Vec<u8>, u32);
        impl Bits {
            fn bit(&mut self, b: u32) {
                if self.1 % 8 == 0 {
                    self.0.push(0);
                }
                if b != 0 {
                    *self.0.last_mut().unwrap() |= 0x80 >> (self.1 % 8);
                }
                self.1 += 1;
            }
            fn u(&mut self, n: u32, v: u32) {
                (0..n).rev().for_each(|i| self.bit((v >> i) & 1));
            }
            fn ue(&mut self, v: u32) {
                let x = v + 1;
                let len = 32 - x.leading_zeros();
                self.u(len - 1, 0);
                self.u(len, x);
            }
            fn align_zero(&mut self) {
                while self.1 % 8 != 0 {
                    self.bit(0);
                }
            }
            fn trailing(mut self) -> Vec<u8> {
                self.bit(1);
                self.align_zero();
                self.0
            }
        }
        fn nal(out: &mut Vec<u8>, header: u8, rbsp: Vec<u8>) {
            out.extend_from_slice(&[0, 0, 0, 1, header]);
            let mut zeros = 0;
            for b in rbsp {
                if zeros >= 2 && b <= 3 {
                    out.push(3);
                    zeros = 0;
                }
                zeros = if b == 0 { zeros + 1 } else { 0 };
                out.push(b);
            }
        }
        let mut out = Vec::new();
        let mut sps = Bits::default();
        sps.u(8, 66); // Baseline
        sps.u(8, 0xC0); // constraint_set0/1
        sps.u(8, 30);
        sps.ue(0);
        sps.ue(0); // log2_max_frame_num_minus4
        sps.ue(2); // pic_order_cnt_type
        sps.ue(1); // max_num_ref_frames
        sps.bit(0);
        sps.ue(mbs_w - 1);
        sps.ue(mbs_h - 1);
        sps.bit(1); // frame_mbs_only
        sps.bit(1); // direct_8x8_inference
        sps.bit(0); // cropping
        sps.bit(0); // vui
        nal(&mut out, 0x67, sps.trailing());
        let mut pps = Bits::default();
        pps.ue(0);
        pps.ue(0);
        pps.bit(0); // CAVLC
        pps.bit(0);
        pps.ue(0);
        pps.ue(0);
        pps.ue(0);
        pps.bit(0);
        pps.u(2, 0);
        pps.ue(0); // pic_init_qp_minus26 (se 0 = ue 0)
        pps.ue(0);
        pps.ue(0);
        pps.bit(1); // deblocking_filter_control_present
        pps.bit(0);
        pps.bit(0);
        nal(&mut out, 0x68, pps.trailing());
        let mut s = Bits::default();
        s.ue(0); // first_mb_in_slice
        s.ue(7); // I
        s.ue(0); // pps
        s.u(4, 0); // frame_num
        s.ue(idr_id);
        s.bit(0); // no_output_of_prior_pics
        s.bit(0); // long_term_reference
        s.ue(0); // slice_qp_delta
        s.ue(1); // Deblocking aus
        for _ in 0..mbs_w * mbs_h {
            s.ue(25); // I_PCM
            s.align_zero();
            (0..256).for_each(|_| s.u(8, y.into()));
            (0..64).for_each(|_| s.u(8, u.into()));
            (0..64).for_each(|_| s.u(8, v.into()));
        }
        nal(&mut out, 0x65, s.trailing());
        out
    }

    /// Echter Decoder des Systems: zwei Bilder hintereinander, je eines pro Aufruf, Werte exakt (I_PCM).
    #[cfg(windows)]
    #[test]
    fn media_foundation_decodes_ipcm() {
        let mut d = mf::MfDecoder::default();
        if !d.wait_until_ready() {
            eprintln!("Media Foundation ohne H.264 (z. B. Windows N): Test übersprungen");
            return;
        }
        for (i, px) in [(100, 110, 120), (50, 60, 70)].into_iter().enumerate() {
            let f = d.decode_yuv420(&ipcm_stream(4, 4, i as u32, px)).unwrap();
            let rgba = f.to_rgba();
            assert_eq!((rgba.width(), rgba.height()), (64, 64));
            let reference = Yuv420Frame::new(1, 1, vec![px.0], 1, vec![px.1], vec![px.2], 1).unwrap().to_rgba();
            assert_eq!(&rgba.data()[..4], reference.data(), "Bild {i}");
            assert_eq!(&rgba.data()[rgba.data().len() - 4..], reference.data(), "Bild {i}");
        }
    }

    #[test]
    fn nal_units_split() {
        let s = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 3, 0];
        assert_eq!(nal_units(&s), vec![&[0x67, 1][..], &[0x68, 2][..], &[0x65, 3][..]]);
    }

    /// VideoToolbox: dieselben zwei I_PCM-Bilder wie unter Windows, Werte exakt.
    #[cfg(target_os = "macos")]
    #[test]
    fn videotoolbox_decodes_ipcm() {
        let mut d = super::vt::VtDecoder::default();
        for (i, px) in [(100, 110, 120), (50, 60, 70)].into_iter().enumerate() {
            let f = d.decode_yuv420(&ipcm_stream(4, 4, i as u32, px)).unwrap();
            let rgba = f.to_rgba();
            assert_eq!((rgba.width(), rgba.height()), (64, 64));
            let reference = Yuv420Frame::new(1, 1, vec![px.0], 1, vec![px.1], vec![px.2], 1).unwrap().to_rgba();
            assert_eq!(&rgba.data()[..4], reference.data(), "Bild {i}");
            assert_eq!(&rgba.data()[rgba.data().len() - 4..], reference.data(), "Bild {i}");
        }
    }
}
