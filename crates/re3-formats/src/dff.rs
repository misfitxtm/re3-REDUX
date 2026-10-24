//! Models (`.dff`): a clump of frames, geometries and atomics.
use crate::rw::{self, Chunk, Cur, Error};

#[derive(Debug, PartialEq)]
pub struct Frame {
    pub rotation: [f32; 9],
    pub position: [f32; 3],
    pub parent: i32,
    pub flags: u32,
    /// From the frame's node-name extension (empty if absent).
    pub name: String,
    /// Bone hierarchy for animated characters.
    pub hanim: Option<HAnim>,
}

#[derive(Debug, PartialEq)]
pub struct HAnim {
    pub node_id: i32,
    pub flags: u32,
    pub key_frame_size: u32,
    /// (node id, node index, flags) per bone; empty for non-root nodes.
    pub bones: Vec<(i32, i32, i32)>,
}

/// Vertex skinning data.
#[derive(Debug, PartialEq)]
pub struct Skin {
    pub num_bones: u8,
    pub used_bones: Vec<u8>,
    pub max_weights: u8,
    pub indices: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub inverse_matrices: Vec<[f32; 16]>,
}

#[derive(Debug, PartialEq)]
pub struct Texture {
    pub name: String,
    pub mask: String,
    pub filter_addressing: u32,
}

#[derive(Debug, PartialEq)]
pub struct Material {
    pub flags: u32,
    pub rgba: [u8; 4],
    pub texture: Option<Texture>,
    /// ambient, specular, diffuse (zeros in older streams).
    pub surface: [f32; 3],
}

#[derive(Debug, PartialEq)]
pub struct Mesh {
    pub material: u32,
    pub indices: Vec<u32>,
}

#[derive(Debug, PartialEq)]
pub struct Geometry {
    pub flags: u32,
    pub triangles: Vec<[u16; 4]>,
    /// bounding sphere (x, y, z, radius) of the first morph target
    pub bounds: [f32; 4],
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub prelit: Vec<[u8; 4]>,
    /// One list of UVs per texture-coordinate set.
    pub uvs: Vec<Vec<[f32; 2]>>,
    pub materials: Vec<Material>,
    pub tri_strip: bool,
    pub meshes: Vec<Mesh>,
    pub skin: Option<Skin>,
    /// Extension blocks we do not decode (morph, 2dfx, breakable, ...): (type, size).
    pub other_ext: Vec<(u32, usize)>,
}

#[derive(Debug, PartialEq)]
pub struct Atomic {
    pub frame: u32,
    pub geometry: u32,
    pub flags: u32,
}

#[derive(Debug, PartialEq)]
pub struct Light {
    pub frame: u32,
    pub radius: f32,
    pub colour: [f32; 3],
    pub minus_cos_angle: f32,
    pub flags: u16,
    pub kind: u16,
}

#[derive(Debug, PartialEq)]
pub struct Dff {
    pub frames: Vec<Frame>,
    pub geometries: Vec<Geometry>,
    pub atomics: Vec<Atomic>,
    /// Vice City street lamps and the like carry lights inside the model.
    pub lights: Vec<Light>,
}

const TRISTRIP: u32 = 0x01;
const TEXTURED: u32 = 0x04;
const PRELIT: u32 = 0x08;
const NORMALS: u32 = 0x10;
const TEXTURED2: u32 = 0x80;

fn texture(c: Chunk) -> Result<Texture, Error> {
    let parts = rw::children(c.body)?;
    let st = parts.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("texture struct"))?;
    let fa = u32::from_le_bytes(st.body.get(..4).ok_or(Error::Truncated)?.try_into().unwrap());
    let (n, m) = (parts.get(1).ok_or(Error::Truncated)?, parts.get(2).ok_or(Error::Truncated)?);
    Ok(Texture { name: rw::string(*n), mask: rw::string(*m), filter_addressing: fa })
}

fn material(c: Chunk) -> Result<Material, Error> {
    let version = rw::unpack_version(c.version);
    let parts = rw::children(c.body)?;
    let mut r = Cur::new(parts.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("material struct"))?.body);
    let flags = r.u32()?;
    let rgba = r.take(4)?.try_into().unwrap();
    r.u32()?;
    let textured = r.u32()? != 0;
    let surface = if version > 0x30400 { r.f32s::<3>()? } else { [0.0; 3] };
    let texture = if textured {
        Some(texture(*parts.iter().find(|p| p.ty == rw::TEXTURE).ok_or(Error::Bad("missing texture"))?)?)
    } else {
        None
    };
    Ok(Material { flags, rgba, texture, surface })
}

