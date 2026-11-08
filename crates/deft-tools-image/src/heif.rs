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

/// The codec of a coded picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Codec {
    Hevc,
    Av1,
}

/// The colour coding an `nclx` colour box states.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Nclx {
    pub matrix: u16,
    pub full_range: bool,
}

/// The coded tiles of one picture, ready for a decoder. HEIF stores a photo
/// as a grid of separately coded tiles; decoding them one by one and joining
/// them here avoids depending on how a decoder reads the container.
#[derive(Clone, Debug, PartialEq)]
pub struct Coded {
    pub codec: Codec,
    /// Every tile in grid order as one elementary stream (Annex B for HEVC,
    /// OBUs for AV1), each tile with its own parameter sets.
    pub stream: Vec<u8>,
    pub tiles: u32,
    pub cols: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    /// Size of the joined picture, before cropping and rotation.
    pub width: u32,
    pub height: u32,
    pub nclx: Option<Nclx>,
}

/// The primary picture: its colour, its transparency and the clean-aperture
/// crop (x, y, width, height) to apply before rotation.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub colour: Coded,
    pub alpha: Option<Coded>,
    pub crop: Option<(u32, u32, u32, u32)>,
}

struct Meta<'a> {
    file: &'a [u8],
    idat: &'a [u8],
    types: Vec<(u32, &'a [u8])>,
    locations: Vec<(u32, u8, Vec<(u64, u64)>)>,
    props: Vec<(&'a [u8], &'a [u8])>,
    assoc: Vec<(u32, Vec<usize>)>,
    /// (kind, from, to).
    refs: Vec<(&'a [u8], u32, Vec<u32>)>,
}

fn uint(data: &[u8], at: usize, size: usize) -> Option<u64> {
    let b = data.get(at..at + size)?;
    Some(b.iter().fold(0u64, |v, x| (v << 8) | *x as u64))
}

/// Item locations from an `iloc` box: (item, construction method, extents).
fn locations(iloc: &[u8]) -> Option<Vec<(u32, u8, Vec<(u64, u64)>)>> {
    let version = *iloc.first()?;
    let (offset_size, length_size) = ((iloc.get(4)? >> 4) as usize, (iloc.get(4)? & 15) as usize);
    let base_size = (iloc.get(5)? >> 4) as usize;
    let index_size = if version >= 1 {
        (iloc.get(5)? & 15) as usize
    } else {
        0
    };
    let wide = version >= 2;
    let count = uint(iloc, 6, if wide { 4 } else { 2 })?;
    let mut at = if wide { 10 } else { 8 };
    let mut out = Vec::new();
    for _ in 0..count.min(100_000) {
        let id = uint(iloc, at, if wide { 4 } else { 2 })? as u32;
        at += if wide { 4 } else { 2 };
        let method = if version >= 1 {
            at += 2;
            (uint(iloc, at - 2, 2)? & 15) as u8
        } else {
            0
        };
        at += 2; // data reference index
        let base = uint(iloc, at, base_size)?;
        at += base_size;
        let extents = uint(iloc, at, 2)?;
        at += 2;
        let mut list = Vec::new();
        for _ in 0..extents {
            at += index_size;
            let offset = uint(iloc, at, offset_size)?;
            at += offset_size;
            let length = uint(iloc, at, length_size)?;
            at += length_size;
            list.push((base.checked_add(offset)?, length));
        }
        out.push((id, method, list));
    }
    Some(out)
}

/// Item types from an `iinf` box.
fn item_types(iinf: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let start = if *iinf.first()? == 0 { 6 } else { 8 };
    let mut out = Vec::new();
    for (kind, body) in boxes(iinf.get(start..)?) {
        if kind != b"infe" || *body.first()? < 2 {
            continue;
        }
        let (id, at) = if body[0] == 2 {
            (be16(body, 4)?, 8)
        } else {
            (be32(body, 4)?, 10)
        };
        out.push((id, body.get(at..at + 4)?));
    }
    Some(out)
}

/// References from an `iref` box: (kind, from, to).
fn references(iref: &[u8]) -> Vec<(&[u8], u32, Vec<u32>)> {
    let wide = iref.first() == Some(&1);
    let size = if wide { 4 } else { 2 };
    let read = |d: &[u8], at: usize| uint(d, at, size).map(|v| v as u32);
    boxes(iref.get(4..).unwrap_or_default())
        .filter_map(|(kind, body)| {
            let from = read(body, 0)?;
            let n = uint(body, size, 2)? as usize;
            let to = (0..n)
                .map(|i| read(body, size + 2 + i * size))
                .collect::<Option<Vec<u32>>>()?;
            Some((kind, from, to))
        })
        .collect()
}

impl<'a> Meta<'a> {
    fn parse(file: &'a [u8]) -> Option<Meta<'a>> {
        let meta = find(file, b"meta")?.get(4..)?;
        let iprp = find(meta, b"iprp")?;
        Some(Meta {
            file,
            idat: find(meta, b"idat").unwrap_or_default(),
            types: item_types(find(meta, b"iinf")?)?,
            locations: locations(find(meta, b"iloc")?)?,
            props: boxes(find(iprp, b"ipco")?).collect(),
            assoc: associations(find(iprp, b"ipma")?)?,
            refs: find(meta, b"iref").map(references).unwrap_or_default(),
        })
    }

    fn kind(&self, id: u32) -> Option<&[u8]> {
        self.types.iter().find(|(i, _)| *i == id).map(|(_, t)| *t)
    }

    fn data(&self, id: u32) -> Option<Vec<u8>> {
        let (_, method, extents) = self.locations.iter().find(|(i, ..)| *i == id)?;
        let source = match method {
            0 => self.file,
            1 => self.idat,
            _ => return None,
        };
        let mut out = Vec::new();
        for &(offset, length) in extents {
            let start = usize::try_from(offset).ok()?;
            let end = if length == 0 {
                source.len()
            } else {
                start.checked_add(usize::try_from(length).ok()?)?
            };
            out.extend_from_slice(source.get(start..end)?);
        }
        Some(out)
    }

    fn prop(&self, id: u32, kind: &[u8]) -> Option<&'a [u8]> {
        let (_, list) = self.assoc.iter().find(|(i, _)| *i == id)?;
        list.iter()
            .filter_map(|n| n.checked_sub(1).and_then(|i| self.props.get(i)))
            .find(|(k, _)| *k == kind)
            .map(|(_, b)| *b)
    }

    fn size(&self, id: u32) -> Option<(u32, u32)> {
        let ispe = self.prop(id, b"ispe")?;
        Some((be32(ispe, 4)?, be32(ispe, 8)?))
    }

    fn nclx(&self, id: u32) -> Option<Nclx> {
        let (_, list) = self.assoc.iter().find(|(i, _)| *i == id)?;
        list.iter()
            .filter_map(|n| n.checked_sub(1).and_then(|i| self.props.get(i)))
            .filter(|(k, b)| *k == b"colr" && b.starts_with(b"nclx"))
            .find_map(|(_, b)| {
                Some(Nclx {
                    matrix: be16(b, 8)? as u16,
                    full_range: b.get(10)? & 0x80 != 0,
                })
            })
    }

    fn targets(&self, kind: &[u8], from: u32) -> Option<&[u32]> {
        self.refs
            .iter()
            .find(|(k, f, _)| *k == kind && *f == from)
            .map(|(_, _, to)| &to[..])
    }

    /// One coded tile in a decoder's own format, with its parameter sets.
    fn tile(&self, id: u32, out: &mut Vec<u8>) -> Option<Codec> {
        let data = self.data(id)?;
        match self.kind(id)? {
            b"hvc1" => {
                let config = self.prop(id, b"hvcC")?;
                let length_size = (config.get(21)? & 3) as usize + 1;
                let mut at = 23;
                for _ in 0..*config.get(22)? {
                    let n = be16(config, at + 1)?;
                    at += 3;
                    for _ in 0..n {
                        let len = be16(config, at)? as usize;
                        out.extend_from_slice(&[0, 0, 0, 1]);
                        out.extend_from_slice(config.get(at + 2..at + 2 + len)?);
                        at += 2 + len;
                    }
                }
                let mut at = 0;
                while at < data.len() {
                    let len = uint(&data, at, length_size)? as usize;
                    at += length_size;
                    out.extend_from_slice(&[0, 0, 0, 1]);
                    out.extend_from_slice(data.get(at..at + len)?);
                    at += len;
                }
                Some(Codec::Hevc)
            }
            b"av01" => {
                // A temporal delimiter starts each tile as a frame of its own.
                out.extend_from_slice(&[0x12, 0]);
                out.extend_from_slice(self.prop(id, b"av1C")?.get(4..)?);
                out.extend_from_slice(&data);
                Some(Codec::Av1)
            }
            _ => None,
        }
    }

    /// An image item, a grid of them, or an identity item pointing at one.
    fn coded(&self, id: u32) -> Option<Coded> {
        let (width, height) = self.size(id)?;
        let nclx = self.nclx(id);
        match self.kind(id)? {
            b"grid" => {
                let grid = self.data(id)?;
                let rows = *grid.get(2)? as u32 + 1;
                let cols = *grid.get(3)? as u32 + 1;
                let (width, height) = if grid.get(1)? & 1 == 1 {
                    (be32(&grid, 4)?, be32(&grid, 8)?)
                } else {
                    (be16(&grid, 4)?, be16(&grid, 6)?)
                };
                let tiles = self.targets(b"dimg", id)?;
                if tiles.len() as u32 != rows * cols {
                    return None;
                }
                let (tile_width, tile_height) = self.size(*tiles.first()?)?;
                let mut stream = Vec::new();
                let mut codec = None;
                for &tile in tiles {
                    if self.size(tile)? != (tile_width, tile_height) {
                        return None;
                    }
                    codec = Some(self.tile(tile, &mut stream)?);
                }
                (tile_width * cols >= width && tile_height * rows >= height).then_some(())?;
                Some(Coded {
                    codec: codec?,
                    stream,
                    tiles: rows * cols,
                    cols,
                    tile_width,
                    tile_height,
                    width,
                    height,
                    nclx: nclx.or_else(|| self.nclx(tiles[0])),
                })
            }
            b"iden" => self.coded(*self.targets(b"dimg", id)?.first()?),
            _ => {
                let mut stream = Vec::new();
                let codec = self.tile(id, &mut stream)?;
                Some(Coded {
                    codec,
                    stream,
                    tiles: 1,
                    cols: 1,
                    tile_width: width,
                    tile_height: height,
                    width,
                    height,
                    nclx,
                })
            }
        }
    }

    /// The clean aperture as a crop of a `width` x `height` picture.
    fn crop(&self, id: u32, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let clap = self.prop(id, b"clap")?;
        let signed = |at: usize| be32(clap, at).map(|v| v as i32 as f64);
        let ratio = |at: usize| Some(signed(at)? / signed(at + 4)?.max(1.0));
        let (w, h) = (ratio(0)?.round(), ratio(8)?.round());
        let x = (ratio(16)? + (width as f64 - w) / 2.0).round();
        let y = (ratio(24)? + (height as f64 - h) / 2.0).round();
        let fits = w >= 1.0 && h >= 1.0 && x >= 0.0 && y >= 0.0;
        (fits && x + w <= width as f64 && y + h <= height as f64)
            .then_some((x as u32, y as u32, w as u32, h as u32))
    }
}

/// The coded primary picture of a HEIC or AVIF file. None for a layout this
/// reader does not know, such as a JPEG item or an overlay.
pub fn picture(bytes: &[u8]) -> Option<Picture> {
    let meta = Meta::parse(bytes)?;
    let pitm = find(find(bytes, b"meta")?.get(4..)?, b"pitm")?;
    let primary = if *pitm.first()? == 0 {
        be16(pitm, 4)?
    } else {
        be32(pitm, 4)?
    };
    let colour = meta.coded(primary)?;
    let alpha = meta
        .refs
        .iter()
        .filter(|(kind, _, to)| *kind == b"auxl" && to.contains(&primary))
        .map(|(_, from, _)| *from)
        .find(|&id| {
            meta.prop(id, b"auxC")
                .is_some_and(|c| c.windows(5).any(|w| w == b"alpha"))
        })
        .and_then(|id| meta.coded(id))
        .filter(|a| (a.width, a.height) == (colour.width, colour.height));
    let crop = meta.crop(primary, colour.width, colour.height);
    Some(Picture {
        colour,
        alpha,
        crop,
    })
}

/// Join decoded tiles, `channels` bytes per pixel, in grid order, into the
/// picture they make, cutting off what lies past its edges.
pub fn join(tiles: &[u8], coded: &Coded, channels: usize) -> Option<Vec<u8>> {
    let (tw, th) = (coded.tile_width as usize, coded.tile_height as usize);
    let (w, h) = (coded.width as usize, coded.height as usize);
    let tile_bytes = tw * th * channels;
    if tiles.len() != tile_bytes * coded.tiles as usize {
        return None;
    }
    let mut out = vec![0u8; w * h * channels];
    for (n, tile) in tiles.chunks_exact(tile_bytes).enumerate() {
        let (x0, y0) = (
            (n % coded.cols as usize) * tw,
            (n / coded.cols as usize) * th,
        );
        if x0 >= w || y0 >= h {
            continue;
        }
        let run = tw.min(w - x0) * channels;
        for row in 0..th.min(h - y0) {
            let from = row * tw * channels;
            let to = ((y0 + row) * w + x0) * channels;
            out[to..to + run].copy_from_slice(&tile[from..from + run]);
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "heif_tests.rs"]
mod tests;
