//! [`LleEngine`]: ARM0 and the DSP chips on one virtual-time scheduler.
//!
//! Per ARM timer tick (the master clock, [`crate::clock`]): (1) ARM0 runs one tick, (2) the DSP
//! messages it emitted are routed to the chips ([`crate::route`]) and queued on their nodes, SMP
//! descriptors place the bound `.npno` ([`crate::smp_refill`]), (3) the passes owed by the clock
//! run on the [`AudioBus`] (tone/sample chips into the FX cores into the host stereo buffer).
//! Everything runs on the caller's thread in a fixed order: same inputs give the same bytes. The
//! engine is an offline oracle, it is slower than real time and allocates (`Vec`s) while it runs;
//! the real-time rules of `.agents/rules/dsp-and-realtime.md` apply to the future hybrid, not here.

use crate::arm::ArmHost;
use crate::clock::Clock;
use crate::input::{apply, Applied};
use crate::route::{Action, Router};
use crate::smp_refill::{bind_flash, SmpRefill};
use nord_arm::machine::system::Message;
use nord_core::lle_io::{LleEvent, LleInput, LleTelemetry, TelemetrySnapshot};
use nord_sharc::core::Core;
use nord_sharc::runner::audio_bus::{AudioBus, BusNode, Route, Stereo};
use nord_sharc::runner::live_flash;
use std::time::Instant;

/// Comm index of the SMP chip.
pub const SMP_COMM: u8 = 4;

/// Arithmetic of the DSP passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    /// Emulated 40-bit extended float, bit exact: the oracle (the only mode implemented).
    Exact,
    /// Native `f32` (`ns4-lle3`): NOT available yet, the engine runs Exact and says so.
    Fast,
}

/// One DSP core on the bus.
pub struct ChipNode {
    /// Comm index (FX 0, PNO 1, ORG 2, SY 3, SMP 4).
    pub comm: u8,
    /// Core 1 or 2, 0 for SY and SMP.
    pub core: u8,
    pub name: String,
    pub node: Box<dyn BusNode>,
}

/// Wall time and counters of a run (a measurement, not part of the deterministic output).
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub ticks: u64,
    pub passes: u64,
    pub messages: u64,
    pub arm_ns: u64,
    pub route_ns: u64,
    pub dsp_ns: u64,
    pub events_midi: u64,
    pub events_panel: u64,
    pub events_unmodelled: u64,
    /// SMP descriptors whose directory index could not be placed (skipped, not fatal).
    pub bad_descriptors: u64,
}

/// One message as the node received it, for replays (`lle_sharc_bus --msg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoggedMsg {
    pub node: usize,
    /// Pass of the node (its own pass counter) before which the message is delivered.
    pub pass: u64,
    pub word: u32,
    pub value: u32,
}

/// ARM0 + DSP chips.
pub struct LleEngine<A: ArmHost> {
    pub arm: A,
    pub bus: AudioBus,
    pub router: Router,
    pub clock: Clock,
    pub stats: Stats,
    names: Vec<String>,
    refill: Option<(usize, SmpRefill)>,
    buf: Vec<Message>,
    fault: Option<String>,
    requested: Precision,
    /// Every message posted to a node, when [`LleEngine::record_messages`] was called.
    pub msg_log: Option<Vec<LoggedMsg>>,
}

impl<A: ArmHost> LleEngine<A> {
    /// `chips` in bus node order, `routes` between nodes by index (`Dest::Host` = system output).
    pub fn new(
        arm: A,
        chips: Vec<ChipNode>,
        routes: Vec<Route>,
        precision: Precision,
    ) -> Result<Self, String> {
        let mut router = Router::default();
        let mut names = Vec::new();
        let mut nodes = Vec::new();
        for (i, c) in chips.into_iter().enumerate() {
            if router.owner(c.comm, c.core).is_some() {
                return Err(format!("two nodes own comm {} core {}", c.comm, c.core));
            }
            router.add(c.comm, c.core, i);
            names.push(c.name);
            nodes.push(c.node);
        }
        Ok(Self {
            arm,
            bus: AudioBus::new(nodes, routes)?,
            router,
            clock: Clock::default(),
            stats: Stats::default(),
            names,
            refill: None,
            buf: Vec::new(),
            fault: None,
            requested: precision,
            msg_log: None,
        })
    }

