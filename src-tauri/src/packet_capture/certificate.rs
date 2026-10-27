use sha2::{Digest, Sha256};
use x509_parser::{parse_x509_certificate, pem::parse_x509_pem};

#[derive(Debug)]
pub(super) struct Certificate {
    pub subject_hash: String,
    pub source_sha256: String,
    pub fingerprint: String,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Certificate, String> {
    if bytes.len() > 64 * 1024 {
        return Err("Reqable CA 证书过大，已取消安装".into());
    }
    let (remaining, pem) = parse_x509_pem(bytes)
        .map_err(|_| "Reqable CA 证书格式无效，请先打开 Reqable 初始化证书")?;
    if pem.label != "CERTIFICATE" || !remaining.iter().all(u8::is_ascii_whitespace) {
        return Err("Reqable CA 文件必须只包含一张公共证书".into());
    }
    let (remaining, cert) =
        parse_x509_certificate(&pem.contents).map_err(|_| "Reqable CA 证书内容无效")?;
    if !remaining.is_empty() || !cert.is_ca() {
        return Err("Reqable 证书不是有效的 CA 证书".into());
    }
    if !cert.validity().is_valid() {
        return Err("Reqable CA 证书尚未生效或已经过期，请在 Reqable 中重新生成证书".into());
    }
    // Android's legacy subject_hash_old is the little-endian first 32 bits
    // of MD5 over the DER subject name. This is a filename, not a security hash.
    let digest = md5::compute(cert.subject().as_raw());
    let hash = u32::from_le_bytes(digest.0[..4].try_into().unwrap());
    let fingerprint = Sha256::digest(&pem.contents)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    Ok(Certificate {
        subject_hash: format!("{hash:08x}"),
        source_sha256: format!("{:x}", Sha256::digest(bytes)),
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_or_oversized_certificates() {
        assert!(parse(b"not a certificate").is_err());
        assert!(parse(&vec![b'A'; 65 * 1024]).unwrap_err().contains("过大"));
    }
}
