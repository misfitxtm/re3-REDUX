//! Animation files (`.ifp`, version 1): blocks of named animations, each a set of per-bone
//! keyframe tracks. Values are as stored; `AnimManager` turns them into runtime form.
use crate::rw::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// `KR00`: rotation + time
    Rotation,
    /// `KRT0`: rotation, translation, time
    RotationTranslation,
    /// `KRTS`: rotation, translation, scale, time
    RotationTranslationScale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// x, y, z, w as stored (the game uses the conjugate)
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub scale: [f32; 3],
    /// Absolute time in seconds.
    pub time: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub name: String,
    /// Skeleton bone id (present when the animation chunk is 44 bytes).
    pub bone_tag: Option<i32>,
    pub kind: TrackKind,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    pub sequences: Vec<Sequence>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub animations: Vec<Animation>,
}

struct Rd<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.at..self.at.checked_add(n).ok_or(Error::Truncated)?).ok_or(Error::Truncated)?;
        self.at += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }
    /// `{ident, size}` header.
    fn header(&mut self) -> Result<([u8; 4], u32), Error> {
        let h = self.take(8)?;
        Ok((h[..4].try_into().unwrap(), u32::from_le_bytes(h[4..].try_into().unwrap())))
    }
    /// A chunk body, read rounded up to a multiple of 4 as the original does.
    fn body(&mut self, size: u32) -> Result<&'a [u8], Error> {
        self.take((size as usize).div_ceil(4) * 4)
    }
}

fn cstr(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned()
}

fn expect(r: &mut Rd, ident: &[u8; 4]) -> Result<u32, Error> {
    let (id, size) = r.header()?;
    if &id != ident {
        return Err(Error::Bad("unexpected IFP chunk"));
    }
    Ok(size)
}

fn sequence(r: &mut Rd) -> Result<Sequence, Error> {
    expect(r, b"CPAN")?;
    let size = expect(r, b"ANIM")?;
    let a = r.body(size)?;
    if a.len() < 32 {
        return Err(Error::Truncated);
    }
    let name = cstr(&a[..28]);
    let num_frames = i32::from_le_bytes(a[28..32].try_into().unwrap());
    let bone_tag = (size == 44).then(|| i32::from_le_bytes(a[40..44].try_into().unwrap()));
    let num_frames = usize::try_from(num_frames).map_err(|_| Error::Bad("negative frame count"))?;
    if num_frames == 0 {
        // no track chunk follows; the original skips it entirely
        return Ok(Sequence { name, bone_tag, kind: TrackKind::Rotation, keyframes: vec![] });
    }
    let (kind_id, _) = r.header()?;
    let kind = match &kind_id {
        b"KRTS" => TrackKind::RotationTranslationScale,
        b"KRT0" => TrackKind::RotationTranslation,
        b"KR00" => TrackKind::Rotation,
        _ => return Err(Error::Bad("unknown keyframe type")),
    };
    let mut keyframes = Vec::with_capacity(num_frames.min(4096));
    for _ in 0..num_frames {
        let rotation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        let (mut translation, mut scale) = ([0.0; 3], [1.0; 3]);
        if kind != TrackKind::Rotation {
            translation = [r.f32()?, r.f32()?, r.f32()?];
        }
        if kind == TrackKind::RotationTranslationScale {
            scale = [r.f32()?, r.f32()?, r.f32()?];
        }
        keyframes.push(Keyframe { rotation, translation, scale, time: r.f32()? });
    }
    Ok(Sequence { name, bone_tag, kind, keyframes })
}

fn animation(r: &mut Rd) -> Result<Animation, Error> {
    let size = expect(r, b"NAME")?;
    let name = cstr(r.body(size)?);
    expect(r, b"DGAN")?;
    let size = expect(r, b"INFO")?;
    let info = r.body(size)?;
    let n = u32::from_le_bytes(info.get(..4).ok_or(Error::Truncated)?.try_into().unwrap()) as usize;
    let sequences = (0..n).map(|_| sequence(r)).collect::<Result<_, _>>()?;
    Ok(Animation { name, sequences })
}

/// Parses an `.ifp` file: either a single `ANPK` block or an `ANLF` list of them.
pub fn parse_ifp(data: &[u8]) -> Result<Vec<Block>, Error> {
    let mut r = Rd { b: data, at: 0 };
    let (id, size) = r.header()?;
    let count = if &id == b"ANLF" {
        let b = r.body(size)?;
        u32::from_le_bytes(b.get(..4).ok_or(Error::Truncated)?.try_into().unwrap()) as usize
    } else if &id == b"ANPK" {
        r.at = 0;
        1
    } else {
        return Err(Error::Bad("not an IFP file"));
    };
    let mut blocks = vec![];
    for _ in 0..count {
        expect(&mut r, b"ANPK")?;
        let size = expect(&mut r, b"INFO")?;
        let info = r.body(size)?;
        if info.len() < 8 {
            return Err(Error::Truncated);
        }
        let n = u32::from_le_bytes(info[..4].try_into().unwrap()) as usize;
        let name = cstr(&info[4..info.len().min(28)]);
        let animations = (0..n).map(|_| animation(&mut r)).collect::<Result<_, _>>()?;
        blocks.push(Block { name, animations });
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [id.to_vec(), (body.len() as u32).to_le_bytes().to_vec(), body.to_vec()].concat()
    }

    fn name(s: &str, n: usize) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.resize(n, 0);
        v
    }

    fn f(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn sample() -> Vec<u8> {
        let anim = [name("Root", 28), 2i32.to_le_bytes().to_vec(), vec![0; 12]].concat(); // 44 bytes: has a bone tag slot
        let mut anim = anim;
        anim[40..44].copy_from_slice(&7i32.to_le_bytes());
        let seq = [chunk(b"CPAN", &[]), chunk(b"ANIM", &anim), chunk(b"KRT0", &[f(&[0., 0., 0., 1., 1., 2., 3., 0.0]), f(&[0., 0., 0., 1., 4., 5., 6., 0.5])].concat())].concat();
        let animation = [chunk(b"NAME", b"walk\0\0\0\0"), chunk(b"DGAN", &[]), chunk(b"INFO", &[1u32.to_le_bytes().to_vec(), vec![0; 4]].concat()), seq].concat();
        
        [chunk(b"ANPK", &[]), chunk(b"INFO", &[1u32.to_le_bytes().to_vec(), name("ped", 24)].concat()), animation].concat()
    }

    #[test]
    fn one_block() {
        let blocks = parse_ifp(&sample()).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "ped");
        let a = &blocks[0].animations[0];
        assert_eq!((a.name.as_str(), a.sequences.len()), ("walk", 1));
        let s = &a.sequences[0];
        assert_eq!((s.name.as_str(), s.bone_tag, s.kind, s.keyframes.len()), ("Root", Some(7), TrackKind::RotationTranslation, 2));
        assert_eq!((s.keyframes[1].translation, s.keyframes[1].time), ([4., 5., 6.], 0.5));
    }

    #[test]
    fn truncated_and_bad() {
        let b = sample();
        assert!(parse_ifp(&b[..b.len() - 8]).is_err());
        assert!(parse_ifp(b"NOPE\0\0\0\0").is_err());
    }
}
