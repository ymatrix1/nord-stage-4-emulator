//! The ARM0 side of the engine behind a trait, so the engine runs against the real firmware
//! machine ([`Arm0`]) or a scripted fake (unit tests, no firmware).

use nord_arm::machine::idle::IdleStats;
use nord_arm::machine::system::Message;
use nord_arm::machine::{Machine, Stop};
use nord_core::lle_io::PanelEvent;

/// ASSUMED: ARM instructions per 24 kHz timer tick, the harness convention of
/// `tools/arm_steady.py` (`docs/research/arm0-rust-runtime.md` s5: bound 300 k to 456 k per ms,
/// not measured on hardware). Unchanged from `arm-lle-bench`.
pub const TICK_INSNS: u64 = 200_000;
/// ASSUMED: instruction budget of the boot to the main loop (`arm-lle-bench` uses 1.5 G; the OS
/// v1.66 boot needs about 232 M).
pub const BOOT_BUDGET: u64 = 1_500_000_000;

/// What the engine needs from ARM0.
pub trait ArmHost {
    /// Runs one timer tick of virtual time.
    fn tick(&mut self) -> Result<(), String>;
    /// Appends the DSP messages completed since the last call to `out`.
    fn take_messages(&mut self, out: &mut Vec<Message>);
    /// Queues raw MIDI bytes on the ARM UART.
    fn midi(&mut self, bytes: &[u8]);
    /// Applies a panel edit; `false` when it is not modelled.
    fn panel(&mut self, _e: PanelEvent) -> bool {
        false
    }
    /// Hash of the architectural state (determinism checks); 0 for a double.
    fn state_hash(&self) -> u64 {
        0
    }
    /// Instructions retired so far (0 for a double).
    fn insns(&self) -> u64 {
        0
    }
}

impl<T: ArmHost + ?Sized> ArmHost for Box<T> {
    fn tick(&mut self) -> Result<(), String> {
        (**self).tick()
    }
    fn take_messages(&mut self, out: &mut Vec<Message>) {
        (**self).take_messages(out);
    }
    fn midi(&mut self, bytes: &[u8]) {
        (**self).midi(bytes);
    }
    fn panel(&mut self, e: PanelEvent) -> bool {
        (**self).panel(e)
    }
    fn state_hash(&self) -> u64 {
        (**self).state_hash()
    }
    fn insns(&self) -> u64 {
        (**self).insns()
    }
}

/// The real ARM0: [`Machine`] with idle skipping and the native DSP return source.
pub struct Arm0 {
    pub machine: Machine,
    pub idle: IdleStats,
    pub tick_insns: u64,
    /// Instructions the boot executed (virtual).
    pub boot_insns: u64,
}

impl Arm0 {
    /// Boots `code` (`bank0_CODE.bin`) and `sram` (`bank0_SRAM.bin`) to the main loop; `eeprom`
    /// is the 256 KB I2C image (program store), blank when `None`. Installs the DSP return
    /// layers and the real timer (the `arm-lle-bench` sequence).
    pub fn boot(code: &[u8], sram: &[u8], eeprom: Option<&[u8]>) -> Result<Self, String> {
        let mut m = Machine::new();
        m.prepare_system(code, sram)
            .map_err(|s| format!("ARM0 init array stopped: {s:?}"))?;
        if let (Some(e), Some(i2c)) = (eeprom, m.bus.i2c.as_mut()) {
            i2c.load_image(e);
        }
        let mut idle = IdleStats::default();
        let (n, _, stop) = m.boot_system(BOOT_BUDGET, &mut idle);
        if stop != Stop::Until && stop != Stop::StopPc {
            return Err(format!(
                "ARM0 boot ended with {stop:?} at pc {:#x} after {n} instructions",
                m.cpu.pc()
            ));
        }
        Ok(Self::from_booted(m, n))
    }

    /// Wraps a machine that already booted (also used by tests on tiny programs).
    pub fn from_booted(mut m: Machine, boot_insns: u64) -> Self {
        m.install_dsp_ports();
        m.install_dsp_return();
        m.enable_real_timer();
        Self {
            machine: m,
            idle: IdleStats::default(),
            tick_insns: TICK_INSNS,
            boot_insns,
        }
    }
}

