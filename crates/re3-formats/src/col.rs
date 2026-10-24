//! Collision files (`.col` / entries in `gta3.img`): `COLL` blocks, one model each.
use crate::rw::{Cur, Error};

#[derive(Debug, PartialEq)]
pub struct Sphere {
    pub radius: f32,
    pub centre: [f32; 3],
    pub surface: u8,
    pub piece: u8,
}

#[derive(Debug, PartialEq)]
pub struct Line {
    pub p0: [f32; 3],
    pub p1: [f32; 3],
}

#[derive(Debug, PartialEq)]
pub struct Box3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub surface: u8,
    pub piece: u8,
}

#[derive(Debug, PartialEq)]
pub struct Face {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub surface: u8,
    pub piece: u8,
}

#[derive(Debug, PartialEq)]
pub struct ColModel {
    pub name: String,
    pub model_id: u16,
    pub bounding_radius: f32,
    pub bounding_centre: [f32; 3],
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub spheres: Vec<Sphere>,
    pub lines: Vec<Line>,
    pub boxes: Vec<Box3>,
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
}

/// Counts are 16-bit values stored in 4-byte slots; negative counts mean none.
fn count(r: &mut Cur) -> Result<usize, Error> {
    Ok((r.u32()? as i16).max(0) as usize)
}

fn model(body: &[u8]) -> Result<ColModel, Error> {
    let mut r = Cur::new(body);
    let name = r.name(22)?;
    let model_id = r.u16()?;
    let bounding_radius = r.f32()?;
    let bounding_centre = r.f32s::<3>()?;
    let (min, max) = (r.f32s::<3>()?, r.f32s::<3>()?);
    let spheres = (0..count(&mut r)?)
        .map(|_| {
            let (radius, centre) = (r.f32()?, r.f32s::<3>()?);
            let (surface, piece) = (r.u8()?, r.u8()?);
            r.take(2)?;
            Ok(Sphere { radius, centre, surface, piece })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let lines = (0..count(&mut r)?).map(|_| Ok(Line { p0: r.f32s::<3>()?, p1: r.f32s::<3>()? })).collect::<Result<Vec<_>, Error>>()?;
    let boxes = (0..count(&mut r)?)
        .map(|_| {
            let (min, max) = (r.f32s::<3>()?, r.f32s::<3>()?);
            let (surface, piece) = (r.u8()?, r.u8()?);
            r.take(2)?;
            Ok(Box3 { min, max, surface, piece })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let vertices = (0..count(&mut r)?).map(|_| r.f32s::<3>()).collect::<Result<Vec<_>, Error>>()?;
    let faces = (0..count(&mut r)?)
        .map(|_| {
            let (a, b, c) = (r.i32()?, r.i32()?, r.i32()?);
            let (surface, piece) = (r.u8()?, r.u8()?);
            r.take(2)?;
            Ok(Face { a, b, c, surface, piece })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if r.left() != 0 {
        return Err(Error::Bad("trailing bytes in collision model"));
    }
    Ok(ColModel { name, model_id, bounding_radius, bounding_centre, min, max, spheres, lines, boxes, vertices, faces })
}

/// Parses a stream of `COLL` blocks; trailing zero padding (archive sectors) is ignored.
pub fn parse_col(mut data: &[u8]) -> Result<Vec<ColModel>, Error> {
    let mut out = vec![];
    while !data.iter().all(|&b| b == 0) {
        let h = data.get(..8).ok_or(Error::Truncated)?;
        if &h[..4] != b"COLL" {
            return Err(Error::Bad("not a COLL block"));
        }
        let size = u32::from_le_bytes(h[4..].try_into().unwrap()) as usize;
        out.push(model(data.get(8..8 + size).ok_or(Error::Truncated)?)?);
        data = &data[8 + size..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn model_with_one_box() -> Vec<u8> {
        let mut body = b"testcol".to_vec();
        body.resize(22, 0);
        body.extend(7u16.to_le_bytes());
        body.extend(f(&[2.0, 0., 0., 0., -1., -1., -1., 1., 1., 1.]));
        body.extend([0u8; 4]); // spheres
        body.extend([0u8; 4]); // lines
        body.extend([1, 0, 0, 0]); // boxes
        body.extend(f(&[-1., -1., -1., 1., 1., 1.]));
        body.extend([4, 9, 0, 0]);
        body.extend([0u8; 4]); // vertices
        body.extend([0u8; 4]); // faces
        [b"COLL".to_vec(), (body.len() as u32).to_le_bytes().to_vec(), body].concat()
    }

    #[test]
    fn one_box_and_padding() {
        let mut b = model_with_one_box();
        b.extend([0u8; 100]); // sector padding
        let m = parse_col(&b).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].name.as_str(), m[0].model_id, m[0].boxes.len()), ("testcol", 7, 1));
        assert_eq!((m[0].boxes[0].surface, m[0].boxes[0].piece), (4, 9));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_col(b"NOPE\x04\0\0\0abcd").is_err());
        let b = model_with_one_box();
        assert!(parse_col(&b[..b.len() - 3]).is_err());
    }
}
