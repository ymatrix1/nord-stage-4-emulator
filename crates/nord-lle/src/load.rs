//! Images from a user-provided directory, read at run time (D20, D128: nothing is embedded).
//!
//! ```text
//! DIR/arm/bank0_CODE.bin  bank0_SRAM.bin  [eeprom.bin]     ARM0 (OS update, `target/fw_unpacked`)
//! DIR/FX/core1_sram.bin   core2_sram.bin                    FX Core1 / Core2   (presets fx1, fx2)
//! DIR/PNO/core1_sram.bin                                    PNO core 1         (preset pno)
//! DIR/PNO/core2_sram.bin                                    PNO core 2         (preset org)
//! DIR/ORG/core2_sram.bin                                    ORG core 2         (preset org)
//! DIR/SY/core1_sram.bin                                     SY1SY2             (preset sy)
//! DIR/SMP/core1_sram.bin                                    SMP                (preset smp)
//! ```
//! The flat SRAM images come from `tools/ns4_dsp_ldr.py IMG --sram DIR/<chip>`. A missing file is
//! not an error: the slot is left out and named in [`Images::missing`]. ASSUMED: the file names
//! of the SY and SMP slots (only the PNO, ORG Core2 names are documented in
//! `docs/research/org-system-run.md`).

use crate::engine::ChipNode;
use nord_sharc::runner::audio_bus::{Dest, Route};
use nord_sharc::runner::live_presets::{boot_core, preset_named};
use nord_sharc::runner::lle_system::wrap_node;
use nord_sharc::runner::pass_sched::Engine;
use nord_sharc::runner::periph_run::PeriphOpts;
use std::path::Path;

/// ORG core 1 is NOT a slot: under the `org` preset it does not return to the poll loop (2,000,000
/// steps at pass 0, run 2026-11-27), its start-up differs and is unknown.
/// `(comm, core, chip directory, file, preset, node name)` of every DSP slot.
pub const SLOTS: [(u8, u8, &str, &str, &str, &str); 7] = [
    (1, 1, "PNO", "core1_sram.bin", "pno", "pno1"),
    (1, 2, "PNO", "core2_sram.bin", "org", "pno2"),
    (2, 2, "ORG", "core2_sram.bin", "org", "org2"),
    (3, 0, "SY", "core1_sram.bin", "sy", "sy"),
    (4, 0, "SMP", "core1_sram.bin", "smp", "smp"),
    (0, 1, "FX", "core1_sram.bin", "fx1", "fx1"),
    (0, 2, "FX", "core2_sram.bin", "fx2", "fx2"),
];

/// The ARM0 firmware files.
pub struct ArmImages {
    pub code: Vec<u8>,
    pub sram: Vec<u8>,
    pub eeprom: Option<Vec<u8>>,
}

/// Everything read from a directory.
pub struct Images {
    pub arm: Option<ArmImages>,
    /// `(slot index into SLOTS, flat SRAM bytes)`.
    pub chips: Vec<(usize, Vec<u8>)>,
    /// Paths that were not there.
    pub missing: Vec<String>,
}

fn read_opt(p: &Path, missing: &mut Vec<String>) -> Option<Vec<u8>> {
    match std::fs::read(p) {
        Ok(b) => Some(b),
        Err(_) => {
            missing.push(p.display().to_string());
            None
        }
    }
}

/// Reads whatever of the layout exists under `dir`; fails only when `dir` is not a directory.
pub fn read_dir(dir: &Path) -> Result<Images, String> {
    if !dir.is_dir() {
        return Err(format!("{}: not a directory", dir.display()));
    }
    let mut missing = Vec::new();
    let a = dir.join("arm");
    let arm = match (
        read_opt(&a.join("bank0_CODE.bin"), &mut missing),
        read_opt(&a.join("bank0_SRAM.bin"), &mut missing),
    ) {
        (Some(code), Some(sram)) => Some(ArmImages {
            code,
            sram,
            eeprom: std::fs::read(a.join("eeprom.bin")).ok(),
        }),
        _ => None,
    };
    let mut chips = Vec::new();
    for (i, s) in SLOTS.iter().enumerate() {
        if let Some(b) = read_opt(&dir.join(s.2).join(s.3), &mut missing) {
            chips.push((i, b));
        }
    }
    Ok(Images {
        arm,
        chips,
        missing,
    })
}

