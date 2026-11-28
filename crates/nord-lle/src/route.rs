//! ARM message to chip/core routing (port of `tools/arm_dsp_route.py::Router`).
//!
//! The comm index is the download order FX 0, PNO 1, ORG 2, SY 3, SMP 4 (ASSUMED,
//! `arm-param-messages.md` s3). Path 2 messages (FX, PNO, ORG) carry a system byte address:
//! `0x28000000 + 4 w` for core 1, `0x28800000 + 4 w` for core 2 (`tools/arm_emu.py::Message`);
//! stores into the L2 window below (`0x20000000..0x28000000`, wavetables of the synth stages,
//! VERIFIED `synth-stage-models-12.md` s1) are forwarded to core 2 of the same chip as data word
//! `byte >> 2`. Path 1 messages (SY, SMP) carry a DM word address and go to core 0.

use nord_arm::machine::system::Message;
use std::collections::BTreeMap;

/// Chip names by comm index.
pub const CHIP_NAMES: [&str; 5] = ["FX", "PNO", "ORG", "SY", "SMP"];
/// Data words in one core window (`0x28000000 + 4 w`, `w < 2^20`).
pub const DM_WORDS: u32 = 0x10_0000;
const SYS_BASE: u32 = 0x2800_0000;
const CORE2_BASE: u32 = 0x2880_0000;
const L2_LOW: u32 = 0x2000_0000;

/// Where a message goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// DM word write on `(comm, core)`.
    Core {
        comm: u8,
        core: u8,
        word: u32,
        value: u32,
    },
    /// L2 window store, to core 2 of `comm` as data word `word`.
    L2 { comm: u8, word: u32, value: u32 },
    /// Outside the core window (`word` is the offending data word).
    Skipped { comm: u8, core: u8, word: u32 },
    /// No chip takes this path.
    Counted { comm: u8, path: u8 },
}

/// `(core, data word)` of a path 2 byte address on the three tone/FX chips.
pub fn resolve(addr: u32) -> (u8, u32) {
    if addr >= CORE2_BASE {
        (2, addr.wrapping_sub(CORE2_BASE) / 4)
    } else {
        (1, addr.wrapping_sub(SYS_BASE) / 4)
    }
}

/// Classifies one message.
pub fn classify(m: &Message) -> Dest {
    let (comm, path) = (m.comm, m.path);
    match (path, comm) {
        (2, 0..=2) => {
            if (L2_LOW..SYS_BASE).contains(&m.word1) {
                return Dest::L2 {
                    comm,
                    word: m.word1 >> 2,
                    value: m.word2,
                };
            }
            let (core, word) = resolve(m.word1);
            if word >= DM_WORDS {
                return Dest::Skipped { comm, core, word };
            }
            Dest::Core {
                comm,
                core,
                word,
                value: m.word2,
            }
        }
        (1, 3 | 4) => Dest::Core {
            comm,
            core: 0,
            word: m.word1,
            value: m.word2,
        },
        _ => Dest::Counted { comm, path },
    }
}

/// What the engine does after [`Router::route`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Post `(word, value)` to bus node `node`.
    Post {
        node: usize,
        word: u32,
        value: u32,
    },
    None,
}

/// Owner table `(comm, core)` to bus node plus per-class message counters.
#[derive(Clone, Debug, Default)]
pub struct Router {
    owner: BTreeMap<(u8, u8), usize>,
    /// Messages seen by `(comm, core, path)`.
    pub seen: BTreeMap<(u8, u8, u8), u64>,
    /// Messages delivered to a node, by `(comm, core)`.
    pub sent: BTreeMap<(u8, u8), u64>,
    pub skipped: u64,
    pub counted: u64,
    pub l2_forwarded: u64,
    /// L2 stores whose chip has no core 2 node.
    pub l2_dropped: u64,
}

impl Router {
    /// Node `node` owns `(comm, core)`.
    pub fn add(&mut self, comm: u8, core: u8, node: usize) {
        self.owner.insert((comm, core), node);
    }

    /// The node owning `(comm, core)`.
    pub fn owner(&self, comm: u8, core: u8) -> Option<usize> {
        self.owner.get(&(comm, core)).copied()
    }

