//! `sharc-organ IMAGE [options]` runs the organ chain of PNO Core1 in the SHARC oracle:
//! boot to the idle loop, then per call the oscillator bank (pc `0x122620`), the mix stage
//! (`0x12268F`, once per set) and optionally the post stage (`0x12272D`), with scripted data pokes
//! between calls, capturing the words the mix stage wrote.
//! Method and results: `docs/research/organ-oracle-audio.md`.
//!
//! ```text
//! --calls N         number of 2-frame calls (default 1)     --boot N   start-up steps (default 1500000)
//! --sets A,B        mix-stage sets to run per call (default 0); set s uses the record at
//!                   DM(0xB471C + 4 s .. +3) = I2, I3, I10, I11 (I1 = 0xB473C + 4 s)
//! --out-base ADDR   output pointer I0 of the first set (default 0xB0200); set k writes 4 words at
//!                   ADDR + 4 k per call (2 frames x lanes PEx, PEy)
//! --no-bank         skip the oscillator bank (mix-only experiments: poke the x array directly)
//! --post            run the post stage 0x12272D after the mixes of every call
//! --mode1 V         OR V into MODE1 after the start-up (default 0x200000 = PEYEN)
//! --pre ADDR=V      data poke before the first call       --script FILE  lines `CALL ADDR VALUE`
//! --out FILE        per call the words of the --range windows (default: the mix outputs), LE u32
//! --range ADDR:COUNT  capture window (repeatable)         --dump ADDR:COUNT  print words at the end
//! ```
use nord_sharc::harness::{boot_pno, call_once, parse_number, parse_script};
use std::process::ExitCode;

const BANK: (u32, u32) = (0x122620, 0x12268C);
const MIX: (u32, u32) = (0x12268F, 0x12272A);
const POST_ENTRY: u32 = 0x12272D;
/// Return of the post stage: its `RTS` at byte 0x245364 (pc 0x1229B2).
const POST_RET: u32 = 0x1229B2;
const RECORD_BASE: u32 = 0xB473C;
const STEP_LIMIT: u64 = 400_000;

struct Opts {
    image: String,
    calls: u64,
    boot: u64,
    sets: Vec<u32>,
    out_base: u32,
    post: bool,
    no_bank: bool,
    mode1: u32,
    pokes: Vec<(u64, u32, u32)>,
    out: Option<String>,
    ranges: Vec<(u32, u32)>,
    dumps: Vec<(u32, u32)>,
}

fn pair(spec: &str, sep: char) -> Result<(u32, u32), String> {
    let (k, v) = spec.split_once(sep).ok_or("needs A<sep>B")?;
    let k = parse_number(k).ok_or("bad number")? as u32;
    let v = parse_number(v).ok_or("bad number")? as u32;
    Ok((k, v))
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        image: String::new(),
        calls: 1,
        boot: 1_500_000,
        sets: vec![0],
        out_base: 0xB0200,
        post: false,
        no_bank: false,
        mode1: 0x20_0000,
        pokes: Vec::new(),
        out: None,
        ranges: Vec::new(),
        dumps: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |what: &str| it.next().cloned().ok_or(format!("{a} needs {what}"));
        match a.as_str() {
            "--calls" => o.calls = parse_number(&val("N")?).ok_or("bad --calls")?,
            "--boot" => o.boot = parse_number(&val("N")?).ok_or("bad --boot")?,
            "--mode1" => o.mode1 = parse_number(&val("V")?).ok_or("bad --mode1")? as u32,
            "--out-base" => {
                o.out_base = parse_number(&val("ADDR")?).ok_or("bad --out-base")? as u32
            }
            "--post" => o.post = true,
            "--no-bank" => o.no_bank = true,
            "--out" => o.out = Some(val("FILE")?),
            "--sets" => {
                o.sets = val("A,B")?
                    .split(',')
                    .map(|s| parse_number(s).map(|n| n as u32).ok_or("bad --sets"))
                    .collect::<Result<_, _>>()?;
            }
            "--pre" => {
                let (k, v) = pair(&val("A=V")?, '=')?;
                o.pokes.push((0, k, v));
            }
            "--script" => {
                let path = val("FILE")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                o.pokes.extend(parse_script(&text)?);
            }
            "--range" => o.ranges.push(pair(&val("ADDR:COUNT")?, ':')?),
            "--dump" => o.dumps.push(pair(&val("ADDR:COUNT")?, ':')?),
            _ => o.image = a.clone(),
        }
    }
    if o.image.is_empty() {
        return Err("usage: sharc-organ IMAGE [options]; see the module docs".to_string());
    }
    o.pokes.sort_by_key(|p| p.0);
    Ok(o)
}

fn run(args: &[String]) -> Result<String, String> {
    let mut o = parse_args(args)?;
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let mut core = boot_pno(&data, 0x121007, o.boot, o.mode1)?;
    if o.ranges.is_empty() {
        o.ranges.push((o.out_base, 4 * o.sets.len() as u32));
    }
    let (mut next, mut bytes, mut steps) = (0, Vec::new(), 0);
    for n in 0..o.calls {
        while next < o.pokes.len() && o.pokes[next].0 <= n {
            let (_, addr, value) = o.pokes[next];
            core.dm_write(addr, value)
                .map_err(|a| format!("bad poke 0x{a:x}"))?;
            next += 1;
        }
        steps = 0;
        if !o.no_bank {
            steps = call_once(&mut core, BANK.0, BANK.1, STEP_LIMIT)?;
        }
        for (k, s) in o.sets.iter().enumerate() {
            core.write_ureg(0x11, RECORD_BASE + 4 * s); // I1
            core.write_ureg(0x10, o.out_base + 4 * k as u32); // I0
            steps += call_once(&mut core, MIX.0, MIX.1, STEP_LIMIT)?;
        }
        if o.post {
            core.write_ureg(0x10, o.out_base);
            steps += call_once(&mut core, POST_ENTRY, POST_RET, STEP_LIMIT)?;
        }
        for &(base, count) in &o.ranges {
            for k in 0..count {
                let w = core.dm_read(base + k).unwrap_or(0);
                bytes.extend_from_slice(&w.to_le_bytes());
            }
        }
    }
    if let Some(path) = &o.out {
        std::fs::write(path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    }
    let mut text = format!("calls {}, steps per call {steps}\n", o.calls);
    for &(addr, count) in &o.dumps {
        let words: Vec<String> = (0..count)
            .map(|k| format!("{:x}", core.dm_read(addr + k).unwrap_or(0)))
            .collect();
        text.push_str(&format!("DM(0x{addr:x}..): {}\n", words.join(" ")));
    }
    Ok(text)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn options_parse() {
        let o = parse_args(&strings(&[
            "img", "--calls", "5", "--sets", "0,2", "--post", "--pre", "0x10=7", "--range",
            "0x20:4",
        ]))
        .unwrap();
        assert_eq!((o.calls, o.sets.clone(), o.post), (5, vec![0, 2], true));
        assert_eq!(o.pokes, vec![(0, 0x10, 7)]);
        assert_eq!(o.ranges, vec![(0x20, 4)]);
        assert!(parse_args(&strings(&["--calls", "1"])).is_err());
    }

    #[test]
    fn a_missing_image_is_an_error() {
        assert!(run(&strings(&["/nonexistent/image.bin"])).is_err());
    }
}
