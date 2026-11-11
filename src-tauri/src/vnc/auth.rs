//! RFB-Anmeldeverfahren: VNC-Auth (Typ 2) und Apple Remote Desktop (Typ 30, Diffie-Hellman).

use aes::Aes128;
use des::Des;
use des::cipher::{BlockCipherEncrypt, KeyInit};
use num_bigint::BigUint;

/// VNC-Auth: Challenge mit DES verschlüsseln, Schlüssel = Passwort (max. 8 Byte) mit gespiegelten Bits.
pub fn vnc_response(password: &[u8], challenge: &[u8; 16]) -> [u8; 16] {
    let mut key = [0u8; 8];
    for (k, p) in key.iter_mut().zip(password) {
        *k = p.reverse_bits();
    }
    let cipher = Des::new_from_slice(&key).expect("DES-Schlüssel hat 8 Byte");
    let mut out = *challenge;
    for chunk in out.as_chunks_mut::<8>().0 {
        let mut b = des::cipher::Block::<Des>::default();
        b.copy_from_slice(chunk);
        cipher.encrypt_block(&mut b);
        chunk.copy_from_slice(&b);
    }
    out
}

/// Apple-Auth (Typ 30): Antwort = AES-128-ECB(Benutzer|Passwort, MD5(DH-Geheimnis)) + öffentlicher Client-Schlüssel.
pub fn ard_response(generator: &[u8], prime: &[u8], server_pub: &[u8], user: &str, password: &str) -> Result<Vec<u8>, String> {
    let mut private = vec![0u8; prime.len()];
    let mut filler = [0u8; 128];
    getrandom::fill(&mut private).and_then(|_| getrandom::fill(&mut filler)).map_err(|e| e.to_string())?;
    ard_with(generator, prime, server_pub, user, password, &private, filler)
}

fn ard_with(
    generator: &[u8],
    prime: &[u8],
    server_pub: &[u8],
    user: &str,
    password: &str,
    private: &[u8],
    mut creds: [u8; 128],
) -> Result<Vec<u8>, String> {
    let len = prime.len();
    if !(64..=1024).contains(&len) || server_pub.len() != len {
        return Err("Ungültige Apple-Auth-Parameter".into());
    }
    let p = BigUint::from_bytes_be(prime);
    let g = BigUint::from_bytes_be(generator);
    // Keine schwachen DH-Werte vom Server (1, p-1, ausserhalb): sonst wäre das gemeinsame Geheimnis vorhersagbar
    // (security-review-2026-11-11, S-03).
    let one = BigUint::from(1u8);
    let y = BigUint::from_bytes_be(server_pub);
    let pm1 = &p - &one;
    if g <= one || g >= pm1 || y <= one || y >= pm1 {
        return Err("Ungültige Apple-Auth-Parameter".into());
    }
    let x = BigUint::from_bytes_be(private);
    let client_pub = g.modpow(&x, &p);
    let shared = BigUint::from_bytes_be(server_pub).modpow(&x, &p);
    let key = md5::compute(left_pad(&shared.to_bytes_be(), len)).0;

    // Je 64 Byte, nullterminiert, Rest bleibt zufällig.
    for (field, s) in [(0, user), (64, password)] {
        let b = s.as_bytes();
        if b.len() > 63 {
            return Err("Benutzer oder Passwort länger als 63 Byte".into());
        }
        creds[field..field + b.len()].copy_from_slice(b);
        creds[field + b.len()] = 0;
    }
    let aes = Aes128::new_from_slice(&key).expect("AES-Schlüssel hat 16 Byte");
    for chunk in creds.as_chunks_mut::<16>().0 {
        let mut b = aes::cipher::Block::<Aes128>::default();
        b.copy_from_slice(chunk);
        aes.encrypt_block(&mut b);
        chunk.copy_from_slice(&b);
    }
    let mut out = creds.to_vec();
    out.extend(left_pad(&client_pub.to_bytes_be(), len));
    Ok(out)
}

fn left_pad(b: &[u8], len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len.saturating_sub(b.len())];
    v.extend_from_slice(b);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockCipherDecrypt;

    #[test]
    fn vnc_des_matches_fips_vector() {
        // FIPS-81-Beispiel: Schlüssel 0123456789ABCDEF, «Now is t» -> 3FA40E8A984D4815.
        // VNC spiegelt die Bits jedes Passwort-Bytes, also das gespiegelte Passwort übergeben.
        let pw: Vec<u8> = [0x01u8, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF].iter().map(|b| b.reverse_bits()).collect();
        let mut ch = [0u8; 16];
        ch[..8].copy_from_slice(b"Now is t");
        ch[8..].copy_from_slice(b"Now is t");
        let r = vnc_response(&pw, &ch);
        let want = [0x3F, 0xA4, 0x0E, 0x8A, 0x98, 0x4D, 0x48, 0x15];
        assert_eq!(&r[..8], &want);
        assert_eq!(&r[8..], &want);
    }

    #[test]
    fn ard_server_can_decrypt() {
        // 2^521 - 1 ist prim (66 Byte); reicht für den Test.
        let prime = (BigUint::from(1u8) << 521u32) - 1u8;
        let n = 66;
        let p = left_pad(&prime.to_bytes_be(), n);
        let g = [0u8, 2];
        let server_priv = BigUint::from(123_456_789u64);
        let server_pub = left_pad(&BigUint::from(2u8).modpow(&server_priv, &prime).to_bytes_be(), n);

        let out = ard_with(&g, &p, &server_pub, "mike", "geheim", &[7u8; 16], [0xAA; 128]).unwrap();
        assert_eq!(out.len(), 128 + n);

        let client_pub = BigUint::from_bytes_be(&out[128..]);
        let shared = client_pub.modpow(&server_priv, &prime);
        let key = md5::compute(left_pad(&shared.to_bytes_be(), n)).0;
        let aes = Aes128::new_from_slice(&key).unwrap();
        let mut creds = out[..128].to_vec();
        for chunk in creds.as_chunks_mut::<16>().0 {
            let mut b = aes::cipher::Block::<Aes128>::default();
            b.copy_from_slice(chunk);
            aes.decrypt_block(&mut b);
            chunk.copy_from_slice(&b);
        }
        assert_eq!(&creds[..5], b"mike\0");
        assert_eq!(&creds[64..71], b"geheim\0");

        // Schwache Werte vom Server: zu kurze Primzahl, g = 1, y = 1 bzw. p-1.
        let short = left_pad(&((BigUint::from(1u8) << 127u32) - 1u8).to_bytes_be(), 16);
        assert!(ard_with(&g, &short, &short, "m", "p", &[7u8; 16], [0; 128]).is_err());
        assert!(ard_with(&[0, 1], &p, &server_pub, "m", "p", &[7u8; 16], [0; 128]).is_err());
        let one = left_pad(&[1], n);
        let pm1 = left_pad(&(&prime - 1u8).to_bytes_be(), n);
        assert!(ard_with(&g, &p, &one, "m", "p", &[7u8; 16], [0; 128]).is_err());
        assert!(ard_with(&g, &p, &pm1, "m", "p", &[7u8; 16], [0; 128]).is_err());
    }
}
