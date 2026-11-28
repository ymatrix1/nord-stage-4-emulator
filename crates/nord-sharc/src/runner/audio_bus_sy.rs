//! The SY1SY2 and SMP sample chips as sources of the [`AudioBus`](super::audio_bus::AudioBus).
//!
//! These chips have no mailbox pass: a pass is `isrs_per_pass` audio interrupts served by
//! [`SyModel`] (D153), each of which taps one stereo frame (the SMP post-stage ring, or the SY
//! frame buffer, `live_sy.rs`). The node feeds ARM messages into the model's FIFO queue before the
//! pass and returns the tapped frames. Provenance: the model and its taps are those of
//! `sharc-live` (VERIFIED against the oracle runs of `sy-smp-audio.md`); the number of interrupts
//! per bus tick (default 2 = one tick of the 2-frame PNO lane) is ASSUMED.
use super::audio_bus::{BusNode, Frame};
use super::live_sy::SyModel;
use super::msg::Msg;
use crate::core::Core;

/// An SY1SY2 or SMP chip on the bus.
pub struct SyNode {
    pub name: String,
    pub core: Core,
    pub model: SyModel,
    /// FIFO messages: `word1` = FIFO address word, `word2` = value, delivered before pass `pass`.
    pub msgs: Vec<Msg>,
    pub every: u32,
    pub pass: u32,
    next_msg: usize,
}

impl SyNode {
    pub fn new(name: &str, core: Core, model: SyModel, mut msgs: Vec<Msg>, every: u32) -> Self {
        msgs.sort_by_key(|m| m.pass);
        Self {
            name: name.into(),
            core,
            model,
            msgs,
            every,
            pass: 0,
            next_msg: 0,
        }
    }
}

impl BusNode for SyNode {
    fn in_frames(&self) -> usize {
        0
    }

    fn every(&self) -> u32 {
        self.every
    }

    fn core_mut(&mut self) -> Option<&mut Core> {
        Some(&mut self.core)
    }

    fn run_pass(&mut self, _: &[Frame]) -> Result<Vec<Frame>, String> {
        while self.next_msg < self.msgs.len() && self.msgs[self.next_msg].pass <= self.pass {
            let m = self.msgs[self.next_msg];
            self.model.send(m.word1, m.word2);
            self.next_msg += 1;
        }
        let (_, ok) = self.model.pass(&mut self.core);
        if let Some(e) = self.model.take_error() {
            return Err(format!("{}: pass {}: {e}", self.name, self.pass));
        }
        if !ok {
            return Err(format!("{}: pass {} failed", self.name, self.pass));
        }
        self.pass += 1;
        Ok(self
            .model
            .frames()
            .iter()
            .map(|f| [f32::from_bits(f[0]), f32::from_bits(f[1])])
            .collect())
    }

    fn post(&mut self, word1: u32, word2: u32) -> bool {
        self.model.send(word1, word2);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::live_bus::BusTap;
    use super::*;

    /// Core with `RTI` in every vector (IVT at short word `0x120000`, 12 parcels per vector), so an
    /// interrupt costs nothing and the tapped bus pair is whatever the host wrote.
    fn core_with_rti_vectors() -> Core {
        let mut image = vec![0u8; 0x24_0000 + 2 * 12 * 20];
        for v in 0..20usize {
            let at = 0x24_0000 + 2 * 12 * v;
            for (k, p) in [0x0bbeu16, 1, 1].iter().enumerate() {
                image[at + 2 * k..at + 2 * k + 2].copy_from_slice(&p.to_le_bytes());
            }
        }
        let mut c = Core::new(&image);
        c.pc = 0x12_0000 + 3 * 100;
        c.mode1 |= 1 << 12; // MODE1.IRPTEN
        c.write_ureg(0x7D, 1 << 10); // LIRPTL mask bit of vector 17, as the firmware sets it
        c
    }

    fn msg(pass: u32, word1: u32, word2: u32) -> Msg {
        Msg { pass, word1, word2 }
    }

    #[test]
    fn a_pass_returns_one_tapped_frame_per_interrupt_and_messages_follow_pass_order() {
        let mut core = core_with_rti_vectors();
        core.dm_write(0x9_405A, 1.0f32.to_bits()).unwrap();
        core.dm_write(0x9_405B, (-0.5f32).to_bits()).unwrap();
        let mut model = SyModel::new(2);
        model.bus_tap = BusTap::parse(0x9_405A, Some("f")).unwrap();
        // given out of order: the pass-1 message must not enter the queue in pass 0
        let msgs = vec![msg(1, 7, 8), msg(0, 5, 6)];
        let mut n = SyNode::new("sy", core, model, msgs, 1);
        assert_eq!((n.in_frames(), n.every()), (0, 1));
        let f = n.run_pass(&[]).unwrap();
        assert_eq!(
            f,
            vec![[1.0, -0.5], [1.0, -0.5]],
            "isrs_per_pass = 2 frames"
        );
        assert_eq!(
            (n.pass, n.model.queued()),
            (1, 1),
            "only the pass-0 message queued"
        );
        n.run_pass(&[]).unwrap();
        assert_eq!(
            n.model.queued(),
            2,
            "FIFO cap 0: the queue is kept, both messages in"
        );
        assert!(n.core_mut().is_some());
    }

    #[test]
    fn a_posted_message_goes_straight_into_the_model_queue() {
        let mut n = SyNode::new("sy", core_with_rti_vectors(), SyModel::new(2), vec![], 1);
        assert!(n.post(0x92196, 3));
        assert_eq!(n.model.queued(), 1);
    }
}
