//! Series CSV reader shared by the oracle comparison tools (`smp_voice_ref`).
//!
//! Format: optional `#` comment lines, a header row (first column is the frame index) and one
//! numeric row per frame; empty cells are skipped.

use std::collections::HashMap;

pub type Cols = HashMap<String, Vec<f64>>;

pub fn read_csv(path: &str) -> Result<Cols, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut lines = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    let header: Vec<String> = lines
        .next()
        .ok_or("empty csv")?
        .split(',')
        .map(String::from)
        .collect();
    let mut cols: Cols = header[1..]
        .iter()
        .map(|n| (n.clone(), Vec::new()))
        .collect();
    for l in lines {
        for (name, cell) in header[1..].iter().zip(l.split(',').skip(1)) {
            if !cell.is_empty() {
                cols.get_mut(name)
                    .unwrap()
                    .push(cell.parse().map_err(|_| format!("bad cell {cell}"))?);
            }
        }
    }
    Ok(cols)
}

/// Integer words of a lo/hi series pair, as `f64`.
pub fn join(cols: &Cols, n: &str) -> Result<Vec<f64>, String> {
    let (lo, hi) = (cols.get(&format!("{n}_lo")), cols.get(&format!("{n}_hi")));
    let (lo, hi) = lo.zip(hi).ok_or(format!("series {n} missing"))?;
    Ok(lo
        .iter()
        .zip(hi)
        .map(|(l, h)| f64::from(((*h as i32) << 16) | (*l as i32 & 0xFFFF)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, text: &str) -> String {
        let p = std::env::temp_dir().join(name);
        std::fs::write(&p, text).unwrap();
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn read_csv_skips_comments_and_empty_cells() {
        let p = write(
            "ns4_csv_cols_a.csv",
            "# note
frame,a,b
0,1.5,2
1,,3
",
        );
        let cols = read_csv(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(cols["a"], vec![1.5]);
        assert_eq!(cols["b"], vec![2.0, 3.0]);
    }

    #[test]
    fn read_csv_rejects_bad_cells_and_missing_files() {
        let p = write(
            "ns4_csv_cols_b.csv",
            "frame,a
0,x
",
        );
        assert!(read_csv(&p).unwrap_err().contains("bad cell"));
        std::fs::remove_file(&p).ok();
        assert!(read_csv("no/such/file.csv").is_err());
    }

    #[test]
    fn join_combines_signed_hi_with_unsigned_lo_words() {
        let mut cols = Cols::new();
        cols.insert("x_lo".into(), vec![1.0, 65535.0]);
        cols.insert("x_hi".into(), vec![2.0, -1.0]);
        assert_eq!(join(&cols, "x").unwrap(), vec![131073.0, -1.0]);
        assert!(join(&cols, "y").is_err());
    }
}
