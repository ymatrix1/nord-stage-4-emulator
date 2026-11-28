//! Engine tests with synthetic fixtures: a scripted ARM and fake chips, no firmware.

use crate::arm::{Arm0, ArmHost, FakeArm};
use crate::engine::{ChipNode, LleEngine, Precision};
use crate::smp_refill::{CHANNELS_AT, CNSP_BASE, NSTROKES_AT, STROKE_SIZE, STROKE_TABLE};
use nord_arm::cpu::Bus;
use nord_arm::machine::sysbus::{PORT1_BASE, PORT2_BASE};
use nord_arm::machine::system::Message;
use nord_arm::machine::Machine;
use nord_core::lle_io::{link, LleEvent, LleInput, LleTelemetry, TelemetrySnapshot};
use nord_sharc::core::Core;
use nord_sharc::runner::audio_bus::{BusNode, Dest, Frame, Route};
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<(u32, u32)>>>;

/// A chip double: `post(1, bits)` sets its level, each pass emits `[level, -level]` x 2.
struct Fake {
    level: f32,
    log: Log,
    core: Option<Core>,
    in_frames: usize,
    fail_at: Option<u32>,
    passes: u32,
}

impl Fake {
    fn new(log: &Log) -> Self {
        Self {
            level: 0.0,
            log: log.clone(),
            core: None,
            in_frames: 0,
            fail_at: None,
            passes: 0,
        }
    }
}

impl BusNode for Fake {
    fn in_frames(&self) -> usize {
        self.in_frames
    }
    fn core_mut(&mut self) -> Option<&mut Core> {
        self.core.as_mut()
    }
    fn run_pass(&mut self, input: &[Frame]) -> Result<Vec<Frame>, String> {
        if self.fail_at == Some(self.passes) {
            return Err("fake chip fault".into());
        }
        self.passes += 1;
        if self.in_frames > 0 {
            return Ok(input.to_vec());
        }
        Ok(vec![[self.level, -self.level]; 2])
    }
    fn post(&mut self, w: u32, v: u32) -> bool {
        self.log.borrow_mut().push((w, v));
        if w == 1 {
            self.level = f32::from_bits(v);
        }
        true
    }
}

fn msg(path: u8, comm: u8, word1: u32, word2: u32) -> Message {
    Message {
        path,
        comm,
        word1,
        word2,
        pc: 0,
    }
}

fn chip(comm: u8, core: u8, name: &str, f: Fake) -> ChipNode {
    ChipNode {
        comm,
        core,
        name: name.into(),
        node: Box::new(f),
    }
}

fn org_to_host(script: Vec<(u64, Message)>) -> (LleEngine<FakeArm>, Log) {
    let log: Log = Rc::default();
    let arm = FakeArm {
        script,
        ..Default::default()
    };
    let chips = vec![chip(2, 2, "org2", Fake::new(&log))];
    let routes = vec![Route {
        from: 0,
        to: Dest::Host,
        gain: 1.0,
    }];
    (
        LleEngine::new(arm, chips, routes, Precision::Exact).unwrap(),
        log,
    )
}

#[test]
fn messages_reach_the_chip_that_owns_them_and_shape_the_host_audio() {
    let one = 0.5f32.to_bits();
    // path 2, ORG core 2 window, data word 1 = level
    let m = msg(2, 2, 0x2880_0004, one);
    let (mut e, log) = org_to_host(vec![(3, m), (3, msg(2, 2, 0x2800_0004, 9))]);
    let n = e.render(40).unwrap();
    assert!(n >= 40);
    assert_eq!(
        log.borrow().as_slice(),
        &[(1, one)],
        "core 1 store has no owner"
    );
    let h = e.host();
    assert_eq!((h.left[0], h.right[0]), (0.0, 0.0), "before the message");
    assert_eq!((h.left[39], h.right[39]), (0.5, -0.5), "after the message");
    assert_eq!((e.router.counted, e.stats.messages), (1, 2));
}

