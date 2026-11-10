//! Bildspeicher (RGBA) und Dekoder für Raw, CopyRect und ZRLE.
//! Unser Pixelformat: 32 bpp, Tiefe 24, Little Endian, R/G/B-Shift 0/8/16 – im Speicher also R,G,B,X.

use flate2::{Decompress, FlushDecompress};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    fn right(&self) -> u32 {
        u32::from(self.x) + u32::from(self.w)
    }
    fn bottom(&self) -> u32 {
        u32::from(self.y) + u32::from(self.h)
    }
    fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect { x, y, w: (self.right().max(o.right()) - u32::from(x)) as u16, h: (self.bottom().max(o.bottom()) - u32::from(y)) as u16 }
    }
    fn touches(&self, o: &Rect) -> bool {
        u32::from(self.x) <= o.right() && u32::from(o.x) <= self.right() && u32::from(self.y) <= o.bottom() && u32::from(o.y) <= self.bottom()
    }
}

/// Geänderte Fläche sammeln, bis die Oberfläche das nächste Bild abholt: Überlappendes wird vereint,
/// ab 32 Stücken alles zu einem Rechteck (weniger Kopien und Nachrichten als viele kleine).
pub fn add_dirty(list: &mut Vec<Rect>, mut r: Rect) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    while let Some(i) = list.iter().position(|d| d.touches(&r)) {
        r = r.union(&list.swap_remove(i));
    }
    list.push(r);
    if list.len() > 32 {
        let all = list.iter().fold(list[0], |a, b| a.union(b));
        *list = vec![all];
    }
}

pub struct Framebuffer {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

impl Framebuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self { width, height, rgba: vec![0; width as usize * height as usize * 4] }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        *self = Self::new(width, height);
    }

    /// Server-Rechtecke ausserhalb des Bildes sind ein Protokollfehler, kein Absturz.
    pub fn check(&self, r: Rect) -> Result<(), String> {
        if r.x as u32 + r.w as u32 > self.width as u32 || r.y as u32 + r.h as u32 > self.height as u32 {
            return Err(format!("Rechteck {r:?} liegt ausserhalb von {}×{}", self.width, self.height));
        }
        Ok(())
    }

    fn set(&mut self, x: usize, y: usize, px: [u8; 3]) {
        let i = (y * self.width as usize + x) * 4;
        self.rgba[i..i + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
    }

    /// RGBA-Kopie eines Ausschnitts, fürs Frontend.
    pub fn crop(&self, r: Rect) -> Vec<u8> {
        let mut out = Vec::with_capacity(r.w as usize * r.h as usize * 4);
        for row in r.y as usize..(r.y + r.h) as usize {
            let start = (row * self.width as usize + r.x as usize) * 4;
            out.extend_from_slice(&self.rgba[start..start + r.w as usize * 4]);
        }
        out
    }

    pub fn raw(&mut self, r: Rect, pixels: &[u8]) -> Result<(), String> {
        self.check(r)?;
        if pixels.len() != r.w as usize * r.h as usize * 4 {
            return Err("Raw-Daten haben die falsche Länge".into());
        }
        for (i, px) in pixels.chunks_exact(4).enumerate() {
            self.set(r.x as usize + i % r.w as usize, r.y as usize + i / r.w as usize, [px[0], px[1], px[2]]);
        }
        Ok(())
    }

    pub fn copy_rect(&mut self, r: Rect, src_x: u16, src_y: u16) -> Result<(), String> {
        self.check(r)?;
        self.check(Rect { x: src_x, y: src_y, ..r })?;
        let src = self.crop(Rect { x: src_x, y: src_y, ..r });
        let row = r.w as usize * 4;
        for (i, line) in src.chunks_exact(row).enumerate() {
            let start = ((r.y as usize + i) * self.width as usize + r.x as usize) * 4;
            self.rgba[start..start + row].copy_from_slice(line);
        }
        Ok(())
    }
}

