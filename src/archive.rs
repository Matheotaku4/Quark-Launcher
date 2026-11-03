//! Extraction minimale d'un membre tar, sans suivre les liens ni écrire sur
//! le disque. Les archives de release ne contiennent qu'un binaire attendu :
//! tout autre chemin, type ou checksum invalide est refusé.

/// Extrait le contenu du fichier régulier `expected` (chemin exact, sans
/// `./`) d'une archive tar déjà décompressée.
pub(crate) fn extract_member(bytes: &[u8], expected: &str) -> Result<Vec<u8>, String> {
    let mut offset = 0usize;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name_end = header[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let size_text = std::str::from_utf8(&header[124..136])
            .map_err(|_| "archive tar invalide".to_string())?
            .trim_matches(|ch| ch == '\0' || ch == ' ');
        let size =
            usize::from_str_radix(size_text, 8).map_err(|_| "taille tar invalide".to_string())?;
        let body = offset + 512;
        let end = body
            .checked_add(size)
            .ok_or_else(|| "archive tar invalide".to_string())?;
        if end > bytes.len() {
            return Err("archive tar tronquée".to_string());
        }
        let entry_type = header[156];
        let stored_checksum = std::str::from_utf8(&header[148..156])
            .ok()
            .and_then(|text| {
                usize::from_str_radix(text.trim_matches(|ch| ch == '\0' || ch == ' '), 8).ok()
            })
            .ok_or_else(|| "checksum tar invalide".to_string())?;
        let actual_checksum: usize = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    b' ' as usize
                } else {
                    *byte as usize
                }
            })
            .sum();
        if stored_checksum != actual_checksum {
            return Err("checksum tar invalide".to_string());
        }
        let prefix_end = header[345..500]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(155);
        let mut full_name = header[345..345 + prefix_end].to_vec();
        if !full_name.is_empty() {
            full_name.push(b'/');
        }
        full_name.extend_from_slice(&header[..name_end]);
        let full_name =
            std::str::from_utf8(&full_name).map_err(|_| "archive tar invalide".to_string())?;
        if full_name.starts_with('/') || full_name.split('/').any(|part| part == "..") {
            return Err("chemin dangereux dans l'archive".to_string());
        }
        if (entry_type == 0 || entry_type == b'0') && full_name == expected {
            return Ok(bytes[body..end].to_vec());
        }
        offset = body + size.div_ceil(512) * 512;
    }
    Err(format!("{expected} absent de l'archive"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_with(name: &str, body: &[u8]) -> Vec<u8> {
        let mut archive = vec![0u8; 512 + body.len().div_ceil(512) * 512];
        let header = &mut archive[..512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].fill(b'0');
        header[106] = b'7';
        header[124..136].fill(0);
        header[134] = b'4';
        header[156] = b'0';
        header[148..156].fill(b' ');
        let checksum: usize = header.iter().map(|byte| *byte as usize).sum();
        let encoded = format!("{checksum:06o}");
        header[148..154].copy_from_slice(encoded.as_bytes());
        header[154] = 0;
        header[155] = b' ';
        archive[512..512 + body.len()].copy_from_slice(body);
        archive
    }

    #[test]
    fn extrait_uniquement_le_membre_attendu() {
        assert_eq!(
            extract_member(&archive_with("quark/quark", b"ELF!"), "quark/quark").unwrap(),
            b"ELF!"
        );
        assert!(extract_member(&archive_with("quark/README.md", b"x"), "quark/quark").is_err());
    }

    #[test]
    fn refuse_un_chemin_dangereux_ou_un_checksum_invalide() {
        assert!(extract_member(&archive_with("../quark", b"x"), "../quark").is_err());
        let mut bad = archive_with("quark/quark", b"x");
        bad[148] ^= 1;
        assert!(extract_member(&bad, "quark/quark").is_err());
    }
}