fn bin_mesh(body: &[u8]) -> Result<(bool, Vec<Mesh>), Error> {
    let mut r = Cur::new(body);
    let strip = r.u32()? == 1;
    let (n, _total) = (r.u32()?, r.u32()?);
    let mut meshes = vec![];
    for _ in 0..n {
        let (count, material) = (r.u32()?, r.u32()?);
        let indices = (0..count).map(|_| r.u32()).collect::<Result<_, _>>()?;
        meshes.push(Mesh { material, indices });
    }
    Ok((strip, meshes))
}

fn skin(body: &[u8], num_verts: usize) -> Result<Skin, Error> {
    let mut r = Cur::new(body);
    let (num_bones, num_used, max_weights) = (r.u8()?, r.u8()?, r.u8()?);
    r.u8()?;
    let used_bones = r.take(num_used as usize)?.to_vec();
    let indices = (0..num_verts).map(|_| r.take(4).map(|b| [b[0], b[1], b[2], b[3]])).collect::<Result<_, _>>()?;
    let weights = (0..num_verts).map(|_| r.f32s::<4>()).collect::<Result<_, _>>()?;
    let mut inverse_matrices = vec![];
    for _ in 0..num_bones {
        if num_used == 0 {
            r.u32()?; // older streams put 4 unused bytes before every matrix
        }
        inverse_matrices.push(r.f32s::<16>()?);
    }
    // what is left is the (unused on PC) split data: nothing, or 12 bytes
    if !matches!(r.left(), 0 | 12) {
        return Err(Error::Bad("skin data has unexpected size"));
    }
    Ok(Skin { num_bones, used_bones, max_weights, indices, weights, inverse_matrices })
}

fn hanim(body: &[u8]) -> Result<HAnim, Error> {
    let mut r = Cur::new(body);
    r.u32()?; // version
    let (node_id, count) = (r.i32()?, r.i32()?);
    let (mut flags, mut key_frame_size, mut bones) = (0, 0, vec![]);
    if count > 0 {
        flags = r.u32()?;
        key_frame_size = r.u32()?;
        for _ in 0..count {
            bones.push((r.i32()?, r.i32()?, r.i32()?));
        }
    }
    Ok(HAnim { node_id, flags, key_frame_size, bones })
}

fn geometry(c: Chunk) -> Result<Geometry, Error> {
    let version = rw::unpack_version(c.version);
    let parts = rw::children(c.body)?;
    let mut r = Cur::new(parts.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("geometry struct"))?.body);
    let flags = r.u32()?;
    let (num_tris, num_verts, morphs) = (r.u32()? as usize, r.u32()? as usize, r.u32()?);
    if version < 0x34000 {
        r.f32s::<3>()?; // ambient, diffuse, specular
    }
    let uv_sets = match (flags >> 16) & 0xFF {
        0 if flags & TEXTURED != 0 => 1,
        0 if flags & TEXTURED2 != 0 => 2,
        n => n as usize,
    };
    let prelit = if flags & PRELIT != 0 { (0..num_verts).map(|_| r.take(4).map(|p| [p[0], p[1], p[2], p[3]])).collect::<Result<_, _>>()? } else { vec![] };
    let mut uvs = vec![];
    for _ in 0..uv_sets {
        uvs.push((0..num_verts).map(|_| Ok([r.f32()?, r.f32()?])).collect::<Result<Vec<_>, Error>>()?);
    }
    let triangles = (0..num_tris).map(|_| Ok([r.u16()?, r.u16()?, r.u16()?, r.u16()?])).collect::<Result<Vec<_>, Error>>()?;
    let (mut bounds, mut vertices, mut normals) = ([0.0; 4], vec![], vec![]);
    for m in 0..morphs {
        let b = r.f32s::<4>()?;
        let (has_v, has_n) = (r.u32()? != 0, r.u32()? != 0);
        let v: Vec<[f32; 3]> = if has_v { (0..num_verts).map(|_| r.f32s::<3>()).collect::<Result<_, _>>()? } else { vec![] };
        let n: Vec<[f32; 3]> = if has_n { (0..num_verts).map(|_| r.f32s::<3>()).collect::<Result<_, _>>()? } else { vec![] };
        if m == 0 {
            (bounds, vertices, normals) = (b, v, n);
        }
    }
    let _ = NORMALS;
    if r.left() != 0 {
        return Err(Error::Bad("geometry struct has trailing bytes"));
    }
    let list = parts.iter().find(|p| p.ty == rw::MATERIAL_LIST).ok_or(Error::Bad("missing material list"))?;
    let lp = rw::children(list.body)?;
    let count = u32::from_le_bytes(lp.first().ok_or(Error::Truncated)?.body.get(..4).ok_or(Error::Truncated)?.try_into().unwrap()) as usize;
    let materials: Vec<Material> = lp[1..].iter().filter(|p| p.ty == rw::MATERIAL).map(|p| material(*p)).collect::<Result<_, _>>()?;
    if materials.len() != count {
        return Err(Error::Bad("material count mismatch"));
    }
    let (mut tri_strip, mut meshes, mut other_ext, mut skin_data) = (flags & TRISTRIP != 0, vec![], vec![], None);
    for ext in parts.iter().filter(|p| p.ty == rw::EXTENSION) {
        for (ty, body) in rw::extension(*ext)? {
            match ty {
                rw::BIN_MESH => (tri_strip, meshes) = bin_mesh(&body)?,
                rw::SKIN => skin_data = Some(skin(&body, num_verts)?),
                _ => other_ext.push((ty, body.len())),
            }
        }
    }
    Ok(Geometry { flags, triangles, bounds, vertices, normals, prelit, uvs, materials, tri_strip, meshes, skin: skin_data, other_ext })
}