    /// The precision the passes really run in (Fast falls back to Exact, see [`Precision`]).
    pub fn effective_precision(&self) -> Precision {
        Precision::Exact
    }

    /// The precision the caller asked for.
    pub fn requested_precision(&self) -> Precision {
        self.requested
    }

    /// Starts keeping every routed message in [`LleEngine::msg_log`].
    pub fn record_messages(&mut self) {
        self.msg_log.get_or_insert_with(Vec::new);
    }

    pub fn node_names(&self) -> &[String] {
        &self.names
    }

    /// The first error that stopped the engine (sticky).
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
    }

    /// Binds a user `.npno` to the SMP chip: its core gets the flash image and the descriptor
    /// watcher runs on every SMP message (`ns4-pnoc`).
    pub fn bind_smp(&mut self, npno: Vec<u8>) -> Result<(), String> {
        if npno.len() as u64 > live_flash::MAX_BYTES {
            return Err(format!(
                "npno of {} bytes exceeds {}",
                npno.len(),
                live_flash::MAX_BYTES
            ));
        }
        let node = self
            .router
            .owner(SMP_COMM, 0)
            .ok_or("no SMP node to bind a .npno to")?;
        let watcher = SmpRefill::new(&npno)?;
        let core = self.bus.nodes_mut()[node]
            .core_mut()
            .ok_or("the SMP node has no core")?;
        bind_flash(core, npno);
        self.refill = Some((node, watcher));
        Ok(())
    }

    /// The SMP descriptor watcher, when a `.npno` is bound.
    pub fn smp_refill(&self) -> Option<&SmpRefill> {
        self.refill.as_ref().map(|(_, r)| r)
    }

    /// Runs one ARM tick and the DSP passes it owes.
    pub fn step(&mut self) -> Result<(), String> {
        if let Some(f) = &self.fault {
            return Err(f.clone());
        }
        let r = self.step_inner();
        if let Err(e) = &r {
            self.fault = Some(e.clone());
        }
        r
    }

    fn step_inner(&mut self) -> Result<(), String> {
        let t = Instant::now();
        self.arm.tick()?;
        self.buf.clear();
        self.arm.take_messages(&mut self.buf);
        self.stats.arm_ns += t.elapsed().as_nanos() as u64;

        let t = Instant::now();
        let msgs = std::mem::take(&mut self.buf);
        for m in &msgs {
            self.route_one(m)?;
        }
        self.stats.messages += msgs.len() as u64;
        self.buf = msgs;
        self.stats.route_ns += t.elapsed().as_nanos() as u64;

        let t = Instant::now();
        for _ in 0..self.clock.tick() {
            self.bus.run_tick()?;
            self.stats.passes += 1;
        }
        self.stats.dsp_ns += t.elapsed().as_nanos() as u64;
        self.stats.ticks += 1;
        Ok(())
    }

    fn route_one(&mut self, m: &Message) -> Result<(), String> {
        if let Action::Post { node, word, value } = self.router.route(m) {
            self.bus.post(node, word, value);
            if let Some(log) = &mut self.msg_log {
                let every = u64::from(self.bus.every(node).max(1));
                let pass = self.clock.passes.div_ceil(every);
                log.push(LoggedMsg {
                    node,
                    pass,
                    word,
                    value,
                });
            }
            let hit = match &mut self.refill {
                Some((n, r)) if *n == node && m.comm == SMP_COMM => r.observe(word, value),
                _ => None,
            };
            if let Some(rec11) = hit {
                let core = self.bus.nodes_mut()[node]
                    .core_mut()
                    .ok_or("SMP node has no core")?;
                // a descriptor is firmware data: a bad one is counted and skipped, not fatal
                if live_flash::place(core, rec11).is_err() {
                    self.stats.bad_descriptors += 1;
                }
            }
        }
        Ok(())
    }

    /// Runs `n` ARM ticks.
    pub fn run_ticks(&mut self, n: u64) -> Result<(), String> {
        (0..n).try_for_each(|_| self.step())
    }

    /// Runs until at least `frames` more host frames exist; returns how many were produced.
    /// Fails after a generous tick cap when no route reaches the host.
    pub fn render(&mut self, frames: usize) -> Result<usize, String> {
        let start = self.bus.host.left.len();
        let cap = frames as u64 * 8 + 4096;
        let mut ticks = 0u64;
        while self.bus.host.left.len() - start < frames {
            if ticks >= cap {
                let got = self.bus.host.left.len() - start;
                let e = format!("{got} host frames after {ticks} ticks: no route into the host?");
                self.fault = Some(e.clone());
                return Err(e);
            }
            self.step()?;
            ticks += 1;
        }
        Ok(self.bus.host.left.len() - start)
    }

    /// The host stereo output so far (DSP frame rate, `FRAME_RATE`, not resampled).
    pub fn host(&self) -> &Stereo {
        &self.bus.host
    }

    /// FNV-1a over the host buffers, the ARM state hash, the routing counters and, per DSP node
    /// with a core, its registers, step counter and data words `DM_HASH` (read with `dm_peek`, no
    /// watch or device-read side effect). Equal inputs give equal digests; wall times are not
    /// included.
    pub fn digest(&mut self) -> u64 {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        let mut mix = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01B3);
        for s in self.bus.host.left.iter().chain(&self.bus.host.right) {
            mix(u64::from(s.to_bits()));
        }
        mix(self.arm.state_hash());
        mix(self.stats.messages);
        mix(self.clock.passes);
        mix(self.router.skipped + self.router.counted);
        for n in self.bus.nodes_mut() {
            if let Some(c) = n.core_mut() {
                mix(core_digest(c));
            }
        }
        h
    }
}

