//! What a HEIC or AVIF file says about its main picture. ffmpeg decodes the
//! pixels but leaves rotation and the colour profile to the caller.

/// Facts about the primary item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heif {
    /// Size as stored, before rotation.
    pub width: u32,
    pub height: u32,
    /// The rotation and mirroring as an EXIF orientation value, 1 to 8.
    pub orientation: u8,
    pub icc: Option<Vec<u8>>,
    /// The picture has a separate transparency plane.
    pub alpha: bool,
}

/// The boxes in `data`, as (type, body).
fn boxes(mut data: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        let size = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as u64;
        let kind = data.get(4..8)?;
        let (start, size) = match size {
            0 => (8, data.len() as u64),
            1 => (16, u64::from_be_bytes(data.get(8..16)?.try_into().ok()?)),
            n => (8, n),
        };
        let end = usize::try_from(size).ok()?;
        let body = data.get(start..end)?;
        data = &data[end..];
        Some((kind, body))
    })
}

fn find<'a>(data: &'a [u8], kind: &[u8]) -> Option<&'a [u8]> {
    boxes(data).find(|(k, _)| *k == kind).map(|(_, b)| b)
}

fn be16(data: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32)
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The property numbers (from 1) of each item, from an `ipma` box.
fn associations(ipma: &[u8]) -> Option<Vec<(u32, Vec<usize>)>> {
    let wide_id = *ipma.first()? >= 1;
    let wide_index = ipma.get(3)? & 1 == 1;
    let count = be32(ipma, 4)?;
    let mut at = 8;
    let mut out = Vec::new();
    for _ in 0..count {
        let id = if wide_id {
            be32(ipma, at)?
        } else {
            be16(ipma, at)?
        };
        at += if wide_id { 4 } else { 2 };
        let n = *ipma.get(at)?;
        at += 1;
        let mut props = Vec::new();
        for _ in 0..n {
            if wide_index {
                props.push((be16(ipma, at)? & 0x7FFF) as usize);
                at += 2;
            } else {
                props.push((ipma.get(at)? & 0x7F) as usize);
                at += 1;
            }
        }
        out.push((id, props));
    }
    Some(out)
}

/// A quarter-turn count and a mirror axis as an EXIF orientation value. Axis 0
/// swaps top and bottom, axis 1 swaps left and right; the turn comes first.
fn exif_orientation(turns: u8, axis: Option<u8>) -> u8 {
    match (turns & 3, axis) {
        (0, None) => 1,
        (1, None) => 8,
        (2, None) => 3,
        (3, None) => 6,
        (0, Some(0)) | (2, Some(1)) => 4,
        (0, Some(_)) | (2, Some(_)) => 2,
        (1, Some(0)) | (3, Some(1)) => 5,
        _ => 7,
    }
}

/// Read the primary item's properties. None when this is not a HEIF file.
pub fn read(bytes: &[u8]) -> Option<Heif> {
    let meta = find(bytes, b"meta")?.get(4..)?;
    let pitm = find(meta, b"pitm")?;
    let primary = if *pitm.first()? == 0 {
        be16(pitm, 4)?
    } else {
        be32(pitm, 4)?
    };
    let iprp = find(meta, b"iprp")?;
    let props: Vec<(&[u8], &[u8])> = boxes(find(iprp, b"ipco")?).collect();
    let items = associations(find(iprp, b"ipma")?)?;
    let mut out = Heif {
        orientation: 1,
        ..Default::default()
    };
    let (mut turns, mut axis) = (0, None);
    for n in &items.iter().find(|(id, _)| *id == primary)?.1 {
        let Some((kind, body)) = n.checked_sub(1).and_then(|i| props.get(i)) else {
            continue;
        };
        match *kind {
            b"ispe" => (out.width, out.height) = (be32(body, 4)?, be32(body, 8)?),
            b"colr" if matches!(body.get(..4)?, b"prof" | b"rICC") => {
                out.icc = Some(body[4..].to_vec());
            }
            b"irot" => {
                turns = body.first()? & 3;
                // A mirror listed first acts on the other axis once turned.
                if turns % 2 == 1 {
                    axis = axis.map(|a: u8| 1 - a);
                }
            }
            b"imir" => axis = Some(body.first()? & 1),
            _ => {}
        }
    }
    out.orientation = exif_orientation(turns, axis);
    out.alpha = props
        .iter()
        .any(|(kind, body)| *kind == b"auxC" && body.windows(5).any(|w| w == b"alpha"));
    Some(out)
}

#[cfg(test)]
#[path = "heif_tests.rs"]
mod tests;
