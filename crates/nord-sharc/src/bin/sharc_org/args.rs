//! Command line of `sharc-org`: option parsing into [`Options`].
use crate::audio::{parse_spec, BufSpec};
use crate::msg::{parse_msg, parse_rx_irq, parse_script, Msg, STRIP_BASE};
use crate::pass::PassConfig;

pub fn parse_number(s: &str) -> Option<u32> {
    match s.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// A data value: number, `0x` hex or `f<float>` (float bit pattern).
pub fn parse_value(s: &str) -> Option<u32> {
    match s.strip_prefix('f') {
        Some(f) => f.parse::<f32>().ok().map(f32::to_bits),
        None => parse_number(s),
    }
}

/// `A=V` into an (address, value) pair.
fn parse_assign(s: &str) -> Result<(u32, u32), String> {
    let bad = || format!("bad ADDR=VALUE {s:?}");
    let (a, v) = s.split_once('=').ok_or_else(bad)?;
    Ok((
        parse_number(a).ok_or_else(bad)?,
        parse_value(v).ok_or_else(bad)?,
    ))
}

/// `REG=V` for R/I/M/L/B registers, returning the universal register code.
fn parse_register(s: &str) -> Result<(u8, u32), String> {
    let bad = || format!("bad --set {s:?} (R I M L B 0-15)");
    let (name, v) = s.split_once('=').ok_or_else(bad)?;
    let group = match name.get(..1) {
        Some("R") => 0,
        Some("I") => 1,
        Some("M") => 2,
        Some("L") => 3,
        Some("B") => 4,
        _ => return Err(bad()),
    };
    let n: u8 = name[1..].parse().ok().filter(|n| *n < 16).ok_or_else(bad)?;
    Ok(((group << 4) | n, parse_value(v).ok_or_else(bad)?))
}

#[derive(Default)]
pub struct Options {
    pub image: String,
    pub entry: u32,
    pub boot: u64,
    pub cfg: Option<PassConfig>,
    pub passes: u32,
    pub devs: Vec<(u32, u32)>,
    pub regs: Vec<(u8, u32)>,
    pub pre: Vec<(u32, u32)>,
    pub pokes: Vec<(u32, u32, u32)>,
    pub lenient: bool,
    pub diff: Option<(u32, u32)>,
    pub bufs: Vec<BufSpec>,
    pub wav: Option<String>,
    pub rate: u32,
    pub csv: Option<String>,
    pub dumps: Vec<(u32, u32)>,
    pub watch: Option<(u32, u32, usize)>,
    pub boot_stores: bool,
    pub coverage: bool,
    pub flags: Vec<(u8, bool)>,
    pub msgs: Vec<Msg>,
    pub msg_base: u32,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        entry: 0x12_5C88,
        boot: 100_000,
        passes: 1,
        rate: 48_000,
        msg_base: STRIP_BASE,
        ..Options::default()
    };
    let mut cfg = PassConfig {
        trigger: 0xB_00E2,
        idle_pc: 0x12_5DDF,
        max_steps: 200_000,
        irq: None,
    };
    let mut it = args.iter().peekable();
    let need = |it: &mut std::iter::Peekable<std::slice::Iter<String>>, what: &str| {
        it.next().cloned().ok_or(format!("{what} needs a value"))
    };
    let num = |s: String, what: &str| parse_number(&s).ok_or(format!("bad {what} {s:?}"));
    while let Some(a) = it.next() {
        match a.as_str() {
            "--entry" => o.entry = num(need(&mut it, a)?, a)?,
            "--boot" => o.boot = u64::from(num(need(&mut it, a)?, a)?),
            "--trigger" => cfg.trigger = num(need(&mut it, a)?, a)?,
            "--idle-pc" => cfg.idle_pc = num(need(&mut it, a)?, a)?,
            "--max-pass-steps" => cfg.max_steps = u64::from(num(need(&mut it, a)?, a)?),
            "--passes" => o.passes = num(need(&mut it, a)?, a)?,
            "--rate" => o.rate = num(need(&mut it, a)?, a)?,
            "--dev" => o.devs.push(parse_assign(&need(&mut it, a)?)?),
            "--pre" => o.pre.push(parse_assign(&need(&mut it, a)?)?),
            "--set" => o.regs.push(parse_register(&need(&mut it, a)?)?),
            "--poke" => {
                let s = need(&mut it, a)?;
                let (p, rest) = s.split_once(':').ok_or("--poke needs PASS:ADDR=V")?;
                let (addr, v) = parse_assign(rest)?;
                o.pokes.push((num(p.to_string(), "--poke pass")?, addr, v));
            }
            "--msg" => o.msgs.push(parse_msg(&need(&mut it, a)?)?),
            "--msg-script" => {
                let f = need(&mut it, a)?;
                let text = std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?;
                o.msgs.extend(parse_script(&text)?);
            }
            "--msg-base" => o.msg_base = num(need(&mut it, a)?, a)?,
            "--rx-irq" => cfg.irq = Some(parse_rx_irq(&need(&mut it, a)?)?),
            "--lenient" => o.lenient = true,
            "--diff" => {
                // optional LO:HI range as the next argument
                let range = match it.peek() {
                    Some(n) if !n.starts_with("--") => {
                        let n = need(&mut it, a)?;
                        let (lo, hi) = n.split_once(':').ok_or("--diff range is LO:HI")?;
                        (num(lo.into(), "--diff lo")?, num(hi.into(), "--diff hi")?)
                    }
                    _ => (0, u32::MAX),
                };
                o.diff = Some(range);
            }
            "--buf" => o.bufs.push(parse_spec(&need(&mut it, a)?)?),
            "--wav" => o.wav = Some(need(&mut it, a)?),
            "--csv" => o.csv = Some(need(&mut it, a)?),
            "--boot-stores" => o.boot_stores = true,
            "--coverage" => o.coverage = true,
            "--flag" => {
                // N=LEVEL drives the FLAGn input pin (Core1 sections wait for FLAG1 high)
                let s = need(&mut it, a)?;
                let (n, l) = s.split_once('=').ok_or("--flag needs N=LEVEL")?;
                let n = num(n.into(), "--flag number")?;
                let high = num(l.into(), "--flag level")? != 0;
                o.flags.push((
                    u8::try_from(n)
                        .ok()
                        .filter(|n| *n < 16)
                        .ok_or("bad --flag number")?,
                    high,
                ));
            }
            "--watch" => {
                // LO:LEN[:LIMIT] logs every store into the window during the passes
                let s = need(&mut it, a)?;
                let mut f = s.split(':');
                let lo = num(f.next().unwrap_or("").into(), "--watch lo")?;
                let len = num(f.next().unwrap_or("").into(), "--watch len")?;
                let limit = f
                    .next()
                    .map_or(Ok(100_000), |l| num(l.into(), "--watch limit"))?;
                o.watch = Some((lo, len, limit as usize));
            }
            "--dump" => {
                let s = need(&mut it, a)?;
                let (addr, n) = s.split_once(':').ok_or("--dump needs ADDR:COUNT")?;
                o.dumps.push((
                    num(addr.into(), "--dump addr")?,
                    num(n.into(), "--dump count")?,
                ));
            }
            _ if o.image.is_empty() && !a.starts_with("--") => o.image = a.clone(),
            _ => return Err(format!("unknown argument {a:?}")),
        }
    }
    if o.image.is_empty() {
        return Err("usage: sharc-org IMAGE [--entry PC] [--passes N] [--poke PASS:ADDR=V]... [--diff] [--buf SPEC]... [--wav F] [--csv F] (see the module docs)".into());
    }
    o.cfg = Some(cfg);
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn values_registers_and_assignments_parse() {
        assert_eq!(parse_value("f1.0"), Some(0x3F80_0000));
        assert_eq!(parse_value("0x10"), Some(16));
        assert_eq!(parse_assign("0xB0000=f0.5"), Ok((0xB_0000, 0x3F00_0000)));
        assert!(parse_assign("nonsense").is_err());
        assert_eq!(parse_register("I8=0x10"), Ok((0x18, 0x10)));
        assert!(parse_register("Q1=1").is_err());
        assert!(parse_register("R16=1").is_err());
    }

    #[test]
    fn options_parse_and_bad_arguments_are_rejected() {
        let o = parse_args(&args(&[
            "img",
            "--passes",
            "3",
            "--poke",
            "1:0xB0000=f0.5",
            "--diff",
            "0xB0000:0xB00FF",
            "--flag",
            "1=1",
            "--buf",
            "@0xB2EF4:8",
            "--watch",
            "0x90000:16",
            "--entry",
            "0x10",
        ]))
        .unwrap();
        assert_eq!(
            (o.passes, o.entry, o.diff),
            (3, 0x10, Some((0xB_0000, 0xB_00FF)))
        );
        assert_eq!(o.pokes, vec![(1, 0xB_0000, 0x3F00_0000)]);
        assert_eq!(
            (o.flags.clone(), o.bufs.len(), o.watch),
            (vec![(1, true)], 1, Some((0x9_0000, 16, 100_000)))
        );
        // a bare --diff covers everything and does not eat the next option
        let o = parse_args(&args(&["img", "--diff", "--lenient"])).unwrap();
        assert_eq!((o.diff, o.lenient), (Some((0, u32::MAX)), true));
        for bad in [
            &["img", "--poke", "1"][..],
            &["img", "--dump", "5"],
            &["img", "--flag", "16=1"],
            &["img", "--msg", "5"],
            &["img", "--rx-irq", "P4I"],
            &["img", "--buf", "zz"],
            &["img", "--watch", "5"],
            &["img", "--diff", "5"],
            &["img", "--bogus"],
            &[],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }
}