    /// Routes one message and counts it.
    pub fn route(&mut self, m: &Message) -> Action {
        let d = classify(m);
        let core_of = |d: &Dest| match *d {
            Dest::Core { core, .. } | Dest::Skipped { core, .. } => core,
            Dest::L2 { .. } => 1,
            Dest::Counted { .. } => 0,
        };
        *self.seen.entry((m.comm, core_of(&d), m.path)).or_default() += 1;
        match d {
            Dest::Core {
                comm,
                core,
                word,
                value,
            } => match self.owner(comm, core) {
                Some(node) => {
                    *self.sent.entry((comm, core)).or_default() += 1;
                    Action::Post { node, word, value }
                }
                None => {
                    self.counted += 1;
                    Action::None
                }
            },
            Dest::L2 { comm, word, value } => match self.owner(comm, 2) {
                Some(node) => {
                    self.l2_forwarded += 1;
                    Action::Post { node, word, value }
                }
                None => {
                    self.l2_dropped += 1;
                    Action::None
                }
            },
            Dest::Skipped { .. } => {
                self.skipped += 1;
                Action::None
            }
            Dest::Counted { .. } => {
                self.counted += 1;
                Action::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(path: u8, comm: u8, word1: u32, word2: u32) -> Message {
        Message {
            path,
            comm,
            word1,
            word2,
            pc: 0,
        }
    }

    #[test]
    fn path_two_addresses_pick_the_core_and_word() {
        assert_eq!(
            classify(&msg(2, 1, 0x2800_0010, 7)),
            Dest::Core {
                comm: 1,
                core: 1,
                word: 4,
                value: 7
            }
        );
        assert_eq!(
            classify(&msg(2, 2, 0x2880_0010, 8)),
            Dest::Core {
                comm: 2,
                core: 2,
                word: 4,
                value: 8
            }
        );
        assert_eq!(
            classify(&msg(2, 0, 0x28FF_FFFC, 1)),
            Dest::Skipped {
                comm: 0,
                core: 2,
                word: 0x1F_FFFF
            }
        );
    }

    #[test]
    fn path_one_goes_to_the_sample_chips_and_the_l2_window_to_core_two() {
        assert_eq!(
            classify(&msg(1, 3, 0x92196, 5)),
            Dest::Core {
                comm: 3,
                core: 0,
                word: 0x92196,
                value: 5
            }
        );
        assert_eq!(
            classify(&msg(1, 1, 4, 5)),
            Dest::Counted { comm: 1, path: 1 }
        );
        assert_eq!(
            classify(&msg(2, 3, 0x2800_0000, 5)),
            Dest::Counted { comm: 3, path: 2 }
        );
        assert_eq!(
            classify(&msg(2, 1, 0x200C_0000, 9)),
            Dest::L2 {
                comm: 1,
                word: 0x0803_0000,
                value: 9
            }
        );
    }

    #[test]
    fn the_router_posts_to_owners_and_counts_the_rest() {
        let mut r = Router::default();
        r.add(2, 2, 0);
        r.add(1, 2, 1);
        assert_eq!(
            r.route(&msg(2, 2, 0x2880_0004, 3)),
            Action::Post {
                node: 0,
                word: 1,
                value: 3
            }
        );
        assert_eq!(
            r.route(&msg(2, 2, 0x2800_0004, 3)),
            Action::None,
            "no ORG core 1 node"
        );
        assert_eq!(
            r.route(&msg(2, 1, 0x2000_0010, 4)),
            Action::Post {
                node: 1,
                word: 0x0800_0004,
                value: 4
            }
        );
        assert_eq!(r.route(&msg(2, 0, 0x2800_0004, 3)), Action::None);
        assert_eq!(r.route(&msg(2, 0, 0x28FF_FFFC, 1)), Action::None);
        assert_eq!(
            (r.counted, r.skipped, r.l2_forwarded, r.l2_dropped),
            (2, 1, 1, 0)
        );
        assert_eq!(r.sent.get(&(2, 2)), Some(&1));
        assert_eq!(r.seen.get(&(2, 1, 2)), Some(&1));
    }
}
