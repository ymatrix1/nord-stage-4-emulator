//! A SHARC core as a node of the [`AudioBus`](super::audio_bus::AudioBus): the pass runs through
//! [`PassJob`] (messages, ping-pong, FX hand-over, engine), the input frames of the bus are written
//! into the core's input block before the pass and the output frames are read after it.
//!
//! Sources need no input slot (the ORG/PNO lane, the SY/SMP ring half or any DM block is read with
//! an [`OutTap`]); the FX cores take their frames at `FxCore::input_base` and put the master
//! words out at `FxCore::out`. Provenance: the FX addresses are VERIFIED from the listings
//! (`fx_run.rs`); that a tone chip's frames land in exactly that block is ASSUMED (the audio DMA
//! destination was never captured), so the wiring is data of the caller, not hard-coded here.
use super::audio_bus::{BusNode, Frame};
use super::fx_run::{FxCore, OutLoc};
use super::msg::Msg;
use super::pass_sched::PassJob;
use super::periph_run::OUT_POINTERS;
use crate::core::Core;

/// Where the frames a pass produced are read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutTap {
    /// `frames` stereo frames `[L0 R0 L1 R1 ..]` at `base`, each word times `sign`.
    Block { base: u32, frames: u32, sign: f32 },
    /// The same behind the pointer held in data word `cell` (FX Core2 `DM(0xC03D7)`).
    Pointer { cell: u32, frames: u32, sign: f32 },
    /// Output pointer set `set` (0..5) pair `pair` (0..2) of the pass buffers `DM(0xB0159 + set)`:
    /// 2 frames per pass (VERIFIED, `pno-full-pass.md` s1).
    Lane { set: u32, pair: u32 },
}

impl OutTap {
    /// The tap of an FX core's master words (4 frames).
    pub fn of_fx(fx: &FxCore) -> Self {
        match fx.out {
            OutLoc::Fixed(base) => Self::Block {
                base,
                frames: 4,
                sign: fx.out_sign,
            },
            OutLoc::Pointer(cell) => Self::Pointer {
                cell,
                frames: 4,
                sign: fx.out_sign,
            },
        }
    }

    /// Reads the frames from `core` (a word that cannot be read is silence).
    pub fn read(&self, core: &mut Core) -> Vec<Frame> {
        let (base, frames, sign) = match *self {
            Self::Block { base, frames, sign } => (base, frames, sign),
            Self::Pointer { cell, frames, sign } => (core.dm_read(cell).unwrap_or(0), frames, sign),
            Self::Lane { set, pair } => (
                core.dm_read(OUT_POINTERS + set)
                    .unwrap_or(0)
                    .wrapping_add(4 * pair),
                2,
                1.0,
            ),
        };
        (0..frames)
            .map(|k| {
                let mut w = |o: u32| {
                    sign * f32::from_bits(core.dm_read(base.wrapping_add(2 * k + o)).unwrap_or(0))
                };
                [w(0), w(1)]
            })
            .collect()
    }
}

/// Writes `frames` as `[L0 R0 L1 R1 ..]` at `base`.
pub fn write_frames(core: &mut Core, base: u32, frames: &[Frame]) {
    for (k, f) in frames.iter().enumerate() {
        let _ = core.dm_write(base + 2 * k as u32, f[0].to_bits());
        let _ = core.dm_write(base + 2 * k as u32 + 1, f[1].to_bits());
    }
}

/// One core on the bus.
pub struct CoreNode {
    pub job: PassJob,
    /// `(input block address, frames per pass)`, none for a source.
    pub input: Option<(u32, usize)>,
    pub out: OutTap,
    /// A pass runs on every `every`-th tick.
    pub every: u32,
    /// Passes run so far.
    pub pass: u32,
    next_msg: usize,
}

impl CoreNode {
    pub fn new(mut job: PassJob, input: Option<(u32, usize)>, out: OutTap, every: u32) -> Self {
        job.msgs.sort_by_key(|m| m.pass);
        Self {
            job,
            input,
            out,
            every,
            pass: 0,
            next_msg: 0,
        }
    }
}

impl BusNode for CoreNode {
    fn in_frames(&self) -> usize {
        self.input.map_or(0, |(_, n)| n)
    }

    fn every(&self) -> u32 {
        self.every
    }

    fn core_mut(&mut self) -> Option<&mut Core> {
        Some(&mut self.job.core)
    }

    fn run_pass(&mut self, input: &[Frame]) -> Result<Vec<Frame>, String> {
        let p = self.pass;
        self.job
            .before_pass(p, &mut self.next_msg)
            .map_err(|e| format!("{}: {e}", self.job.name))?;
        if let Some((base, _)) = self.input {
            write_frames(&mut self.job.core, base, input);
        }
        let (steps, idle) = self.job.run_pass();
        if !idle {
            return Err(format!(
                "{}: pass {p} did not return to the poll loop ({steps} steps)",
                self.job.name
            ));
        }
        self.pass += 1;
        Ok(self.out.read(&mut self.job.core))
    }