/// Cursor-Pseudo-Encoding: Pixel (R,G,B,X) plus 1-Bit-Maske (Zeilen auf Byte aufgefüllt) → RGBA.
pub fn cursor(px: &[u8], mask: &[u8], w: u16, h: u16) -> Vec<u8> {
    let (w, stride) = (w as usize, (w as usize).div_ceil(8));
    let mut out = Vec::with_capacity(w * h as usize * 4);
    for (i, p) in px.chunks_exact(4).enumerate() {
        let (row, col) = (i / w, i % w);
        let visible = mask[row * stride + col / 8] >> (7 - col % 8) & 1 == 1;
        out.extend_from_slice(&[p[0], p[1], p[2], if visible { 255 } else { 0 }]);
    }
    out
}

/// Tight (Kodierung 7): vier zlib-Ströme, Füllfarbe, Kopie/Palette/Gradient und JPEG.
/// Pixel kommen als TPIXEL (3 Bytes R,G,B bei unserem 32-bpp-Format mit Tiefe 24).
#[derive(Default)]
pub struct Tight {
    z: [Option<Decompress>; 4],
}

/// Was nach dem Steuerbyte vom Server zu lesen ist (die Sitzung liest es asynchron vom Netz).
pub enum TightNeed {
    /// Füllfarbe: 3 Bytes.
    Fill,
    /// JPEG: kompakte Länge, dann so viele Bytes.
    Jpeg,
    /// Grundform: optional ein Filter-Byte, dann je nach Filter Palette und Daten.
    Basic { stream: usize, filter: bool },
}

impl Tight {
    /// Steuerbyte auswerten: Ströme zurücksetzen und sagen, was folgt.
    pub fn control(&mut self, ctrl: u8) -> Result<TightNeed, String> {
        for i in 0..4 {
            if ctrl & (1 << i) != 0 {
                self.z[i] = None;
            }
        }
        match ctrl >> 4 {
            8 => Ok(TightNeed::Fill),
            9 => Ok(TightNeed::Jpeg),
            c if c < 8 => Ok(TightNeed::Basic { stream: (c & 3) as usize, filter: c & 4 != 0 }),
            c => Err(format!("Tight: unbekannte Kompression {c}")),
        }
    }

    /// Länge der (entpackten) Bilddaten für Filter und Palettengrösse.
    pub fn data_len(r: Rect, filter: u8, colors: usize) -> usize {
        let (w, h) = (r.w as usize, r.h as usize);
        match filter {
            1 if colors <= 2 => w.div_ceil(8) * h,
            1 => w * h,
            _ => w * h * 3,
        }
    }

    /// Zlib-Daten eines Stroms entpacken (genau `len` Bytes erwartet).
    pub fn inflate(&mut self, stream: usize, input: &[u8], len: usize) -> Result<Vec<u8>, String> {
        if len > MAX_INFLATED {
            return Err("Tight-Daten zu gross".into());
        }
        let z = self.z[stream].get_or_insert_with(|| Decompress::new(true));
        let mut out = Vec::with_capacity(len);
        let start = z.total_in();
        while out.len() < len {
            let (in0, out0) = (z.total_in(), z.total_out());
            let used = (in0 - start) as usize;
            z.decompress_vec(&input[used..], &mut out, FlushDecompress::Sync).map_err(|e| format!("Tight: {e}"))?;
            if z.total_in() == in0 && z.total_out() == out0 {
                break;
            }
        }
        if out.len() != len {
            return Err("Tight: Daten unvollständig".into());
        }
        Ok(out)
    }

    pub fn fill(fb: &mut Framebuffer, r: Rect, px: [u8; 3]) -> Result<(), String> {
        fb.check(r)?;
        for y in r.y as usize..(r.y + r.h) as usize {
            for x in r.x as usize..(r.x + r.w) as usize {
                fb.set(x, y, px);
            }
        }
        Ok(())
    }

