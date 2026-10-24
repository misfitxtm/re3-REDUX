//! Whitespace tokenizer standing in for the original's `sscanf`/`strtok` calls.
use std::str::FromStr;

pub(crate) struct Tok<'a>(std::str::SplitWhitespace<'a>);

impl<'a> Tok<'a> {
    pub fn new(l: &'a str) -> Self {
        Self(l.split_whitespace())
    }
    pub fn s(&mut self) -> Option<String> {
        self.0.next().map(str::to_owned)
    }
    pub fn n<T: FromStr>(&mut self) -> Option<T> {
        self.0.next()?.parse().ok()
    }
    pub fn hex(&mut self) -> Option<u32> {
        let t = self.0.next()?;
        u32::from_str_radix(t.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
    }
    pub fn rest(self) -> Vec<String> {
        self.0.map(str::to_owned).collect()
    }
}

/// Reads `n` numbers, or None if any is missing or malformed.
pub(crate) fn nums<T: FromStr>(t: &mut Tok, n: usize) -> Option<Vec<T>> {
    (0..n).map(|_| t.n()).collect()
}
