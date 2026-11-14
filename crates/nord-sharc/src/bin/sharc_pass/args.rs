//! Command line of `sharc-pass`: option parsing into [`Options`].
use nord_sharc::runner::msg::{parse_msg, parse_script, Msg};
use nord_sharc::runner::{parse_number, parse_value};

/// Entry of the PNO core 1 image (`docs/research/pno-dsp-map.md` section 2).
pub const ENTRY: u32 = 0x12_1007;
/// Poll loop of the pass handler and its mailbox.
pub const IDLE_PC: u32 = 0x12_1091;
pub const MAILBOX: u32 = 0xB_00E1;
/// Board gates the start-up waits for (`docs/research/sharc-sequencer.md` section 3).
pub const GATES: [(u32, u32); 4] = [
    (0x28ac_0380, 1),
    (0x3108_d008, 5),
    (0x3108_c06c, 0x400),
    (0x3108_c00c, 4),
];
/// Output pointer words `DM(0xB0159 + k)`, `k` = 0..5, written by the pass ping-pong select.
pub const OUT_POINTERS: u32 = 0xB_0159;

/// Which output lane pair becomes the stereo WAV: pointer set `set` (0..5), pair `pair` (0..2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lane {
    pub set: u32,
    pub pair: u32,
}

#[derive(Default)]
pub struct Options {
    pub image: String,
    pub boot: u64,
    pub passes: u32,
    pub max_steps: u64,
    pub msgs: Vec<Msg>,
    pub pokes: Vec<(u32, u32, u32)>,
    pub watch: Option<(u32, u32)>,
    pub lane: Option<Lane>,
    pub wav: Option<String>,
    pub raw: Option<String>,
    pub rate: u32,
    pub dumps: Vec<(u32, u32)>,
    /// `--rec ADDR:COUNT:FILE`: after every pass, `COUNT` DM words from `ADDR` appended to `FILE`.
    pub recs: Vec<(u32, u32, String)>,
    pub quiet: bool,
    pub no_ping_pong: bool,
    pub calls: bool,
}

/// `FILE[@PASS]`: script path and a pass offset added to every line.
pub fn parse_script_arg(s: &str) -> Result<(String, u32), String> {
    match s.rsplit_once('@') {
        Some((f, p)) => Ok((
            f.to_string(),
            parse_number(p).ok_or_else(|| format!("bad pass offset in {s:?}"))?,
        )),
        None => Ok((s.to_string(), 0)),
    }
}