/// Peripheral models a preset needs when the ARM drives it (`arm_dsp_live.py` flags: `--epdma` on
/// the tone chips, which would otherwise fault on `DM(0x3E021)`, `org-system-run.md` s1; the
/// external-port chain on SMP for the page refill, `smp-page-refill.md`). ASSUMED for FX and SY:
/// none needed (not checked against a live run).
pub fn periph_for(preset: &str) -> PeriphOpts {
    PeriphOpts {
        epdma: matches!(preset, "pno" | "org"),
        epchain: preset == "smp",
        ..PeriphOpts::default()
    }
}

/// Boots every loaded DSP image under its preset and wraps it as a bus node.
pub fn build_chips(images: &Images) -> Result<Vec<ChipNode>, String> {
    images
        .chips
        .iter()
        .map(|(i, data)| {
            let (comm, core, _, _, preset, name) = SLOTS[*i];
            let p = preset_named(preset).ok_or_else(|| format!("no preset {preset}"))?;
            let c = boot_core(data, &p, &periph_for(preset)).map_err(|e| format!("{name}: {e}"))?;
            let node = wrap_node(name, c, &p, vec![], Engine::Interp)?;
            Ok(ChipNode {
                comm,
                core,
                name: name.into(),
                node,
            })
        })
        .collect()
}

/// ASSUMED default audio wiring (nothing in the repository fixes it, `lle_system.rs`): every
/// source chip (`pno1 org2 sy smp`) into `fx1`, `fx1` into `fx2`, `fx2` to the host. Without both
/// FX cores the sources go straight to the host.
pub fn default_routes(chips: &[ChipNode]) -> Vec<Route> {
    let at = |n: &str| chips.iter().position(|c| c.name == n);
    let sources: Vec<usize> = ["pno1", "pno2", "org2", "sy", "smp"]
        .iter()
        .filter_map(|n| at(n))
        .collect();
    let route = |from, to, gain| Route { from, to, gain };
    match (at("fx1"), at("fx2")) {
        (Some(f1), Some(f2)) => sources
            .iter()
            .map(|&s| route(s, Dest::Node(f1), 1.0))
            .chain([route(f1, Dest::Node(f2), 1.0), route(f2, Dest::Host, 1.0)])
            .collect(),
        _ => sources.iter().map(|&s| route(s, Dest::Host, 1.0)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_sharc::runner::audio_bus::BusNode;

    struct Dummy;
    impl BusNode for Dummy {
        fn in_frames(&self) -> usize {
            0
        }
        fn run_pass(&mut self, _: &[[f32; 2]]) -> Result<Vec<[f32; 2]>, String> {
            Ok(vec![])
        }
    }

    fn chip(name: &str) -> ChipNode {
        ChipNode {
            comm: 9,
            core: 0,
            name: name.into(),
            node: Box::new(Dummy),
        }
    }

    #[test]
    fn a_missing_directory_is_an_error_and_missing_files_are_listed() {
        assert!(read_dir(Path::new("/definitely/not/here")).is_err());
        let dir = std::env::temp_dir().join(format!("nord_lle_load_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("PNO")).unwrap();
        std::fs::write(dir.join("PNO/core1_sram.bin"), [1u8, 2, 3]).unwrap();
        let im = read_dir(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(im.arm.is_none());
        assert_eq!(im.chips.len(), 1);
        assert_eq!(SLOTS[im.chips[0].0].5, "pno1");
        assert_eq!(
            im.missing.len(),
            8,
            "2 ARM files and 6 other chip files: {:?}",
            im.missing
        );
    }

    #[test]
    fn tone_chips_get_the_epdma_model_and_smp_the_chain() {
        assert!(periph_for("org").epdma && periph_for("pno").epdma);
        assert!(periph_for("smp").epchain && !periph_for("smp").epdma);
        assert_eq!(periph_for("fx1"), PeriphOpts::default());
    }

    #[test]
    fn default_routes_chain_through_both_fx_cores_or_go_to_the_host() {
        let all: Vec<ChipNode> = ["pno1", "org2", "fx1", "fx2"].map(chip).into();
        let r = default_routes(&all);
        assert_eq!(r.len(), 4);
        assert_eq!((r[2].from, r[2].to), (2, Dest::Node(3)));
        assert_eq!((r[3].from, r[3].to), (3, Dest::Host));
        let no_fx: Vec<ChipNode> = ["pno1", "sy"].map(chip).into();
        let r = default_routes(&no_fx);
        assert!(r.iter().all(|x| x.to == Dest::Host) && r.len() == 2);
        assert!(default_routes(&[]).is_empty());
    }
}
