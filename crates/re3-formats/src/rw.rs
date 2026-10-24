//! RenderWare binary stream: every block is `type, size, version` followed by `size` bytes.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    /// A block of a different type than the format requires.
    Unexpected { expected: u32, found: u32 },
    Bad(&'static str),
    /// A native texture for a platform other than PC (the value is the platform tag).
    Platform(u32),
}

pub const STRUCT: u32 = 0x01;
pub const STRING: u32 = 0x02;
pub const EXTENSION: u32 = 0x03;
pub const TEXTURE: u32 = 0x06;
pub const MATERIAL: u32 = 0x07;
pub const MATERIAL_LIST: u32 = 0x08;
pub const FRAME_LIST: u32 = 0x0E;
pub const GEOMETRY: u32 = 0x0F;
pub const CLUMP: u32 = 0x10;
pub const LIGHT: u32 = 0x12;
pub const ATOMIC: u32 = 0x14;
pub const TEXTURE_NATIVE: u32 = 0x15;
pub const TEXTURE_DICTIONARY: u32 = 0x16;
pub const GEOMETRY_LIST: u32 = 0x1A;
pub const BIN_MESH: u32 = 0x50E;
pub const SKIN: u32 = 0x116;
pub const HANIM: u32 = 0x11E;
pub const NODE_NAME: u32 = 0x253F2FE;

#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub ty: u32,
    /// Raw library stamp from the header; see [`unpack_version`].
    pub version: u32,
    pub body: &'a [u8],
}

/// RenderWare library stamp -> `0xMMmmrrbb` style version (e.g. 0x33002 for Vice City).
pub fn unpack_version(id: u32) -> u32 {
    if id & 0xFFFF_0000 != 0 {
        (((id >> 14) & 0x3FF00) + 0x30000) | ((id >> 16) & 0x3F)
    } else {
        id << 8
    }
}

/// Reads one block from the front of `data`; returns it and the bytes after it.
pub fn chunk(data: &[u8]) -> Result<(Chunk<'_>, &[u8]), Error> {
    let h = data.get(..12).ok_or(Error::Truncated)?;
    let u = |i: usize| u32::from_le_bytes(h[i..i + 4].try_into().unwrap());
    let size = u(4) as usize;
    let body = data.get(12..12 + size).ok_or(Error::Truncated)?;
    Ok((Chunk { ty: u(0), version: u(8), body }, &data[12 + size..]))
}

/// Like [`chunk`] but insists on the block type.
pub fn expect(data: &[u8], ty: u32) -> Result<(Chunk<'_>, &[u8]), Error> {
    let (c, rest) = chunk(data)?;
    if c.ty != ty {
        return Err(Error::Unexpected { expected: ty, found: c.ty });
    }
    Ok((c, rest))
}

/// All blocks laid back to back in `body`.
pub fn children(mut body: &[u8]) -> Result<Vec<Chunk<'_>>, Error> {
    let mut out = vec![];
    while !body.is_empty() {
        let (c, rest) = chunk(body)?;
        out.push(c);
        body = rest;
    }
    Ok(out)
}

/// Extension block contents as (type, bytes) pairs.
pub fn extension(c: Chunk) -> Result<Vec<(u32, Vec<u8>)>, Error> {
    Ok(children(c.body)?.into_iter().map(|c| (c.ty, c.body.to_vec())).collect())
}

/// Little-endian cursor with bounds checks.
pub struct Cur<'a> {
    b: &'a [u8],
    pub at: usize,
}

impl<'a> Cur<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.at..self.at.checked_add(n).ok_or(Error::Truncated)?).ok_or(Error::Truncated)?;
        self.at += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }
    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn f32s<const N: usize>(&mut self) -> Result<[f32; N], Error> {
        let mut a = [0.0; N];
        for x in &mut a {
            *x = self.f32()?;
        }
        Ok(a)
    }
    /// Fixed-size NUL-padded string.
    pub fn name(&mut self, n: usize) -> Result<String, Error> {
        let s = self.take(n)?;
        Ok(String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(n)]).into_owned())
    }
    pub fn left(&self) -> usize {
        self.b.len() - self.at
    }
}

/// A `String` block's text (NUL-terminated, padded to 4 bytes).
pub fn string(c: Chunk) -> String {
    String::from_utf8_lossy(&c.body[..c.body.iter().position(|&b| b == 0).unwrap_or(c.body.len())]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn block(ty: u32, version: u32, body: &[u8]) -> Vec<u8> {
        let mut v = [ty.to_le_bytes(), (body.len() as u32).to_le_bytes(), version.to_le_bytes()].concat();
        v.extend(body);
        v
    }

    #[test]
    fn chunks_and_versions() {
        let data = [block(1, 0x0C02FFFF, &[1, 2, 3, 4]), block(2, 0x0C02FFFF, b"hi\0\0")].concat();
        let c = children(&data).unwrap();
        assert_eq!((c[0].ty, c[0].body), (1, &[1, 2, 3, 4][..]));
        assert_eq!(string(c[1]), "hi");
        assert_eq!(chunk(&data[..14]).unwrap_err(), Error::Truncated);
        assert_eq!(expect(&data, 2).unwrap_err(), Error::Unexpected { expected: 2, found: 1 });
        assert_eq!(unpack_version(0x0C02FFFF), 0x33002);
        assert_eq!(unpack_version(0x1803FFFF), 0x36003); // GTA San Andreas
    }
}