#[test]
fn the_dsp_chain_runs_through_the_bus_with_one_pass_of_latency_per_hop() {
    let log: Log = Rc::default();
    let mut fx = Fake::new(&log);
    fx.in_frames = 2;
    let arm = FakeArm {
        script: vec![(0, msg(2, 2, 0x2880_0004, 1.0f32.to_bits()))],
        ..Default::default()
    };
    let chips = vec![chip(2, 2, "org2", Fake::new(&log)), chip(0, 1, "fx1", fx)];
    let routes = vec![
        Route {
            from: 0,
            to: Dest::Node(1),
            gain: 0.5,
        },
        Route {
            from: 1,
            to: Dest::Host,
            gain: 1.0,
        },
    ];
    let mut e = LleEngine::new(arm, chips, routes, Precision::Exact).unwrap();
    e.render(20).unwrap();
    let l = &e.host().left;
    assert_eq!(l[0..2], [0.0, 0.0], "fx sees silence on its first pass");
    assert_eq!(l[l.len() - 1], 0.5, "org level times the route gain");
    assert_eq!(
        e.bus.underruns[1], 2,
        "the first FX pass was short of one tick of input"
    );
}

#[test]
fn the_l2_window_goes_to_core_two_and_sample_chips_take_path_one() {
    let log: Log = Rc::default();
    let arm = FakeArm {
        script: vec![
            (0, msg(2, 1, 0x200C_0000, 7)),
            (0, msg(1, 3, 0x92196, 8)),
            (0, msg(1, 1, 5, 6)),
        ],
        ..Default::default()
    };
    let chips = vec![
        chip(1, 2, "pno2", Fake::new(&log)),
        chip(3, 0, "sy", Fake::new(&log)),
    ];
    let mut e = LleEngine::new(arm, chips, vec![], Precision::Exact).unwrap();
    e.run_ticks(1).unwrap();
    assert_eq!(log.borrow().as_slice(), &[(0x0803_0000, 7), (0x92196, 8)]);
    assert_eq!((e.router.l2_forwarded, e.router.counted), (1, 1));
}

#[test]
fn the_same_inputs_give_the_same_bytes_and_other_inputs_do_not() {
    let script = |v: f32| vec![(2u64, msg(2, 2, 0x2880_0004, v.to_bits()))];
    let run = |v: f32| {
        let (mut e, _) = org_to_host(script(v));
        e.render(200).unwrap();
        (e.digest(), e.host().clone())
    };
    let (a, ha) = run(0.25);
    let (b, hb) = run(0.25);
    let (c, _) = run(0.75);
    assert_eq!(a, b);
    assert_eq!(ha, hb);
    assert_ne!(a, c);
}

#[test]
fn a_chip_fault_stops_the_engine_with_a_named_error_and_stays_stopped() {
    let log: Log = Rc::default();
    let mut f = Fake::new(&log);
    f.fail_at = Some(5);
    let routes = vec![Route {
        from: 0,
        to: Dest::Host,
        gain: 1.0,
    }];
    let mut e = LleEngine::new(
        FakeArm::default(),
        vec![chip(2, 2, "org2", f)],
        routes,
        Precision::Exact,
    )
    .unwrap();
    let err = e.run_ticks(100).unwrap_err();
    assert!(
        err.contains("fake chip fault") && err.contains("node 0"),
        "{err}"
    );
    assert_eq!(e.fault(), Some(err.as_str()));
    assert_eq!(e.step().unwrap_err(), err);
    let mut arm_fail = FakeArm {
        fail_at: Some(1),
        ..Default::default()
    };
    assert!(arm_fail.tick().is_ok() && arm_fail.tick().is_err());
}

#[test]
fn a_bound_npno_is_placed_when_the_smp_descriptor_appears() {
    // stereo stroke on page 7: offset field 2044 * 7 - 148 - 0x2C, 4321 frames
    let mut npno = vec![0u8; STROKE_TABLE + STROKE_SIZE];
    npno[CHANNELS_AT..CHANNELS_AT + 2].copy_from_slice(&2u16.to_be_bytes());
    npno[NSTROKES_AT..NSTROKES_AT + 2].copy_from_slice(&1u16.to_be_bytes());
    let at = STROKE_TABLE;
    npno[at + 8..at + 12].copy_from_slice(&(2044u32 * 7 - 148 - CNSP_BASE).to_be_bytes());
    npno[at + 0x0E..at + 0x12].copy_from_slice(&4321u32.to_be_bytes());
    let log: Log = Rc::default();
    let mut smp = Fake::new(&log);
    smp.core = Some(Core::new(&vec![0u8; 0x24_0000]));
    let base = 0x9_3000;
    let arm = FakeArm {
        script: vec![
            (0, msg(1, 4, base, 5)),
            (0, msg(1, 4, base + 1, 7)),
            (1, msg(1, 4, base + 2, 4321)),
        ],
        ..Default::default()
    };
    let mut e =
        LleEngine::new(arm, vec![chip(4, 0, "smp", smp)], vec![], Precision::Exact).unwrap();
    assert!(e.bind_smp(vec![0u8; 8]).is_err(), "header too short");
    e.bind_smp(npno).unwrap();
    e.run_ticks(3).unwrap();
    assert_eq!(e.smp_refill().unwrap().placed, 1);
    let core = e.bus.nodes_mut()[0].core_mut().unwrap();
    let flash = core.epchain().unwrap().flash.as_ref().unwrap();
    assert_eq!(
        flash.base(),
        nord_sharc::core::periph::smpflash::SmpFlash::base_for(5)
    );
    // a second engine without an SMP node cannot bind
    let (mut e2, _) = org_to_host(vec![]);
    assert!(e2.bind_smp(vec![]).is_err());
}