    /// Entpackte Daten mit Filter (0 Kopie, 1 Palette, 2 Gradient) ins Bild schreiben.
    pub fn basic(fb: &mut Framebuffer, r: Rect, filter: u8, palette: &[[u8; 3]], data: &[u8]) -> Result<(), String> {
        fb.check(r)?;
        let (w, h) = (r.w as usize, r.h as usize);
        if data.len() != Self::data_len(r, filter, palette.len()) {
            return Err("Tight: falsche Datenlänge".into());
        }
        let put = |fb: &mut Framebuffer, i: usize, px: [u8; 3]| fb.set(r.x as usize + i % w, r.y as usize + i / w, px);
        match filter {
            0 => data.chunks_exact(3).enumerate().for_each(|(i, p)| put(fb, i, [p[0], p[1], p[2]])),
            1 => {
                let color = |k: usize| palette.get(k).copied().ok_or("Tight: Farbe ausserhalb der Palette");
                if palette.len() <= 2 {
                    let stride = w.div_ceil(8);
                    for y in 0..h {
                        for x in 0..w {
                            put(fb, y * w + x, color((data[y * stride + x / 8] >> (7 - x % 8) & 1) as usize)?);
                        }
                    }
                } else {
                    for (i, &k) in data.iter().enumerate() {
                        put(fb, i, color(k as usize)?);
                    }
                }
            }
            2 => {
                // Gradient: Vorhersage links + oben − oben links je Kanal, auf 0..255 begrenzt.
                let mut prev = vec![0u8; w * 3];
                let mut cur = vec![0u8; w * 3];
                for y in 0..h {
                    for x in 0..w {
                        for c in 0..3 {
                            let left = if x > 0 { cur[(x - 1) * 3 + c] as i32 } else { 0 };
                            let up = prev[x * 3 + c] as i32;
                            let ul = if x > 0 { prev[(x - 1) * 3 + c] as i32 } else { 0 };
                            let pred = (left + up - ul).clamp(0, 255) as u8;
                            cur[x * 3 + c] = data[(y * w + x) * 3 + c].wrapping_add(pred);
                        }
                        put(fb, y * w + x, [cur[x * 3], cur[x * 3 + 1], cur[x * 3 + 2]]);
                    }
                    std::mem::swap(&mut prev, &mut cur);
                }
            }
            f => return Err(format!("Tight: unbekannter Filter {f}")),
        }
        Ok(())
    }

    /// Fertig dekodiertes JPEG (RGB oder RGBA, Zeile für Zeile) ins Bild schreiben.
    pub fn pixels(fb: &mut Framebuffer, r: Rect, w: usize, h: usize, spp: usize, stride: usize, data: &[u8]) -> Result<(), String> {
        fb.check(r)?;
        if w != r.w as usize || h != r.h as usize || spp < 3 || data.len() < stride * h {
            return Err("Tight: JPEG passt nicht zum Rechteck".into());
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * spp;
                fb.set(r.x as usize + x, r.y as usize + y, [data[i], data[i + 1], data[i + 2]]);
            }
        }
        Ok(())
    }
}

/// ZRLE nutzt einen zlib-Strom über die ganze Verbindung.
pub struct Zrle {
    z: Decompress,
}

impl Default for Zrle {
    fn default() -> Self {
        Self { z: Decompress::new(true) }
    }
}

/// Grenze gegen Speicherüberlauf durch bösartige Server.
const MAX_INFLATED: usize = 256 << 20;