impl ArmHost for Arm0 {
    fn tick(&mut self) -> Result<(), String> {
        match self.machine.run_tick_idle(self.tick_insns, &mut self.idle) {
            Stop::Budget => Ok(()),
            s => Err(format!(
                "ARM0 stopped: {s:?} at pc {:#x}",
                self.machine.cpu.pc()
            )),
        }
    }

    fn take_messages(&mut self, out: &mut Vec<Message>) {
        self.machine.drain_messages_into(out);
    }

    fn midi(&mut self, bytes: &[u8]) {
        self.machine.send_midi(bytes);
    }

    fn state_hash(&self) -> u64 {
        self.machine.state_hash()
    }

    fn insns(&self) -> u64 {
        self.machine.cpu.insns
    }
}

/// Scripted ARM for tests: emits the listed messages on the listed ticks.
#[derive(Default)]
pub struct FakeArm {
    /// `(tick index, message)`, tick 0 is the first call of [`ArmHost::tick`].
    pub script: Vec<(u64, Message)>,
    pub ticks: u64,
    pub midi_seen: Vec<Vec<u8>>,
    pub pending: Vec<Message>,
    /// Tick at which `tick` fails (fault injection).
    pub fail_at: Option<u64>,
}

impl ArmHost for FakeArm {
    fn tick(&mut self) -> Result<(), String> {
        if self.fail_at == Some(self.ticks) {
            return Err(format!("fake ARM fault at tick {}", self.ticks));
        }
        let t = self.ticks;
        self.pending
            .extend(self.script.iter().filter(|(k, _)| *k == t).map(|(_, m)| *m));
        self.ticks += 1;
        Ok(())
    }

    fn take_messages(&mut self, out: &mut Vec<Message>) {
        out.append(&mut self.pending);
    }

    fn midi(&mut self, bytes: &[u8]) {
        self.midi_seen.push(bytes.to_vec());
    }

    fn insns(&self) -> u64 {
        self.ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boxed_host_forwards_every_call() {
        let mut b: Box<dyn ArmHost> = Box::new(FakeArm::default());
        b.tick().unwrap();
        b.midi(&[1]);
        assert!(!b.panel(PanelEvent::Key {
            id: 1,
            pressed: true
        }));
        assert_eq!((b.insns(), b.state_hash()), (1, 0));
        let mut out = Vec::new();
        b.take_messages(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn fake_arm_emits_scripted_messages_on_their_tick() {
        let m = Message {
            path: 1,
            comm: 3,
            word1: 5,
            word2: 6,
            pc: 0,
        };
        let mut a = FakeArm {
            script: vec![(1, m)],
            ..Default::default()
        };
        let mut out = Vec::new();
        a.tick().unwrap();
        a.take_messages(&mut out);
        assert!(out.is_empty());
        a.tick().unwrap();
        a.take_messages(&mut out);
        assert_eq!(out, vec![m]);
        a.fail_at = Some(2);
        assert!(a.tick().is_err());
    }

    #[test]
    fn arm0_ticks_a_tiny_program_and_reports_a_stop() {
        // `b .` at 0xC0000000: a pure spin, the whole tick budget is skipped
        let mut m = Machine::new();
        m.bus.mem.write32(0xC000_0000, 0xEAFF_FFFE);
        m.cpu.r[15] = 0xC000_0000;
        let mut a = Arm0::from_booted(m, 0);
        a.tick_insns = 10_000;
        a.tick().unwrap();
        a.tick().unwrap();
        assert!(a.insns() >= 20_000);
        let mut out = Vec::new();
        a.take_messages(&mut out);
        assert!(out.is_empty());
        // an undefined instruction ends the run with a named fault
        let mut m = Machine::new();
        m.bus.mem.write32(0xC000_0000, 0xE7F0_00F0);
        m.cpu.r[15] = 0xC000_0000;
        let mut a = Arm0::from_booted(m, 0);
        assert!(a.tick().unwrap_err().contains("ARM0 stopped"));
    }
}