/// Data words hashed per core: the SRAM data window `0x90000..0x100000` (`memory.rs::SRAM_WORDS`) holds the module state,
/// pass buffers and strips (`pno-full-pass.md`, `org-system-run.md`). ASSUMED: nothing the
/// passes write lives outside it (L2 and SDRAM are not hashed).
pub const DM_HASH: std::ops::Range<u32> = 0x9_0000..0x10_0000;

/// FNV-1a over a core's registers, counters and data window.
pub fn core_digest(c: &mut Core) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    let mut mix = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01B3);
    mix(u64::from(c.pc));
    mix(c.steps);
    for x in
        c.r.iter()
            .chain(&c.s)
            .chain(&c.i)
            .chain(&c.m)
            .chain(&c.l)
            .chain(&c.b)
    {
        mix(u64::from(*x));
    }
    mix(u64::from(c.mode1));
    mix(u64::from(c.mode2));
    for a in DM_HASH {
        mix(u64::from(c.dm_peek(a).unwrap_or(0)));
    }
    h
}

impl<A: ArmHost> LleInput for LleEngine<A> {
    fn handle_event(&mut self, event: LleEvent) {
        match apply(&mut self.arm, event) {
            Applied::Midi => self.stats.events_midi += 1,
            Applied::Panel => self.stats.events_panel += 1,
            Applied::Unmodelled => self.stats.events_unmodelled += 1,
        }
    }
}

impl<A: ArmHost> LleTelemetry for LleEngine<A> {
    /// Frame counter and ARM time only: the OLED framebuffers and LEDs are NOT filled (stub,
    /// `ns4-lle6`), they stay zero.
    fn fill_telemetry(&self, out: &mut TelemetrySnapshot) {
        out.frame = self.bus.host.left.len() as u32;
        out.arm_ms = (self.clock.ticks * 1000 / 24_000) as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::FakeArm;

    #[test]
    fn an_engine_without_chips_ticks_and_the_failure_is_sticky() {
        let mut e = LleEngine::new(FakeArm::default(), vec![], vec![], Precision::Fast).unwrap();
        e.run_ticks(24_000).unwrap();
        assert_eq!((e.stats.ticks, e.stats.passes), (24_000, 21_701));
        assert_eq!(e.requested_precision(), Precision::Fast);
        assert_eq!(e.effective_precision(), Precision::Exact);
        assert!(e.render(1).is_err(), "no route to the host");
        assert!(e.fault().is_some());
        assert!(e.step().is_err());
    }
}