#[test]
fn live_events_reach_the_arm_and_telemetry_reports_time() {
    let (mut host, mut eng) = link(16);
    let (mut e, _) = org_to_host(vec![]);
    assert!(host.events.push(LleEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 90
    }));
    assert!(host.events.push(LleEvent::Nop));
    assert_eq!(e.drain(&mut eng.events), 2, "Nop consumed too");
    assert_eq!(e.arm.midi_seen, vec![vec![0x90, 60, 90]]);
    assert_eq!(e.stats.events_midi, 1);
    e.run_ticks(48).unwrap();
    let mut snap = TelemetrySnapshot::default();
    e.fill_telemetry(&mut snap);
    assert_eq!(snap.arm_ms, 2);
}

#[test]
fn messages_the_arm_sends_are_visible_to_it_through_the_return_path() {
    // ARM0 spinning on `b .`; the test plays the firmware's port stores between ticks
    let mut m = Machine::new();
    m.bus.mem.write32(0xC000_0000, 0xEAFF_FFFE);
    m.cpu.r[15] = 0xC000_0000;
    m.bus.mem.map(PORT2_BASE, 0x2000);
    m.bus.mem.map(PORT1_BASE, 0x1000);
    let mut arm = Arm0::from_booted(m, 0);
    arm.tick_insns = 1_000;
    let mut e = LleEngine::new(arm, vec![], vec![], Precision::Exact).unwrap();
    let bus = &mut e.arm.machine.bus;
    bus.write32(PORT2_BASE + 0x20, 0x2880_0010); // ORG (comm 2): address, pad, value
    bus.write32(PORT2_BASE + 0x20, 0);
    bus.write32(PORT2_BASE + 0x20, 0xAB);
    bus.write32(PORT1_BASE + 0x30, 0x92414); // SY (comm 3) voice 0 state: busy
    bus.write32(PORT1_BASE + 0x30, 0x1C07CD);
    e.run_ticks(2).unwrap();
    assert_eq!(e.stats.messages, 2);
    assert_eq!(
        e.router.counted, 2,
        "no chips attached: routed, counted, not lost"
    );
    let ret = e
        .arm
        .machine
        .dsp_return
        .as_mut()
        .expect("return source installed");
    let mut w = [0u32; 1];
    ret.read(2, 0x2880_0010, &mut w);
    assert_eq!(w[0], 0xAB, "shadow layer: what the ARM wrote it reads back");
    ret.read(3, 0x92196, &mut w);
    assert_eq!(w[0] & 1, 0, "SY idle mask layer: voice 0 is busy");
    assert_ne!(e.digest(), 0);
}

#[test]
fn recorded_messages_carry_the_pass_of_their_node() {
    let (mut e, _) = org_to_host(vec![(4, msg(2, 2, 0x2880_0004, 5))]);
    e.record_messages();
    e.run_ticks(6).unwrap();
    let log = e.msg_log.as_ref().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].node, log[0].word, log[0].value), (0, 1, 5));
    assert_eq!(
        log[0].pass, 3,
        "3 passes were run before tick 4 (floor(4 * 0.9042))"
    );
}

