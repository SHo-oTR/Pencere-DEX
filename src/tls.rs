use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use std::fs;
use std::net::IpAddr;
use std::path::Path;

pub struct TlsFiles {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Generates or loads existing self-signed TLS certificates for local IP
pub fn get_or_generate_tls(ip: IpAddr) -> Result<TlsFiles, Box<dyn std::error::Error + Send + Sync>> {
    let cert_file = format!("pdex_{}_cert.pem", ip);
    let key_file = format!("pdex_{}_key.pem", ip);

    if Path::new(&cert_file).exists() && Path::new(&key_file).exists() {
        if let (Ok(cert), Ok(key)) = (fs::read_to_string(&cert_file), fs::read_to_string(&key_file)) {
            return Ok(TlsFiles { cert_pem: cert, key_pem: key });
        }
    }

    let mut params = CertificateParams::default();
    params.distinguished_name.push(DnType::CommonName, "pencere-dex");
    params.subject_alt_names.push(SanType::IpAddress(ip));

    let key_pair = KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256)?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    let _ = fs::write(&cert_file, &cert_pem);
    let _ = fs::write(&key_file, &key_pem);

    Ok(TlsFiles { cert_pem, key_pem })
}
