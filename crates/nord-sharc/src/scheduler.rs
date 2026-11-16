//! Cycle-synchronized multi-core SHARC scheduler running one dedicated OS worker thread per
//! [`Core`].
//!
//! Each worker thread owns its [`Core`] exclusively so execution within a cycle quantum is
//! completely lock-free across host CPU cores. Between quanta, the coordinator can exchange
//! inter-core / ARM mailbox words (`poke` / `peek`) and synchronize cycle counts.

use crate::core::{Core, Fault};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

enum WorkerCmd {
    Run { cycles: u64, use_jit: bool },
    Poke { addr: u32, value: u32 },
    Peek { addr: u32 },
    Cycles,
    Shutdown,
}

enum WorkerResp {
    Run(Option<Fault>),
    Poke(Result<(), u32>),
    Peek(Result<u32, u32>),
    Cycles(u64),
}

struct WorkerHandle {
    tx: SyncSender<WorkerCmd>,
    rx: Receiver<WorkerResp>,
    join: Option<JoinHandle<()>>,
}

/// Lockstep multi-core scheduler running one dedicated thread per [`Core`].
pub struct MultiCoreScheduler {
    workers: Vec<WorkerHandle>,
}

impl MultiCoreScheduler {
    /// Spawns one dedicated worker thread per [`Core`] in `cores`.
    pub fn new(cores: Vec<Core>) -> Self {
        let workers = cores
            .into_iter()
            .enumerate()
            .map(|(idx, mut core)| {
                let (cmd_tx, cmd_rx) = mpsc::sync_channel::<WorkerCmd>(1);
                let (resp_tx, resp_rx) = mpsc::sync_channel::<WorkerResp>(1);
                let join = thread::Builder::new()
                    .name(format!("sharc-core-{idx}"))
                    .spawn(move || {
                        while let Ok(cmd) = cmd_rx.recv() {
                            match cmd {
                                WorkerCmd::Run { cycles, use_jit } => {
                                    let res = core.run_for_cycles(cycles, use_jit);
                                    if resp_tx.send(WorkerResp::Run(res)).is_err() {
                                        break;
                                    }
                                }
                                WorkerCmd::Poke { addr, value } => {
                                    let res = core.dm_write(addr, value);
                                    if resp_tx.send(WorkerResp::Poke(res)).is_err() {
                                        break;
                                    }
                                }
                                WorkerCmd::Peek { addr } => {
                                    let res = core.dm_read(addr);
                                    if resp_tx.send(WorkerResp::Peek(res)).is_err() {
                                        break;
                                    }
                                }
                                WorkerCmd::Cycles => {
                                    if resp_tx.send(WorkerResp::Cycles(core.cycles())).is_err() {
                                        break;
                                    }
                                }
                                WorkerCmd::Shutdown => break,
                            }
                        }
                    })
                    .expect("spawn sharc worker thread");
                WorkerHandle {
                    tx: cmd_tx,
                    rx: resp_rx,
                    join: Some(join),
                }
            })
            .collect();
        Self { workers }
    }

    /// Number of managed SHARC cores.
    pub fn len(&self) -> usize {
        self.workers.len()
    }