impl Zrle {
    fn inflate(&mut self, input: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(input.len() * 4 + 4096);
        let start = self.z.total_in();
        loop {
            if out.len() == out.capacity() {
                if out.len() >= MAX_INFLATED {
                    return Err("ZRLE-Daten zu gross".into());
                }
                out.reserve(out.capacity());
            }
            let (in0, out0) = (self.z.total_in(), self.z.total_out());
            let used = (in0 - start) as usize;
            self.z
                .decompress_vec(&input[used..], &mut out, FlushDecompress::Sync)
                .map_err(|e| format!("ZRLE: {e}"))?;
            if (self.z.total_in() - start) as usize == input.len() && out.len() < out.capacity() {
                return Ok(out);
            }
            if self.z.total_in() == in0 && self.z.total_out() == out0 {
                return Err("ZRLE-Strom hängt".into());
            }
        }
    }

    pub fn decode(&mut self, fb: &mut Framebuffer, r: Rect, compressed: &[u8]) -> Result<(), String> {
        fb.check(r)?;
        let data = self.inflate(compressed)?;
        let mut rd = Reader { b: &data, i: 0 };
        for ty in (0..r.h as usize).step_by(64) {
            for tx in (0..r.w as usize).step_by(64) {
                let tw = 64.min(r.w as usize - tx);
                let th = 64.min(r.h as usize - ty);
                let (ox, oy) = (r.x as usize + tx, r.y as usize + ty);
                let put = |fb: &mut Framebuffer, i: usize, px: [u8; 3]| fb.set(ox + i % tw, oy + i / tw, px);
                let n = tw * th;
                match rd.u8()? {
                    0 => {
                        for i in 0..n {
                            put(fb, i, rd.cpixel()?);
                        }
                    }
                    1 => {
                        let px = rd.cpixel()?;
                        (0..n).for_each(|i| put(fb, i, px));
                    }
                    p @ 2..=16 => {
                        let pal = rd.palette(p as usize)?;
                        let bits = match p {
                            2 => 1,
                            3..=4 => 2,
                            _ => 4,
                        };
                        for row in 0..th {
                            let (mut byte, mut left) = (0u8, 0);
                            for col in 0..tw {
                                if left == 0 {
                                    byte = rd.u8()?;
                                    left = 8;
                                }
                                left -= bits;
                                let idx = (byte >> left) as usize & ((1 << bits) - 1);
                                put(fb, row * tw + col, *pal.get(idx).ok_or("ZRLE: Palettenindex zu gross")?);
                            }
                        }
                    }
                    s @ (128 | 130..=255) => {
                        let pal = if s == 128 { Vec::new() } else { rd.palette(s as usize - 128)? };
                        let mut i = 0;
                        while i < n {
                            let (px, len) = if s == 128 {
                                (rd.cpixel()?, rd.run()?)
                            } else {
                                let b = rd.u8()?;
                                let px = *pal.get((b & 127) as usize).ok_or("ZRLE: Palettenindex zu gross")?;
                                (px, if b & 128 != 0 { rd.run()? } else { 1 })
                            };
                            if i + len > n {
                                return Err("ZRLE: Lauflänge über Kachelende".into());
                            }
                            (i..i + len).for_each(|j| put(fb, j, px));
                            i += len;
                        }
                    }
                    s => return Err(format!("ZRLE: unbekannte Kachelart {s}")),
                }
            }
        }
        Ok(())
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or("ZRLE: Daten zu kurz")?;
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    /// CPIXEL: 3 Byte R,G,B (unser Format passt in die unteren 3 Byte).
    fn cpixel(&mut self) -> Result<[u8; 3], String> {
        let s = self.take(3)?;
        Ok([s[0], s[1], s[2]])
    }
    fn palette(&mut self, n: usize) -> Result<Vec<[u8; 3]>, String> {
        (0..n).map(|_| self.cpixel()).collect()
    }
    fn run(&mut self) -> Result<usize, String> {
        let mut len = 1;
        loop {
            let b = self.u8()?;
            len += b as usize;
            if b != 255 {
                return Ok(len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tight_fuellen_kopie_palette_gradient() {
        use super::{Framebuffer, Rect, Tight, TightNeed};
        use flate2::{Compress, Compression, FlushCompress};
        let mut fb = Framebuffer::new(4, 2);
        let r = Rect { x: 0, y: 0, w: 4, h: 2 };
        let px = |fb: &Framebuffer, x: usize, y: usize| fb.rgba[(y * 4 + x) * 4..(y * 4 + x) * 4 + 3].to_vec();
        let mut t = Tight::default();
        assert!(matches!(t.control(0x80).unwrap(), TightNeed::Fill));
        Tight::fill(&mut fb, r, [9, 8, 7]).unwrap();
        assert_eq!(px(&fb, 3, 1), [9, 8, 7]);
        // Kopie über Strom 1 (zlib), Steuerbyte 0x51: Strom 0 zurücksetzen, Strom 1, mit Filter-Byte
        assert!(matches!(t.control(0x51).unwrap(), TightNeed::Basic { stream: 1, filter: true }));
        let raw: Vec<u8> = (0..24).collect();
        let mut z = Compress::new(Compression::fast(), true);
        let mut comp = Vec::with_capacity(256);
        z.compress_vec(&raw, &mut comp, FlushCompress::Sync).unwrap();
        let data = t.inflate(1, &comp, Tight::data_len(r, 0, 0)).unwrap();
        Tight::basic(&mut fb, r, 0, &[], &data).unwrap();
        assert_eq!(px(&fb, 1, 0), [3, 4, 5]);
        // Palette mit 2 Farben: 1 Bit je Pixel, Zeilen auf Byte aufgefüllt
        Tight::basic(&mut fb, r, 1, &[[1, 1, 1], [2, 2, 2]], &[0b1010_0000, 0b0101_0000]).unwrap();
        assert_eq!((px(&fb, 0, 0), px(&fb, 1, 0), px(&fb, 0, 1)), (vec![2, 2, 2], vec![1, 1, 1], vec![1, 1, 1]));
        // Gradient: konstante Farbe ergibt nur im ersten Pixel einen Wert, danach Nullen
        let mut g = vec![0u8; 24];
        g[..3].copy_from_slice(&[50, 60, 70]);
        Tight::basic(&mut fb, r, 2, &[], &g).unwrap();
        assert_eq!(px(&fb, 3, 1), [50, 60, 70]);
        assert!(Tight::basic(&mut fb, r, 0, &[], &[0; 5]).is_err());
        assert!(t.control(0xA0).is_err(), "PNG gibt es nur in TightPNG");
    }
    #[test]
    fn dirty_vereint_ueberlappende_und_begrenzt_die_anzahl() {
        use super::{add_dirty, Rect};
        let mut l = Vec::new();
        add_dirty(&mut l, Rect { x: 0, y: 0, w: 10, h: 10 });
        add_dirty(&mut l, Rect { x: 5, y: 5, w: 10, h: 10 });
        assert_eq!(l, vec![Rect { x: 0, y: 0, w: 15, h: 15 }]);
        add_dirty(&mut l, Rect { x: 100, y: 100, w: 1, h: 1 });
        add_dirty(&mut l, Rect { x: 0, y: 0, w: 0, h: 5 });
        assert_eq!(l.len(), 2);
        for i in 0..40 {
            add_dirty(&mut l, Rect { x: 200 + i * 3, y: 300, w: 1, h: 1 });
        }
        assert!(l.len() <= 32, "zusammengefasst: {}", l.len());
        assert!(l.iter().any(|r| r.x == 0 && r.y == 0 && r.w >= 200 && r.h == 301), "eines umfasst alles Frühere");
    }
    use super::*;
    use flate2::{Compress, Compression, FlushCompress};

    fn px(fb: &Framebuffer, x: usize, y: usize) -> [u8; 4] {
        let i = (y * fb.width as usize + x) * 4;
        fb.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn raw_and_copy_rect() {
        let mut fb = Framebuffer::new(4, 4);
        let r = Rect { x: 1, y: 1, w: 2, h: 1 };
        fb.raw(r, &[10, 20, 30, 0, 40, 50, 60, 0]).unwrap();
        assert_eq!(px(&fb, 1, 1), [10, 20, 30, 255]);
        assert_eq!(px(&fb, 2, 1), [40, 50, 60, 255]);
        fb.copy_rect(Rect { x: 0, y: 3, w: 2, h: 1 }, 1, 1).unwrap();
        assert_eq!(px(&fb, 1, 3), [40, 50, 60, 255]);
        assert_eq!(fb.crop(Rect { x: 0, y: 3, w: 2, h: 1 }), vec![10, 20, 30, 255, 40, 50, 60, 255]);
        assert!(fb.raw(Rect { x: 3, y: 0, w: 2, h: 1 }, &[0; 8]).is_err(), "ausserhalb");
        assert!(fb.copy_rect(Rect { x: 0, y: 0, w: 1, h: 1 }, 9, 9).is_err());
    }

    fn zlib(c: &mut Compress, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + 64);
        c.compress_vec(data, &mut out, FlushCompress::Sync).unwrap();
        out
    }

    #[test]
    fn zrle_tiles() {
        let mut fb = Framebuffer::new(70, 2);
        let mut z = Zrle::default();
        let mut c = Compress::new(Compression::default(), true);

        // Rechteck 70×2 = zwei Kacheln: 64×2 einfarbig, 6×2 mit Palette (2 Farben, 1 Bit).
        let mut tiles = vec![1, 255, 0, 0];
        tiles.extend([2, 0, 255, 0, 0, 0, 255, 0b0101_0100, 0b1010_1000]);
        z.decode(&mut fb, Rect { x: 0, y: 0, w: 70, h: 2 }, &zlib(&mut c, &tiles)).unwrap();
        assert_eq!(px(&fb, 63, 1), [255, 0, 0, 255]);
        assert_eq!(px(&fb, 64, 0), [0, 255, 0, 255]);
        assert_eq!(px(&fb, 65, 0), [0, 0, 255, 255]);
        assert_eq!(px(&fb, 64, 1), [0, 0, 255, 255]);
        assert_eq!(px(&fb, 65, 1), [0, 255, 0, 255]);

        // Zweite Nachricht im selben Strom: RLE (128) und Paletten-RLE (130).
        let mut tiles = vec![128, 9, 9, 9, 2, 1, 2, 3, 0];
        tiles.extend([130, 7, 7, 7, 8, 8, 8, 1, 0x80, 1]);
        let r = Rect { x: 0, y: 0, w: 4, h: 1 };
        let mut both = zlib(&mut c, &tiles[..9]);
        z.decode(&mut fb, r, &both).unwrap();
        assert_eq!(px(&fb, 2, 0), [9, 9, 9, 255]);
        assert_eq!(px(&fb, 3, 0), [1, 2, 3, 255]);
        both = zlib(&mut c, &tiles[9..]);
        z.decode(&mut fb, Rect { x: 0, y: 1, w: 3, h: 1 }, &both).unwrap();
        assert_eq!(px(&fb, 0, 1), [8, 8, 8, 255]);
        assert_eq!(px(&fb, 2, 1), [7, 7, 7, 255]);
    }

    #[test]
    fn cursor_mask_sets_alpha() {
        // 9×1: zweites Maskenbyte nötig.
        let px = [7u8; 9 * 4];
        let rgba = cursor(&px, &[0b1000_0001, 0b1000_0000], 9, 1);
        let alpha: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [255, 0, 0, 0, 0, 0, 0, 255, 255]);
    }

    #[test]
    fn zrle_rejects_overlong_run() {
        let mut fb = Framebuffer::new(2, 1);
        let mut c = Compress::new(Compression::default(), true);
        let bad = zlib(&mut c, &[128, 1, 2, 3, 5]);
        assert!(Zrle::default().decode(&mut fb, Rect { x: 0, y: 0, w: 2, h: 1 }, &bad).is_err());
    }
}