    fn post(&mut self, word1: u32, word2: u32) -> bool {
        if self.next_msg == self.job.msgs.len() {
            self.job.msgs.clear();
            self.next_msg = 0;
        }
        let pass = self.pass;
        self.job.msgs.push(Msg { pass, word1, word2 });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::audio_bus::{AudioBus, Dest, Route};
    use super::super::fx_run::{FxDrive, FX2, PING_VALUE};
    use super::super::pass::tests::clearing_core;
    use super::super::pass::PassConfig;
    use super::super::pass_sched::Engine;
    use super::*;
    use crate::core::sdram::Sdram;
    use crate::core::support::core_with;

    /// A core whose pass is `DM(I0,M0) = 0` then `JUMP 0x120000`, `I0` = the mailbox `trigger`:
    /// the pass returns to its idle pc (the jump) as soon as the store cleared the word the start
    /// wrote, on every pass.
    fn job(name: &str, trigger: u32) -> PassJob {
        let mut core = core_with(&[0x9004, 0x0000, 0x06be, 0x0012, 0x0000]);
        core.i[0] = trigger;
        PassJob {
            name: name.into(),
            core,
            cfg: PassConfig {
                trigger,
                idle_pc: 0x12_0002,
                max_steps: 100,
                irq: None,
            },
            msgs: Vec::new(),
            msg_base: 0,
            ping_pong: None,
            fx: None,
            engine: Engine::Interp,
        }
    }

    #[test]
    fn a_posted_message_is_delivered_before_the_next_pass_and_the_queue_is_recycled() {
        let mut n = CoreNode::new(
            job("n", 0xB_00E1),
            None,
            OutTap::Lane { set: 0, pair: 0 },
            1,
        );
        assert!(n.post(0x100, 7));
        n.run_pass(&[]).unwrap();
        assert_eq!(n.core_mut().unwrap().dm_read(0x100), Ok(7));
        assert!(n.post(0x101, 8));
        assert_eq!(n.job.msgs.len(), 1, "the delivered message was dropped");
        n.run_pass(&[]).unwrap();
        assert_eq!(n.core_mut().unwrap().dm_read(0x101), Ok(8));
    }

    #[test]
    fn out_taps_read_blocks_pointers_and_lanes() {
        let mut c = clearing_core();
        write_frames(&mut c, 0xB_0300, &[[1.0, 2.0], [3.0, 4.0]]);
        let blk = OutTap::Block {
            base: 0xB_0300,
            frames: 2,
            sign: -1.0,
        };
        assert_eq!(blk.read(&mut c), vec![[-1.0, -2.0], [-3.0, -4.0]]);
        c.dm_write(0xC_03D7, 0xB_0300).unwrap();
        assert_eq!(OutTap::of_fx(&FX2).read(&mut c)[1], [3.0, 4.0]);
        // lane: pointer set 1, pair 1 = words base + 4..
        c.dm_write(OUT_POINTERS + 1, 0xB_02FE).unwrap();
        write_frames(&mut c, 0xB_0302, &[[7.0, 8.0], [9.0, 10.0]]);
        let lane = OutTap::Lane { set: 1, pair: 1 };
        assert_eq!(lane.read(&mut c), vec![[7.0, 8.0], [9.0, 10.0]]);
    }

    /// Source core (frames pre-written in its pass buffer) -> FX-like sink core with an input
    /// block and an FX hand-over drive -> host, through the bus and one pass of latency; the
    /// sink's delay "line" is the SDRAM window: its mailbox is an SDRAM word, so the pass itself
    /// reads and writes the window (`DM(I0,M0) = 0` on `0x0802_0000`).
    #[test]
    fn two_cores_pass_frames_through_the_bus_to_the_host_and_use_the_sdram_window() {
        let mut src = job("src", 0xB_00E2);
        write_frames(&mut src.core, 0xB_0300, &[[0.25, -0.25], [0.5, -0.5]]);
        let source = CoreNode::new(
            src,
            None,
            OutTap::Block {
                base: 0xB_0300,
                frames: 2,
                sign: 1.0,
            },
            1,
        );
        let mut snk = job("fx", 0x0802_0000);
        assert!(Sdram::contains(snk.cfg.trigger));
        snk.fx = Some(FxDrive::new(FX2));
        let sink = CoreNode::new(
            snk,
            Some((0xB_0400, 2)),
            OutTap::Block {
                base: 0xB_0400,
                frames: 2,
                sign: 1.0,
            },
            1,
        );
        let routes = vec![
            Route {
                from: 0,
                to: Dest::Node(1),
                gain: 1.0,
            },
            Route {
                from: 1,
                to: Dest::Host,
                gain: 1.0,
            },
        ];
        let mut bus = AudioBus::new(vec![Box::new(source), Box::new(sink)], routes).unwrap();
        bus.run(3).unwrap();
        // tick 0 the sink sees silence (underrun), ticks 1 and 2 the source frames
        assert_eq!(bus.underruns, vec![0, 2]);
        assert_eq!(bus.host.left, vec![0.0, 0.0, 0.25, 0.5, 0.25, 0.5]);
        assert_eq!(bus.host.right, vec![0.0, 0.0, -0.25, -0.5, -0.25, -0.5]);
        // the sink's pass went through the window and the FX drive ran before each pass
        let core = bus.nodes_mut()[1].core_mut().unwrap();
        assert_eq!(
            core.sdram_writes, 6,
            "3 passes x (start write + clearing store)"
        );
        assert!(core.sdram_reads >= 3, "the idle check reads the window");
        assert_eq!(core.device_writes, 0);
        assert_eq!(core.dm_read(FX2.ping_select), Ok(PING_VALUE));
        assert_eq!(
            core.dm_read(0xB_0400),
            Ok(0.25f32.to_bits()),
            "last input block"
        );
    }

    #[test]
    fn a_core_that_loses_its_poll_loop_ends_the_run_with_a_named_error() {
        let mut j = job("stuck", 0xB_00E2);
        j.cfg.idle_pc = 0x12_0100; // never reached in 100 steps
        let n = CoreNode::new(
            j,
            None,
            OutTap::Block {
                base: 0xB_0300,
                frames: 1,
                sign: 1.0,
            },
            1,
        );
        let mut bus = AudioBus::new(vec![Box::new(n)], vec![]).unwrap();
        let e = bus.run_tick().unwrap_err();
        assert!(e.contains("stuck: pass 0 did not return"), "{e}");
    }
}
