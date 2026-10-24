//! `.dir` archive index: 32-byte records (offset, size in 2048-byte sectors, 24-byte name).
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

pub const SECTOR: u64 = 2048;
const RECORD: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub offset: u32,
    pub size: u32,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Directory {
    entries: Vec<Entry>,
    by_name: HashMap<String, usize>,
}

impl Directory {
    pub fn parse(bytes: &[u8]) -> Self {
        let mut d = Self::default();
        // a trailing partial record is ignored, as the original read loop does
        for rec in bytes.chunks_exact(RECORD) {
            let name_bytes = &rec[8..];
            let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
            d.add(Entry {
                offset: u32::from_le_bytes(rec[0..4].try_into().unwrap()),
                size: u32::from_le_bytes(rec[4..8].try_into().unwrap()),
                name: String::from_utf8_lossy(&name_bytes[..len]).into_owned(),
            });
        }
        d
    }

    /// First entry wins on duplicate names (re3 `FIX_BUGS` behaviour).
    pub fn add(&mut self, e: Entry) {
        let key = e.name.to_ascii_lowercase();
        if self.by_name.contains_key(&key) {
            return;
        }
        self.by_name.insert(key, self.entries.len());
        self.entries.push(e);
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(&name.to_ascii_lowercase()).map(|&i| &self.entries[i])
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

impl Entry {
    /// Byte range of this entry inside the `.img` file.
    pub fn byte_range(&self) -> std::ops::Range<u64> {
        let start = u64::from(self.offset) * SECTOR;
        start..start + u64::from(self.size) * SECTOR
    }
}

/// A `.dir` index plus its `.img` data file.
pub struct Archive {
    pub dir: Directory,
    img: File,
}

impl Archive {
    pub fn open(dir: &Path, img: &Path) -> io::Result<Self> {
        Ok(Self { dir: Directory::parse(&std::fs::read(dir)?), img: File::open(img)? })
    }

    pub fn read(&mut self, e: &Entry) -> io::Result<Vec<u8>> {
        let r = e.byte_range();
        let mut buf = vec![0; (r.end - r.start) as usize];
        self.img.seek(SeekFrom::Start(r.start))?;
        self.img.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(offset: u32, size: u32, name: &str) -> Vec<u8> {
        let mut r = [0u8; RECORD];
        r[0..4].copy_from_slice(&offset.to_le_bytes());
        r[4..8].copy_from_slice(&size.to_le_bytes());
        r[8..8 + name.len()].copy_from_slice(name.as_bytes());
        r.to_vec()
    }

    #[test]
    fn parse_find_and_range() {
        let mut b = rec(3, 2, "Player.DFF");
        b.extend(rec(9, 1, "player.dff")); // duplicate, ignored
        b.extend([0u8; 5]); // partial trailing record
        let d = Directory::parse(&b);
        assert_eq!(d.entries().len(), 1);
        let e = d.find("PLAYER.dff").unwrap();
        assert_eq!(e.offset, 3);
        assert_eq!(e.byte_range(), 6144..10240);
        assert!(d.find("missing").is_none());
    }
}
