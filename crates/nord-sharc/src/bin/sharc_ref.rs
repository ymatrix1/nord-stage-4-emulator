//! `sharc-ref IMAGE SCENARIO [--out PREFIX] [--wav] [--lenient] [--regs]`: oracle reference render.
//!
//! Loads a flat SRAM image (`python tools/ns4_dsp_raw.py IMG --sram DIR`), executes a scenario
//! file (data pokes, test-signal fills, voice records, handler runs, captures; format in
//! `script.rs` and `docs/research/oracle-vs-engine.md`) on the SHARC core and writes the captured
//! series to `PREFIX.csv` (with a metadata header) and, with `--wav`, `PREFIX.wav`. Without
//! `--out` the CSV goes to standard output. `tools/ns4_oracle_compare.py` compares the dump with
//! a reference render of the Rust engine.
#[path = "sharc_ref/exec.rs"]
mod exec;
#[path = "sharc_ref/out.rs"]
mod out;
#[path = "sharc_ref/script.rs"]
mod script;
#[path = "sharc_ref/signal.rs"]
mod signal;
#[path = "sharc_ref/voice.rs"]
mod voice;

use std::process::ExitCode;

fn run(args: &[String]) -> Result<String, String> {
    let mut positional = Vec::new();
    let mut prefix: Option<String> = None;
    let mut wav = false;
    let mut lenient = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => prefix = Some(it.next().ok_or("--out needs PREFIX")?.clone()),
            "--wav" => wav = true,
            "--lenient" => lenient = true,
            _ => positional.push(a.clone()),
        }
    }
    let [image, scenario] = positional.as_slice() else {
        return Err("usage: sharc-ref IMAGE SCENARIO [--out PREFIX] [--wav] [--lenient]".into());
    };
    let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
    let text = std::fs::read_to_string(scenario).map_err(|e| format!("{scenario}: {e}"))?;
    let cmds = script::parse_script(&text)?;
    let mut runner = exec::Runner::new(&data, lenient);
    runner.exec(&cmds)?;
    let render = &runner.render;
    let extra = vec![
        ("tool".to_string(), "sharc-ref".to_string()),
        ("scenario".to_string(), scenario.clone()),
        ("runs".to_string(), render.run_steps.len().to_string()),
        (
            "steps_per_run_max".to_string(),
            render.run_steps.iter().max().unwrap_or(&0).to_string(),
        ),
        ("core_steps".to_string(), runner.core.steps.to_string()),
    ];
    let csv = out::to_csv(render, &extra);
    let rate = render
        .meta
        .iter()
        .find(|m| m.0 == "rate")
        .and_then(|m| m.1.parse().ok())
        .unwrap_or(48_000);
    match prefix {
        None => Ok(csv),
        Some(p) => {
            std::fs::write(format!("{p}.csv"), &csv).map_err(|e| e.to_string())?;
            let mut msg = format!("wrote {p}.csv ({} series)", render.series.len());
            if wav {
                std::fs::write(format!("{p}.wav"), out::to_wav(render, rate)?)
                    .map_err(|e| e.to_string())?;
                msg.push_str(&format!(", {p}.wav"));
            }
            Ok(msg)
        }
    }
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

    /// Every committed scenario parses (loops included) and uses only known commands.
    #[test]
    fn committed_scenarios_parse() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/oracle_scenarios");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "scn") {
                let text = std::fs::read_to_string(&path).unwrap();
                let cmds = script::parse_script(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                assert!(!cmds.is_empty(), "{path:?}");
                seen += 1;
            }
        }
        assert!(seen >= 4, "expected the Stage B scenarios, found {seen}");
    }

    #[test]
    fn writes_csv_and_wav_for_a_poke_scenario() {
        let dir = std::env::temp_dir().join("nord_sharc_ref_test");
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("img.bin");
        std::fs::write(&image, vec![0u8; 64]).unwrap();
        let scen = dir.join("s.txt");
        std::fs::write(
            &scen,
            "meta rate 44100\npoke 0xA0000 f0.5\npoke 0xA0001 f-0.25\ncapture L 0xA0000 1\ncapture R 0xA0001 1\n",
        )
        .unwrap();
        let (i, s) = (
            image.to_string_lossy().to_string(),
            scen.to_string_lossy().to_string(),
        );
        let csv = run(&[i.clone(), s.clone()]).unwrap();
        assert!(csv.contains("index,L,R\n0,5e-1,-2.5e-1"), "{csv}");
        let prefix = dir.join("o").to_string_lossy().to_string();
        let msg = run(&[i.clone(), s, "--out".into(), prefix.clone(), "--wav".into()]).unwrap();
        assert!(msg.contains("o.wav"), "{msg}");
        let wav = std::fs::read(format!("{prefix}.wav")).unwrap();
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 44100);
        assert!(run(&[i]).is_err());
        assert!(run(&["x".into(), "y".into()]).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