fn frames(c: Chunk) -> Result<Vec<Frame>, Error> {
    let parts = rw::children(c.body)?;
    let mut r = Cur::new(parts.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("frame list struct"))?.body);
    let n = r.u32()? as usize;
    let mut out = (0..n)
        .map(|_| Ok(Frame { rotation: r.f32s::<9>()?, position: r.f32s::<3>()?, parent: r.i32()?, flags: r.u32()?, name: String::new(), hanim: None }))
        .collect::<Result<Vec<_>, Error>>()?;
    let exts: Vec<_> = parts[1..].iter().filter(|p| p.ty == rw::EXTENSION).collect();
    if exts.len() != n {
        return Err(Error::Bad("frame extension count mismatch"));
    }
    for (f, e) in out.iter_mut().zip(exts) {
        for (ty, body) in rw::extension(*e)? {
            match ty {
                rw::NODE_NAME => f.name = String::from_utf8_lossy(&body).trim_end_matches('\0').to_owned(),
                rw::HANIM => f.hanim = Some(hanim(&body)?),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// An atomic, plus its own geometry when the model has no geometry list (older streams).
fn atomic(c: Chunk) -> Result<(Atomic, Option<Geometry>), Error> {
    let ap = rw::children(c.body)?;
    let mut r = Cur::new(ap.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("atomic struct"))?.body);
    let a = Atomic { frame: r.u32()?, geometry: r.u32()?, flags: r.u32()? };
    let embedded = ap.iter().find(|p| p.ty == rw::GEOMETRY).map(|g| geometry(*g)).transpose()?;
    Ok((a, embedded))
}

/// Reads the clump the way the game does: block after block by count, without trusting the
/// clump's own size (some old models understate it).
pub fn parse_dff(data: &[u8]) -> Result<Dff, Error> {
    let (clump, _) = rw::chunk(data)?;
    if clump.ty != rw::CLUMP {
        return Err(Error::Unexpected { expected: rw::CLUMP, found: clump.ty });
    }
    let body = data.get(12..).ok_or(Error::Truncated)?;
    let (st, after_struct) = rw::expect(body, rw::STRUCT)?;
    let num_atomics = u32::from_le_bytes(st.body.get(..4).ok_or(Error::Truncated)?.try_into().unwrap()) as usize;
    let word = |i: usize| st.body.get(i..i + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0) as usize;
    let (num_lights, num_cameras) = (word(4), word(8));
    if num_cameras != 0 {
        return Err(Error::Bad("clump has cameras"));
    }
    let (fl, after_frames) = rw::expect(after_struct, rw::FRAME_LIST)?;
    let (mut geometries, mut next) = (vec![], after_frames);
    if rw::chunk(after_frames)?.0.ty == rw::GEOMETRY_LIST {
        let (gl, rest) = rw::chunk(after_frames)?;
        next = rest;
        let gparts = rw::children(gl.body)?;
        let gcount = u32::from_le_bytes(gparts.first().ok_or(Error::Truncated)?.body.get(..4).ok_or(Error::Truncated)?.try_into().unwrap()) as usize;
        geometries = gparts[1..].iter().filter(|p| p.ty == rw::GEOMETRY).map(|p| geometry(*p)).collect::<Result<_, _>>()?;
        if geometries.len() != gcount {
            return Err(Error::Bad("geometry count mismatch"));
        }
    }
    let mut atomics = vec![];
    for _ in 0..num_atomics {
        let (a, rest) = rw::expect(next, rw::ATOMIC)?;
        let (mut a, embedded) = atomic(a)?;
        if let Some(g) = embedded {
            a.geometry = geometries.len() as u32;
            geometries.push(g);
        }
        atomics.push(a);
        next = rest;
    }
    let mut lights = vec![];
    for _ in 0..num_lights {
        let (frame, rest) = rw::expect(next, rw::STRUCT)?;
        let frame = u32::from_le_bytes(frame.body.get(..4).ok_or(Error::Truncated)?.try_into().unwrap());
        let (light, rest) = rw::expect(rest, rw::LIGHT)?;
        let parts = rw::children(light.body)?;
        let mut r = Cur::new(parts.first().filter(|p| p.ty == rw::STRUCT).ok_or(Error::Bad("light struct"))?.body);
        let radius = r.f32()?;
        let colour = r.f32s::<3>()?;
        lights.push(Light { frame, radius, colour, minus_cos_angle: r.f32()?, flags: r.u16()?, kind: r.u16()? });
        next = rest;
    }
    Ok(Dff { frames: frames(fl)?, geometries, atomics, lights })
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: u32 = 0x1803FFFF; // RenderWare 3.6.0.3

    fn block(ty: u32, body: &[u8]) -> Vec<u8> {
        [ty.to_le_bytes(), (body.len() as u32).to_le_bytes(), V.to_le_bytes()].concat().into_iter().chain(body.iter().copied()).collect()
    }

    fn f(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn u(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// One frame, one triangle with one untextured material, one atomic.
    fn triangle_model() -> Vec<u8> {
        let frame = [f(&[1., 0., 0., 0., 1., 0., 0., 0., 1., 1., 2., 3.]), u(&[0xFFFF_FFFF, 0])].concat();
        let frames = [block(rw::STRUCT, &[u(&[1]), frame].concat()), block(rw::EXTENSION, &block(rw::NODE_NAME, b"root"))].concat();
        let mat_struct = [u(&[0, 0xFFFF_FFFF, 0, 0]), f(&[1., 1., 1.])].concat();
        let matlist = [block(rw::STRUCT, &u(&[1, 0xFFFF_FFFF])), block(rw::MATERIAL, &[block(rw::STRUCT, &mat_struct), block(rw::EXTENSION, &[])].concat())].concat();
        // flags, 1 triangle, 3 vertices, 1 morph target; triangle = (v2, v1, material, v3)
        let tri: Vec<u8> = [1u16, 0, 0, 2].iter().flat_map(|x| x.to_le_bytes()).collect();
        let geo_struct = [u(&[0, 1, 3, 1]), tri, f(&[0., 0., 0., 1.]), u(&[1, 0]), f(&[0., 0., 0., 1., 0., 0., 0., 1., 0.])].concat();
        let geo = [block(rw::STRUCT, &geo_struct), block(rw::MATERIAL_LIST, &matlist), block(rw::EXTENSION, &[])].concat();
        let geos = [block(rw::STRUCT, &u(&[1])), block(rw::GEOMETRY, &geo)].concat();
        let atomic = [block(rw::STRUCT, &u(&[0, 0, 5, 0])), block(rw::EXTENSION, &[])].concat();
        block(rw::CLUMP, &[block(rw::STRUCT, &u(&[1, 0, 0])), block(rw::FRAME_LIST, &frames), block(rw::GEOMETRY_LIST, &geos), block(rw::ATOMIC, &atomic)].concat())
    }

    #[test]
    fn minimal_model() {
        let d = parse_dff(&triangle_model()).unwrap();
        assert_eq!((d.frames.len(), d.geometries.len(), d.atomics.len(), d.lights.len()), (1, 1, 1, 0));
        assert_eq!((d.frames[0].name.as_str(), d.frames[0].position, d.frames[0].parent), ("root", [1., 2., 3.], -1));
        assert_eq!((d.geometries[0].triangles.len(), d.geometries[0].vertices.len(), d.geometries[0].materials.len()), (1, 3, 1));
        assert_eq!(d.geometries[0].vertices[1], [1., 0., 0.]);
        assert_eq!(d.atomics[0], Atomic { frame: 0, geometry: 0, flags: 5 });
    }

    #[test]
    fn truncated_model_is_an_error() {
        let b = triangle_model();
        assert!(parse_dff(&b[..b.len() - 20]).is_err());
        assert!(parse_dff(&b[..8]).is_err());
    }
}