/// `SET:PAIR` into a [`Lane`].
pub fn parse_lane(s: &str) -> Result<Lane, String> {
    let bad = || format!("bad --lane {s:?} (want SET:PAIR, SET 0..5, PAIR 0..2)");
    let (a, b) = s.split_once(':').ok_or_else(bad)?;
    let (set, pair) = (
        parse_number(a).filter(|v| *v < 6).ok_or_else(bad)?,
        parse_number(b).filter(|v| *v < 3).ok_or_else(bad)?,
    );
    Ok(Lane { set, pair })
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        boot: 3_000_000,
        passes: 1,
        max_steps: 2_000_000,
        rate: 48_000,
        ..Options::default()
    };
    let mut it = args.iter();
    let next = |what: &str, it: &mut std::slice::Iter<String>| {
        it.next().cloned().ok_or(format!("{what} needs a value"))
    };
    let num = |s: String, what: &str| parse_number(&s).ok_or(format!("bad {what} {s:?}"));
    while let Some(a) = it.next() {
        match a.as_str() {
            "--boot" => o.boot = u64::from(num(next(a, &mut it)?, a)?),
            "--passes" => o.passes = num(next(a, &mut it)?, a)?,
            "--max-pass-steps" => o.max_steps = u64::from(num(next(a, &mut it)?, a)?),
            "--rate" => o.rate = num(next(a, &mut it)?, a)?,
            "--script" => {
                let (f, shift) = parse_script_arg(&next(a, &mut it)?)?;
                let text = std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?;
                for mut m in parse_script(&text)? {
                    m.pass += shift;
                    o.msgs.push(m);
                }
            }
            "--msg" => o.msgs.push(parse_msg(&next(a, &mut it)?)?),
            "--poke" => {
                let s = next(a, &mut it)?;
                let bad = || format!("bad --poke {s:?} (want PASS:ADDR=V)");
                let (p, rest) = s.split_once(':').ok_or_else(bad)?;
                let (addr, v) = rest.split_once('=').ok_or_else(bad)?;
                o.pokes.push((
                    parse_number(p).ok_or_else(bad)?,
                    parse_number(addr).ok_or_else(bad)?,
                    parse_value(v).ok_or_else(bad)?,
                ));
            }
            "--watch" => {
                let s = next(a, &mut it)?;
                let (lo, len) = s.split_once(':').ok_or("--watch needs LO:LEN")?;
                o.watch = Some((
                    num(lo.into(), "--watch lo")?,
                    num(len.into(), "--watch len")?,
                ));
            }
            "--lane" => o.lane = Some(parse_lane(&next(a, &mut it)?)?),
            "--wav" => o.wav = Some(next(a, &mut it)?),
            "--raw" => o.raw = Some(next(a, &mut it)?),
            "--dump" => {
                let s = next(a, &mut it)?;
                let (addr, n) = s.split_once(':').ok_or("--dump needs ADDR:COUNT")?;
                o.dumps.push((
                    num(addr.into(), "--dump addr")?,
                    num(n.into(), "--dump count")?,
                ));
            }
            "--rec" => {
                let s = next(a, &mut it)?;
                let bad = || format!("bad --rec {s:?} (want ADDR:COUNT:FILE)");
                let mut f = s.splitn(3, ':');
                let (addr, n, file) = (f.next(), f.next(), f.next());
                o.recs.push((
                    parse_number(addr.ok_or_else(bad)?).ok_or_else(bad)?,
                    parse_number(n.ok_or_else(bad)?).ok_or_else(bad)?,
                    file.ok_or_else(bad)?.to_string(),
                ));
            }
            "--quiet" => o.quiet = true,
            "--no-ping-pong" => o.no_ping_pong = true,
            "--calls" => o.calls = true,
            _ if o.image.is_empty() && !a.starts_with("--") => o.image = a.clone(),
            _ => return Err(format!("unknown argument {a:?}")),
        }
    }
    if o.image.is_empty() {
        return Err("usage: sharc-pass IMAGE [--passes N] [--script FILE[@PASS]]... [--lane SET:PAIR --wav F] [--watch LO:LEN] (see the module docs)".into());
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn script_offsets_and_lanes_parse() {
        assert_eq!(parse_script_arg("a.script@12"), Ok(("a.script".into(), 12)));
        assert_eq!(parse_script_arg("a.script"), Ok(("a.script".into(), 0)));
        assert!(parse_script_arg("a@x").is_err());
        assert_eq!(parse_lane("2:1"), Ok(Lane { set: 2, pair: 1 }));
        for bad in ["6:0", "0:3", "1", "x:y"] {
            assert!(parse_lane(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rec_option_parses_address_count_and_file() {
        let o = parse_args(&args(&["img", "--rec", "0xc2b52:196:ring.bin"])).unwrap();
        assert_eq!(o.recs, vec![(0xc2b52, 196, "ring.bin".to_string())]);
        assert!(parse_args(&args(&["img", "--rec", "0x10:4"])).is_err());
    }

    #[test]
    fn options_parse_and_bad_arguments_are_rejected() {
        let o = parse_args(&args(&[
            "img",
            "--passes",
            "7",
            "--poke",
            "3:0x95b42=f1.0",
            "--msg",
            "2:0x10=5",
            "--watch",
            "0x95e46:96",
            "--lane",
            "2:0",
            "--quiet",
        ]))
        .unwrap();
        assert_eq!((o.passes, o.boot, o.quiet), (7, 3_000_000, true));
        assert_eq!(o.pokes, vec![(3, 0x95b42, 0x3F80_0000)]);
        assert_eq!((o.msgs.len(), o.watch), (1, Some((0x95e46, 96))));
        assert_eq!(o.lane, Some(Lane { set: 2, pair: 0 }));
        for bad in [
            &["img", "--poke", "1"][..],
            &["img", "--watch", "5"],
            &["img", "--lane", "9:0"],
            &["img", "--bogus"],
            &["img", "--passes"],
            &[],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_script_file_is_shifted_by_its_pass_offset() {
        let path = std::env::temp_dir().join("nord_sharc_pass_args_test.script");
        std::fs::write(&path, "0 0x95b42 0x3f800000\n2 0x10 5 # c\n").unwrap();
        let spec = format!("{}@10", path.display());
        let o = parse_args(&args(&["img", "--script", &spec])).unwrap();
        let passes: Vec<u32> = o.msgs.iter().map(|m| m.pass).collect();
        assert_eq!(passes, vec![10, 12]);
        std::fs::remove_file(path).ok();
    }
}
