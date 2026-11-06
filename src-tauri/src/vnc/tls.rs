//! TLS für VeNCrypt (X509-Untertypen). VNC-Server nutzen fast immer selbstsignierte
//! Zertifikate, darum prüfen wir per Pinning: SHA256 des Zertifikats, beim ersten Mal bestätigt.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Fehler-Präfixe fürs Frontend: `CERT_UNKNOWN:<SHA256>` bzw. `CERT_CHANGED:<SHA256>`.
pub const CERT_UNKNOWN: &str = "CERT_UNKNOWN:";
pub const CERT_CHANGED: &str = "CERT_CHANGED:";

pub fn fingerprint(der: &[u8]) -> String {
    let h = Sha256::digest(der);
    let hex: Vec<String> = h.iter().map(|b| format!("{b:02X}")).collect();
    format!("SHA256:{}", hex.join(":"))
}

#[derive(Debug)]
pub struct Pinned {
    /// Erwarteter Fingerabdruck (gespeichert oder gerade vom Benutzer bestätigt).
    pub expected: Option<String>,
    /// Was der Server gezeigt hat, für die Fehlermeldung.
    pub seen: Mutex<Option<String>>,
    accept_any: bool,
    provider: Arc<CryptoProvider>,
}

impl Pinned {
    pub fn new(expected: Option<String>) -> Arc<Self> {
        Arc::new(Self { expected, seen: Mutex::new(None), accept_any: false, provider: Arc::new(rustls::crypto::ring::default_provider()) })
    }

    /// Jedes Zertifikat annehmen (Einstellung «Zertifikat nicht prüfen»).
    pub fn accept_any() -> Arc<Self> {
        Arc::new(Self { expected: None, seen: Mutex::new(None), accept_any: true, provider: Arc::new(rustls::crypto::ring::default_provider()) })
    }

    pub fn config(self: &Arc<Self>) -> Result<ClientConfig, String> {
        Ok(ClientConfig::builder_with_provider(self.provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(self.clone())
            .with_no_client_auth())
    }

    /// Verständliche Meldung nach einem fehlgeschlagenen TLS-Handshake.
    pub fn explain(&self, err: std::io::Error) -> String {
        match (self.seen.lock().unwrap().clone(), &self.expected) {
            (Some(fp), None) => format!("{CERT_UNKNOWN}{fp}"),
            (Some(fp), Some(want)) if &fp != want => format!("{CERT_CHANGED}{fp}"),
            _ => format!("TLS-Verbindung fehlgeschlagen: {err}"),
        }
    }
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity);
        *self.seen.lock().unwrap() = Some(fp.clone());
        if self.accept_any || self.expected.as_deref() == Some(fp.as_str()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("Zertifikat nicht bestätigt".into()))
        }
    }

    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

pub fn server_name(host: &str) -> Result<ServerName<'static>, String> {
    ServerName::try_from(host.to_string()).map_err(|_| format!("Ungültiger Hostname für TLS: {host}"))
}
