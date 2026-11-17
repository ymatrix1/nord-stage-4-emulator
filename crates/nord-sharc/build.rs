//! Build script: embeds the optional generated AOT block module.
//!
//! `NORD_SHARC_AOT_HOT=<file.rs>` (an absolute path to the output of `sharc-aot --hot` or
//! `--ranges`) is copied to `$OUT_DIR/aot_hot_gen.rs` and compiled into `core::aot_hot`. Without
//! it a stub with no blocks is written, so ordinary builds (CI, `--all-features`) never need
//! firmware.
use std::{env, fs, path::PathBuf};

const STUB: &str = "use crate::core::aot_hot::AotBlock;\n\
pub(super) fn blocks() -> &'static [AotBlock] {\n    &[]\n}\n";

fn main() {
    println!("cargo:rerun-if-env-changed=NORD_SHARC_AOT_HOT");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("aot_hot_gen.rs");
    match env::var("NORD_SHARC_AOT_HOT") {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            fs::copy(&path, &out).unwrap_or_else(|e| panic!("NORD_SHARC_AOT_HOT={path}: {e}"));
        }
        _ => fs::write(&out, STUB).expect("write stub"),
    }
}
