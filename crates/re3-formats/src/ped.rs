//! `ped.dat`, `pedstats.dat`, `pedgrp.dat`.
use crate::scan::Tok;

fn data_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .map(|l| l.chars().map(|c| if c == ',' || c == '\t' || c == '\r' { ' ' } else { c }).collect::<String>())
        .map(|l| l.trim_start().to_owned())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

#[derive(Debug, Default, PartialEq)]
pub struct PedType {
    pub name: String,
    /// The five numbers after the name (the first three are divided by 50 by the loader).
    pub values: [f32; 5],
    /// Flag names from the `Threat` / `Avoid` lines that follow this type.
    pub threats: Vec<String>,
    pub avoid: Vec<String>,
}

pub fn parse_ped_dat(text: &str) -> Vec<PedType> {
    let mut out: Vec<PedType> = vec![];
    for l in data_lines(text) {
        let mut t = Tok::new(&l);
        let word = t.s().unwrap_or_default();
        match (word.as_str(), out.last_mut()) {
            ("Threat", Some(p)) => p.threats = t.rest(),
            ("Avoid", Some(p)) => p.avoid = t.rest(),
            ("Threat" | "Avoid", None) => {}
            _ => {
                let v: Option<Vec<f32>> = (0..5).map(|_| t.n()).collect();
                if let Some(v) = v {
                    out.push(PedType { name: word, values: v.try_into().unwrap(), ..Default::default() });
                }
            }
        }
    }
    out
}

#[derive(Debug, PartialEq)]
pub struct PedStat {
    pub name: String,
    pub flee_distance: f32,
    pub heading_change_rate: f32,
    pub fear: i32,
    pub temper: i32,
    pub lawfulness: i32,
    pub sexiness: i32,
    pub attack_strength: f32,
    pub defend_weakness: f32,
    pub flags: i32,
}

pub fn parse_ped_stats(text: &str) -> Vec<PedStat> {
    data_lines(text)
        .filter_map(|l| {
            let mut t = Tok::new(&l);
            Some(PedStat {
                name: t.s()?,
                flee_distance: t.n()?,
                heading_change_rate: t.n()?,
                fear: t.n()?,
                temper: t.n()?,
                lawfulness: t.n()?,
                sexiness: t.n()?,
                attack_strength: t.n()?,
                defend_weakness: t.n()?,
                flags: t.n()?,
            })
        })
        .collect()
}

/// Model-name groups of exactly `per_group` names (8 in GTA III, 16 in Vice City);
/// shorter lines are dropped, as in the original.
pub fn parse_ped_groups(text: &str, per_group: usize) -> Vec<Vec<String>> {
    text.lines()
        .filter_map(|l| {
            let l = l.replace([',', '\r'], " ");
            let names: Vec<String> = l.split_whitespace().take_while(|w| !w.starts_with('#')).take(per_group).map(str::to_owned).collect();
            (names.len() == per_group).then_some(names)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ped_dat_threat_and_avoid() {
        let v = parse_ped_dat("# c\nCOP 1 2 3 4 5\nThreat GANG1 GANG2\nAvoid PLAYER1\nCIVMALE 0 0 0 0 0\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].threats, vec!["GANG1", "GANG2"]);
        assert_eq!(v[0].avoid, vec!["PLAYER1"]);
        assert!(v[1].threats.is_empty());
    }

    #[test]
    fn stats_and_groups() {
        let s = parse_ped_stats("STAT_COP 20.0 7.5 10 30 100 50 1.0 1.0 2\n");
        assert_eq!((s[0].fear, s[0].flags), (10, 2));
        let g = parse_ped_groups("a,b,c # note\nA,B,C,D # g\n", 3);
        assert_eq!(g, vec![vec!["a", "b", "c"], vec!["A", "B", "C"]]);
    }
}