/// A real `CoreNode`: `DM(I0) = 0` then `JUMP` back to the poll loop, the mailbox cleared on
/// every pass (the program of the `audio_bus_node` tests); the output tap reads the lane set 2
/// pair 0 words that the ARM messages of the test fill.
fn real_core_engine(script: Vec<(u64, Message)>) -> LleEngine<FakeArm> {
    use nord_sharc::runner::audio_bus_node::{CoreNode, OutTap};
    use nord_sharc::runner::pass::PassConfig;
    use nord_sharc::runner::pass_sched::{Engine, PassJob};
    let mut data = vec![0u8; 0x24_0000];
    for p in [0x9004u16, 0x0000, 0x06be, 0x0012, 0x0000] {
        data.extend_from_slice(&p.to_le_bytes());
    }
    let mut core = Core::new(&data);
    core.pc = 0x12_0000;
    core.i[0] = 0xB_00E2;
    let job = PassJob {
        name: "org2".into(),
        core,
        cfg: PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0002,
            max_steps: 100,
            irq: None,
        },
        msgs: Vec::new(),
        msg_base: 0,
        ping_pong: None,
        fx: None,
        engine: Engine::Interp,
    };
    let node = CoreNode::new(job, None, OutTap::Lane { set: 2, pair: 0 }, 1);
    let chips = vec![ChipNode {
        comm: 2,
        core: 2,
        name: "org2".into(),
        node: Box::new(node),
    }];
    let routes = vec![Route {
        from: 0,
        to: Dest::Host,
        gain: 1.0,
    }];
    let arm = FakeArm {
        script,
        ..Default::default()
    };
    LleEngine::new(arm, chips, routes, Precision::Exact).unwrap()
}

/// ARM message storing `value` at DM word `w` of ORG core 2.
fn store(w: u32, value: u32) -> Message {
    msg(2, 2, 0x2880_0000 + 4 * w, value)
}

#[test]
fn real_nodes_are_deterministic_and_the_digest_sees_dsp_state() {
    let level = 0.25f32.to_bits();
    // pointer of lane set 2 -> 0xB0300, two frames of 0.25 there; plus a word outside the tap
    let script = |extra: u32| {
        vec![
            (0, store(0xB_0159 + 2, 0xB_0300)),
            (0, store(0xB_0300, level)),
            (0, store(0xB_0302, level)),
            (1, store(0xC_0000, extra)),
        ]
    };
    let run = |extra: u32| {
        let mut e = real_core_engine(script(extra));
        e.render(100).unwrap();
        let d = e.digest();
        (d, e.host().clone())
    };
    let (a, ha) = run(1);
    let (b, hb) = run(1);
    let (c, hc) = run(2);
    assert_eq!((a, &ha), (b, &hb), "two runs of real nodes are identical");
    assert!(ha.left.contains(&0.25), "the tap carries the stored level");
    assert_eq!(
        ha, hc,
        "a word outside the tap does not change the host audio"
    );
    assert_ne!(a, c, "but it changes the digest: DSP state is hashed");
}

#[test]
fn a_bad_smp_descriptor_is_counted_and_skipped_and_the_engine_keeps_running() {
    let mut npno = vec![0u8; STROKE_TABLE + STROKE_SIZE];
    npno[CHANNELS_AT..CHANNELS_AT + 2].copy_from_slice(&2u16.to_be_bytes());
    npno[NSTROKES_AT..NSTROKES_AT + 2].copy_from_slice(&1u16.to_be_bytes());
    npno[STROKE_TABLE + 8..STROKE_TABLE + 12]
        .copy_from_slice(&(2044u32 * 7 - 148 - CNSP_BASE).to_be_bytes());
    npno[STROKE_TABLE + 0x0E..STROKE_TABLE + 0x12].copy_from_slice(&4321u32.to_be_bytes());
    let log: Log = Rc::default();
    let mut smp = Fake::new(&log);
    smp.core = Some(Core::new(&vec![0u8; 0x24_0000]));
    let base = 0x9_3000;
    let arm = FakeArm {
        script: vec![
            (0, msg(1, 4, base, 0xFFFF_FFFE)), // rec[-11]: a firmware-controlled index
            (0, msg(1, 4, base + 1, 7)),
            (0, msg(1, 4, base + 2, 4321)),
        ],
        ..Default::default()
    };
    let mut e =
        LleEngine::new(arm, vec![chip(4, 0, "smp", smp)], vec![], Precision::Exact).unwrap();
    e.bind_smp(npno).unwrap();
    e.run_ticks(5).unwrap();
    assert_eq!((e.stats.bad_descriptors, e.fault()), (1, None));
    assert!(e.bind_smp(vec![0u8; 1]).is_err());
}
