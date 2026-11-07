//! Editing the chunks of a WebP file without touching its image data.

/// The chunks of a WebP file, as (type, body).
fn chunks(bytes: &[u8]) -> Option<Vec<(&[u8], &[u8])>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    let size = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    let end = size.saturating_add(8).min(bytes.len());
    let mut out = Vec::new();
    let mut i = 12;
    while i + 8 <= end {
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        out.push((&bytes[i..i + 4], bytes.get(i + 8..i + 8 + len)?));
        i += 8 + len + (len & 1);
    }
    Some(out)
}

fn file(chunks: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    for (kind, data) in chunks {
        body.extend_from_slice(kind);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

const ICC_FLAG: u8 = 0x20;
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

/// Pull out the raw EXIF chunk.
pub fn exif_chunk(bytes: &[u8]) -> Option<&[u8]> {
    let chunks = chunks(bytes)?;
    chunks.iter().find(|(k, _)| *k == b"EXIF").map(|(_, b)| *b)
}

/// The orientation in a WebP's EXIF chunk. Some writers put the JPEG
/// segment's `Exif\0\0` before the TIFF data, which other readers miss.
fn orientation(all: &[(&[u8], &[u8])]) -> Option<u8> {
    let exif = all.iter().find(|(k, _)| *k == b"EXIF")?.1;
    crate::tiff_orientation(exif.strip_prefix(b"Exif\0\0").unwrap_or(exif))
}

/// Keep only the chunks that make the picture: the image data, alpha,
/// animation, the extended header and, unless dropped, the profile. Every
/// other chunk goes, whatever it is called, since metadata chunks are not
/// always spelled `EXIF` or `XMP `. Orientation stays, in a chunk of its own,
/// so the picture keeps displaying upright.
pub fn strip(bytes: &[u8], keep_icc: bool) -> Option<Vec<u8>> {
    let all = chunks(bytes)?;
    let orientation = orientation(&all).unwrap_or_else(|| crate::exif_orientation(bytes));
    let tiff = crate::orientation_tiff(orientation);
    let mut header = all
        .iter()
        .find(|(k, _)| *k == b"VP8X")
        .map(|c| c.1.to_vec());
    if let Some(flags) = header.as_mut().and_then(|h| h.first_mut()) {
        *flags &= !(XMP_FLAG | EXIF_FLAG | if keep_icc { 0 } else { ICC_FLAG });
        if orientation != 1 {
            *flags |= EXIF_FLAG;
        }
    }
    let mut kept: Vec<(&[u8], &[u8])> = Vec::new();
    for (kind, body) in &all {
        match *kind {
            b"VP8 " | b"VP8L" | b"ALPH" | b"ANIM" | b"ANMF" => kept.push((kind, body)),
            b"ICCP" if keep_icc => kept.push((kind, body)),
            b"VP8X" => kept.push((kind, header.as_deref()?)),
            _ => {}
        }
    }
    if orientation != 1 && header.is_some() {
        kept.push((b"EXIF", &tiff));
    }
    Some(file(&kept))
}

/// Add a colour profile to a WebP file of the given size.
pub fn with_profile(bytes: &[u8], icc: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let all = chunks(bytes)?;
    let mut header = match all.iter().find(|(k, _)| *k == b"VP8X") {
        Some((_, b)) => b.to_vec(),
        None => {
            let mut h = vec![0; 4];
            h.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
            h.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
            h
        }
    };
    *header.first_mut()? |= ICC_FLAG;
    let mut kept: Vec<(&[u8], &[u8])> = vec![(b"VP8X", &header), (b"ICCP", icc)];
    kept.extend(
        all.iter()
            .copied()
            .filter(|(k, _)| !matches!(*k, b"VP8X" | b"ICCP")),
    );
    Some(file(&kept))
}

/// Whether the picture data is lossy.
pub fn is_lossy(bytes: &[u8]) -> bool {
    chunks(bytes).is_some_and(|c| c.iter().any(|(k, _)| *k == b"VP8 "))
}
