//! Texture dictionaries (`.txd`) as stored by the PC versions (D3D8 native textures).
use crate::rw::{self, Cur, Error};

#[derive(Debug, PartialEq)]
pub struct Texture {
    pub name: String,
    pub mask: String,
    pub filter_addressing: u32,
    pub raster_format: u32,
    /// D3D8: non-zero if the texture has alpha. D3D9: the four-character format code.
    pub d3d_format: u32,
    pub width: u16,
    pub height: u16,
    pub depth: u8,
    pub raster_type: u8,
    /// 0 = uncompressed, otherwise the DXT variant number (1, 3, 5).
    pub compression: u8,
    pub palette: Option<Vec<[u8; 4]>>,
    /// Raw pixel data per mip level, largest first.
    pub mips: Vec<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub struct Txd {
    pub textures: Vec<Texture>,
}

const PAL8: u32 = 0x2000;
const PAL4: u32 = 0x4000;

fn native(c: rw::Chunk) -> Result<Texture, Error> {
    let (st, _) = rw::expect(c.body, rw::STRUCT)?;
    let mut r = Cur::new(st.body);
    let platform = r.u32()?;
    if platform != 8 && platform != 9 {
        return Err(Error::Platform(platform));
    }
    let filter_addressing = r.u32()?;
    let (name, mask) = (r.name(32)?, r.name(32)?);
    let raster_format = r.u32()?;
    let d3d_format = r.u32()?;
    let (width, height) = (r.u16()?, r.u16()?);
    let (depth, levels, raster_type, compression) = (r.u8()?, r.u8()?, r.u8()?, r.u8()?);
    let palette = if raster_format & PAL8 != 0 || raster_format & PAL4 != 0 {
        let n = if raster_format & PAL8 != 0 { 256 } else { 32 };
        Some((0..n).map(|_| r.take(4).map(|p| [p[0], p[1], p[2], p[3]])).collect::<Result<Vec<_>, _>>()?)
    } else {
        None
    };
    let mut mips = vec![];
    for _ in 0..levels {
        let n = r.u32()? as usize;
        mips.push(r.take(n)?.to_vec());
    }
    Ok(Texture { name, mask, filter_addressing, raster_format, d3d_format, width, height, depth, raster_type, compression, palette, mips })
}

pub fn parse_txd(data: &[u8]) -> Result<Txd, Error> {
    let (dict, _) = rw::expect(data, rw::TEXTURE_DICTIONARY)?;
    let parts = rw::children(dict.body)?;
    let st = parts.first().filter(|c| c.ty == rw::STRUCT).ok_or(Error::Bad("missing dictionary struct"))?;
    let count = u16::from_le_bytes(st.body.get(..2).ok_or(Error::Truncated)?.try_into().unwrap()) as usize;
    let textures: Vec<Texture> = parts[1..].iter().filter(|c| c.ty == rw::TEXTURE_NATIVE).map(|c| native(*c)).collect::<Result<_, _>>()?;
    if textures.len() != count {
        return Err(Error::Bad("texture count mismatch"));
    }
    Ok(Txd { textures })
}

impl Texture {
    /// Bytes the top mip level should occupy, from the format; None if the format is unknown.
    pub fn expected_top_mip(&self) -> Option<usize> {
        let (w, h) = (self.width as usize, self.height as usize);
        if self.compression != 0 {
            let block = if self.compression == 1 { 8 } else { 16 };
            return Some(w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * block);
        }
        if self.palette.is_some() {
            return Some(if self.raster_format & PAL8 != 0 { w * h } else { w * h / 2 });
        }
        Some(w * h * (self.depth as usize / 8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(ty: u32, body: &[u8]) -> Vec<u8> {
        [ty.to_le_bytes(), (body.len() as u32).to_le_bytes(), 0x0C02FFFFu32.to_le_bytes()].concat().into_iter().chain(body.iter().copied()).collect()
    }

    fn texture_2x2() -> Vec<u8> {
        let mut st = [8u32.to_le_bytes(), 0x1106u32.to_le_bytes()].concat();
        for n in ["wall", "wall_a"] {
            let mut name = n.as_bytes().to_vec();
            name.resize(32, 0);
            st.extend(name);
        }
        st.extend(0x500u32.to_le_bytes()); // 8888
        st.extend(1u32.to_le_bytes());
        st.extend([2, 0, 2, 0, 32, 1, 4, 0]);
        st.extend(16u32.to_le_bytes());
        st.extend([0xAB; 16]);
        block(rw::TEXTURE_NATIVE, &[block(rw::STRUCT, &st), block(rw::EXTENSION, &[])].concat())
    }

    #[test]
    fn one_texture() {
        let dict = block(rw::TEXTURE_DICTIONARY, &[block(rw::STRUCT, &[1, 0, 0, 0]), texture_2x2()].concat());
        let t = parse_txd(&dict).unwrap();
        let x = &t.textures[0];
        assert_eq!((x.name.as_str(), x.mask.as_str(), x.width, x.depth, x.mips.len()), ("wall", "wall_a", 2, 32, 1));
        assert_eq!(x.expected_top_mip(), Some(x.mips[0].len()));
    }

    #[test]
    fn count_mismatch_and_other_platforms() {
        let dict = block(rw::TEXTURE_DICTIONARY, &[block(rw::STRUCT, &[2, 0, 0, 0]), texture_2x2()].concat());
        assert_eq!(parse_txd(&dict).unwrap_err(), Error::Bad("texture count mismatch"));
        let ps2 = block(rw::TEXTURE_NATIVE, &block(rw::STRUCT, &0x325350u32.to_le_bytes()));
        let dict = block(rw::TEXTURE_DICTIONARY, &[block(rw::STRUCT, &[1, 0, 0, 0]), ps2].concat());
        assert_eq!(parse_txd(&dict).unwrap_err(), Error::Platform(0x325350));
    }
}
