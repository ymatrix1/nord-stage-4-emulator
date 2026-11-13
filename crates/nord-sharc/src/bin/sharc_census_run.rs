//! `sharc-census-run FIXTURE CASES` prints the model's output line for every case of the census
//! diff (see `core::census`); `docs/research/sharc-seq/census_diff.py` compares it with the simulator.
use nord_sharc::core::census::run_cases;
use std::process::ExitCode;

/// Reads both files and runs the cases.
fn run(fixture: &str, cases: &str) -> Result<String, String> {
    let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    run_cases(&read(fixture)?, &read(cases)?)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [fixture, cases] = args.as_slice() else {
        eprintln!("usage: sharc-census-run FIXTURE CASES");
        return ExitCode::from(2);
    };
    match run(fixture, cases) {
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

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let e = run("no-such-fixture.txt", "no-such-cases.txt").unwrap_err();
        assert!(e.starts_with("no-such-fixture.txt:"), "{e}");
    }
}
