//! `carcols.dat`: a `col` section of RGB colours and a `car` section of per-model colour pairs.
use crate::scan::Tok;

#[derive(Debug, Default, PartialEq)]
pub struct CarCols {
    pub colours: Vec<[u8; 3]>,
    /// Model name -> (primary, secondary) colour index pairs.
    pub cars: Vec<(String, Vec<(u8, u8)>)>,
}

/// Leading integers on a line; reading stops at the first non-integer (a trailing comment).
fn leading_ints(t: &mut Tok, max: usize) -> Vec<i32> {
    std::iter::from_fn(|| t.n::<i32>()).take(max).collect()
}

pub fn parse_carcols(text: &str) -> CarCols {
    let mut out = CarCols::default();
    let mut section = "";
    for l in text.lines() {
        let l: String = l.chars().map(|c| if c == ',' || c == '\r' { ' ' } else { c }).collect();
        let l = l.trim_start();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if section.is_empty() {
            section = if l.starts_with("col") { "col" } else if l.starts_with("car") { "car" } else { "" };
        } else if l.starts_with("end") {
            section = "";
        } else {
            let mut t = Tok::new(l);
            if section == "col" {
                if let [r, g, b] = leading_ints(&mut t, 3)[..] {
                    out.colours.push([r as u8, g as u8, b as u8]);
                }
            } else if let Some(name) = t.s() {
                let v = leading_ints(&mut t, 16);
                out.cars.push((name, v.chunks_exact(2).map(|p| (p[0] as u8, p[1] as u8)).collect()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_and_cars() {
        let c = parse_carcols("col\n5,5,5 # black\n245,245,245\nend\ncar\nlandstal, 1,2, 3,4 # c\nend\n");
        assert_eq!(c.colours, vec![[5, 5, 5], [245, 245, 245]]);
        assert_eq!(c.cars, vec![("landstal".to_string(), vec![(1, 2), (3, 4)])]);
    }
}
