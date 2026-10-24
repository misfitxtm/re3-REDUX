//! GXT text files: `TKEY` (8-byte key + offset) and `TDAT` (UTF-16-ish chars) chunks.
//! Vice City adds a `TABL` chunk that points at per-mission tables inside the same file.
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// A chunk or table runs past the end of the file.
    Truncated,
    /// No `TKEY`/`TDAT` pair was found.
    MissingTable,
}

#[derive(Debug, Default)]
pub struct Table {
    keys: HashMap<Vec<u8>, u32>,
    data: Vec<u16>,
}

impl Table {
    /// Text for `key`, without its terminating 0.
    pub fn get(&self, key: &str) -> Option<&[u16]> {
        let start = *self.keys.get(key.as_bytes())? as usize / 2;
        let s = self.data.get(start..)?;
        Some(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &[u8]> {
        self.keys.keys().map(Vec::as_slice)
    }
}

#[derive(Debug)]
pub struct Gxt<'a> {
    bytes: &'a [u8],
    pub main: Table,
    /// Mission table name -> byte offset of its table (Vice City only).
    pub missions: Vec<(String, u32)>,
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    let s = b.get(at..at + 4).ok_or(Error::Truncated)?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

fn trim_nul(b: &[u8]) -> &[u8] {
    &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]
}

/// Walks chunks from `start`; stops once both tables are loaded, or at end of file.
fn read_chunks(b: &[u8], start: usize) -> Result<(Table, Vec<(String, u32)>), Error> {
    let (mut t, mut tkey, mut tdat, mut missions) = (Table::default(), false, false, vec![]);
    let mut keys = vec![];
    let mut at = start;
    while !(tkey && tdat) && at + 8 <= b.len() {
        let magic = &b[at..at + 4];
        let size = u32_at(b, at + 4)? as usize;
        at += 8;
        let body = b.get(at..at + size).ok_or(Error::Truncated)?;
        at += size;
        match (magic, size) {
            (_, 0) => {}
            (b"TABL", _) => {
                missions = body
                    .chunks_exact(12)
                    .map(|e| (String::from_utf8_lossy(trim_nul(&e[..8])).into_owned(), u32::from_le_bytes(e[8..].try_into().unwrap())))
                    .filter(|(n, _)| n != "MAIN") // MAIN points at the main TKEY, which has no name header
                    .collect();
            }
            (b"TKEY", _) => {
                keys = body.chunks_exact(12).map(|e| (trim_nul(&e[4..]).to_vec(), u32::from_le_bytes(e[..4].try_into().unwrap()))).collect();
                tkey = true;
            }
            (b"TDAT", _) => {
                t.data = body.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                tdat = true;
            }
            _ => {}
        }
    }
    if !(tkey && tdat) {
        return Err(Error::MissingTable);
    }
    for (k, off) in keys {
        t.keys.entry(k).or_insert(off); // first wins on duplicate keys
    }
    Ok((t, missions))
}

impl<'a> Gxt<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let (main, missions) = read_chunks(bytes, 0)?;
        Ok(Self { bytes, main, missions })
    }

    /// Loads a mission table by exact name; its table starts with the 8-byte name.
    pub fn mission(&self, name: &str) -> Result<Option<Table>, Error> {
        let Some(&(_, off)) = self.missions.iter().find(|(n, _)| n == name) else { return Ok(None) };
        read_chunks(self.bytes, off as usize + 8).map(|(t, _)| Some(t))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    English,
    French,
    German,
    Italian,
    Spanish,
}

const UPPER: [u16; 128] = [128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,173,173,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255];
const FRENCH_UPPER: [u16; 128] = [128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,65,65,65,65,132,133,69,69,69,69,73,73,73,73,79,79,79,79,85,85,85,85,173,173,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255];

impl Encoding {
    pub fn to_upper(self, c: u16) -> u16 {
        let ascii = if (b'a' as u16..=b'z' as u16).contains(&c) { Some(c - 32) } else { None };
        match (self, c) {
            (_, c) if ascii.is_some() => c - 32,
            (Self::French, 128..=255) => FRENCH_UPPER[c as usize - 128],
            (Self::German | Self::Italian | Self::Spanish, 128..=255) => UPPER[c as usize - 128],
            _ => c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(magic: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = magic.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }

    fn key(off: u32, k: &str) -> Vec<u8> {
        let mut e = off.to_le_bytes().to_vec();
        e.extend(k.as_bytes());
        e.resize(12, 0);
        e
    }

    fn wide(s: &str) -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn gta3_style_lookup() {
        let mut f = chunk(b"TKEY", &[key(0, "HELLO"), key(6, "BYE")].concat());
        f.extend(chunk(b"TDAT", &[wide("hi"), wide("ok")].concat()));
        let g = Gxt::parse(&f).unwrap();
        assert_eq!(g.main.get("HELLO").unwrap(), &[b'h' as u16, b'i' as u16]);
        assert_eq!(g.main.get("BYE").unwrap().len(), 2);
        assert!(g.main.get("NOPE").is_none());
    }

    #[test]
    fn vc_mission_table() {
        let mission = [b"M1\0\0\0\0\0\0".to_vec(), chunk(b"TKEY", &key(0, "A")), chunk(b"TDAT", &wide("x"))].concat();
        let main = [chunk(b"TKEY", &key(0, "K")), chunk(b"TDAT", &wide("y"))].concat();
        let mut tabl = b"M1\0\0\0\0\0\0".to_vec();
        let tabl_len = 8 + 12;
        let off = (tabl_len + main.len()) as u32;
        tabl.extend(off.to_le_bytes());
        let f = [chunk(b"TABL", &tabl), main, mission].concat();
        let g = Gxt::parse(&f).unwrap();
        assert_eq!(g.missions, vec![("M1".into(), off)]);
        assert_eq!(g.mission("M1").unwrap().unwrap().get("A").unwrap(), &[b'x' as u16]);
        assert!(g.mission("zz").unwrap().is_none());
    }

    #[test]
    fn errors() {
        assert_eq!(Gxt::parse(&chunk(b"TKEY", &[0; 12])).unwrap_err(), Error::MissingTable);
        assert_eq!(Gxt::parse(b"TKEY\xff\0\0\0").unwrap_err(), Error::Truncated);
    }

    #[test]
    fn upper() {
        assert_eq!(Encoding::English.to_upper(b'a' as u16), b'A' as u16);
        assert_eq!(Encoding::English.to_upper(200), 200);
        assert_eq!(Encoding::French.to_upper(150 + 1), FRENCH_UPPER[23]);
    }
}