    /// Returns `true` if the scheduler manages zero cores.
    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    /// Runs all cores in parallel for `cycles` cycles, waiting at the barrier until every core
    /// completes the quantum. Returns `Err((core_idx, fault))` on the first faulting core.
    pub fn run_quantum(&mut self, cycles: u64, use_jit: bool) -> Result<(), (usize, Fault)> {
        for w in &self.workers {
            w.tx.send(WorkerCmd::Run { cycles, use_jit })
                .expect("worker alive");
        }
        let mut first_fault = None;
        for (idx, w) in self.workers.iter().enumerate() {
            if let Ok(WorkerResp::Run(Some(f))) = w.rx.recv() {
                if first_fault.is_none() {
                    first_fault = Some((idx, f));
                }
            }
        }
        match first_fault {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Runs one audio block of `frames` samples at `sample_rate_hz` with each core clocked at
    /// `core_hz` (`cycles = frames * core_hz / sample_rate_hz`).
    pub fn run_audio_block(
        &mut self,
        frames: u32,
        sample_rate_hz: u32,
        core_hz: u32,
        use_jit: bool,
    ) -> Result<u64, (usize, Fault)> {
        let cycles = (u64::from(frames) * u64::from(core_hz)) / u64::from(sample_rate_hz);
        self.run_quantum(cycles, use_jit)?;
        Ok(cycles)
    }

    /// Writes `value` to data-memory `addr` on core `core_idx` (e.g. inter-core / ARM mailbox).
    pub fn poke(&self, core_idx: usize, addr: u32, value: u32) -> Result<(), u32> {
        let w = &self.workers[core_idx];
        w.tx.send(WorkerCmd::Poke { addr, value })
            .expect("worker alive");
        match w.rx.recv().expect("worker reply") {
            WorkerResp::Poke(r) => r,
            _ => unreachable!(),
        }
    }

    /// Reads data-memory `addr` from core `core_idx`.
    pub fn peek(&self, core_idx: usize, addr: u32) -> Result<u32, u32> {
        let w = &self.workers[core_idx];
        w.tx.send(WorkerCmd::Peek { addr }).expect("worker alive");
        match w.rx.recv().expect("worker reply") {
            WorkerResp::Peek(r) => r,
            _ => unreachable!(),
        }
    }

    /// Returns the elapsed cycle count of each managed core.
    pub fn cycles(&self) -> Vec<u64> {
        for w in &self.workers {
            w.tx.send(WorkerCmd::Cycles).expect("worker alive");
        }
        self.workers
            .iter()
            .map(|w| match w.rx.recv().expect("worker reply") {
                WorkerResp::Cycles(c) => c,
                _ => unreachable!(),
            })
            .collect()
    }
}

impl Drop for MultiCoreScheduler {
    fn drop(&mut self) {
        for w in &mut self.workers {
            let _ = w.tx.send(WorkerCmd::Shutdown);
        }
        for w in &mut self.workers {
            if let Some(j) = w.join.take() {
                let _ = j.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with_program(program: &[u16]) -> Core {
        let mut c = Core::new(&[]);
        for (idx, chunk) in program.chunks(2).enumerate() {
            let lo = u32::from(chunk[0]);
            let hi = u32::from(chunk.get(1).copied().unwrap_or(0));
            c.dm_write(0x9_0000 + idx as u32, lo | (hi << 16)).unwrap();
        }
        c.pc = 0x12_0000;
        c
    }

    #[test]
    fn four_cores_run_in_lockstep_and_exchange_mailboxes() {
        // Each core runs: DO (+3) UNTIL LCE, LCNTR=R5; R1 = R1 + R2
        let prog = [0x0d05u16, 0x0000, 0x0003, 0x013e, 0x0000, 0x1112];
        let mut cores = Vec::new();
        let mut ref_cores = Vec::new();
        for idx in 0..4u32 {
            let mut c = core_with_program(&prog);
            c.r[5] = 40;
            c.r[2] = idx + 1;
            cores.push(c);

            let mut r = core_with_program(&prog);
            r.r[5] = 40;
            r.r[2] = idx + 1;
            ref_cores.push(r);
        }

        let mut sched = MultiCoreScheduler::new(cores);
        assert_eq!(sched.len(), 4);
        assert!(!sched.is_empty());

        for q in 0..4u32 {
            // Run 10 cycles per quantum on all 4 cores in parallel.
            assert_eq!(sched.run_quantum(10, true), Ok(()));
            for r in &mut ref_cores {
                assert_eq!(r.run(10), None);
            }
            // Exchange an ARM/inter-core mailbox word between quanta.
            sched.poke(q as usize, 0x9_0500, 0xA000 + q).unwrap();
            assert_eq!(sched.peek(q as usize, 0x9_0500), Ok(0xA000 + q));
        }

        assert_eq!(sched.cycles(), vec![40, 40, 40, 40]);
    }

    #[test]
    fn worker_fault_is_reported_with_core_index() {
        let ok_core = core_with_program(&[0x0001; 16]); // NOPs
        let bad_core = core_with_program(&[0xFFFF]); // invalid opcode

        let mut sched = MultiCoreScheduler::new(vec![ok_core, bad_core]);
        let err = sched.run_quantum(4, false).unwrap_err();
        assert_eq!(err.0, 1);
    }
}
