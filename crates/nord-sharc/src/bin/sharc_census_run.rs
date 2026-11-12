//! `sharc-census-run FIXTURE CASES` prints the model's output line for every case of the census
//! diff (see `core::census`); `docs/research/sharc-seq/census_diff.py` compares it with the simulator.
use nord_sharc::core::census::run_cases;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [fixture, cases] = args.as_slice() else {
        eprintln!("usage: sharc-census-run FIXTURE CASES");
        return ExitCode::from(2);
    };
    let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    match read(fixture).and_then(|f| read(cases).and_then(|c| run_cases(&f, &c))) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
