//! Builds a multi-chip LLE system on the [`AudioBus`]: ORG/PNO tone chips and the SY1SY2/SMP
//! sample chips as sources, the FX cores as consumers, the host stereo buffer as the sink (bean
//! `ns4-lle4`; binary `lle_sharc_bus` in `nord-tools`).
//!
//! Node specs `NAME=IMAGE,PRESET[,MSGFILE]` (the presets of [`super::live_presets`]):
//!
//! | Preset | Node | Frames per pass | Runs |
//! |---|---|---|---|
//! | `org`, `pno` | [`CoreNode`], output pointer set 2 pair 0 (`OutTap::Lane`, VERIFIED `pno-full-pass.md` s1) | 2 out | every tick |
//! | `sy`, `smp` | [`SyNode`] (2 audio interrupts per pass) | 2 out | every tick |
//! | `fx1`, `fx2` | [`CoreNode`], input at `FxCore::input_base`, master words out | 4 in, 4 out | every 2nd tick |
//!
//! The tick is the 2-frame period of the tone chips and the FX pass takes 4 frames, so an FX core
//! runs every second tick. VERIFIED: 4 frames per FX pass (`fx_run.rs`) and 2 frames per tone pass
//! (`pno-full-pass.md` s1). ASSUMED: which tone chip feeds which FX input and that Core1 feeds
//! Core2: the routes are the caller's (`A>B[:GAIN]`, `B` = `host`), nothing is wired by default.
use super::audio_bus::{AudioBus, BusNode, Dest, Route};
use super::audio_bus_node::{CoreNode, OutTap};
use super::audio_bus_sy::SyNode;
use super::live_presets::{boot_core, preset_named, Preset};
use super::live_sy::SyModel;
use super::msg::{parse_msg_text, Msg};
use super::pass_sched::{Engine, PassJob};
use super::periph_run::PeriphOpts;
use crate::core::Core;

/// Frames per FX pass and ticks per FX pass.
pub const FX_FRAMES: usize = 4;
pub const FX_EVERY: u32 = 2;
/// Audio interrupts of a sample chip per pass.
pub const SY_ISRS: u32 = 2;

/// `NAME=IMAGE,PRESET[,MSGFILE]`.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    pub name: String,
    pub image: String,
    pub preset: String,
    pub msgs: Option<String>,
}

impl NodeSpec {
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = || format!("bad node {s:?} (want NAME=IMAGE,PRESET[,MSGFILE])");
        let (name, rest) = s.split_once('=').ok_or_else(bad)?;
        let f: Vec<&str> = rest.split(',').collect();
        if name.is_empty() || name == "host" || f.len() < 2 || f.len() > 3 || f[0].is_empty() {
            return Err(bad());
        }
        Ok(Self {
            name: name.into(),
            image: f[0].into(),
            preset: f[1].into(),
            msgs: f.get(2).filter(|m| !m.is_empty()).map(|m| m.to_string()),
        })
    }
}

/// `FROM>TO[:GAIN]` with `TO` a node name or `host`, resolved against `names`.
pub fn parse_route(s: &str, names: &[String]) -> Result<Route, String> {
    let bad = || format!("bad route {s:?} (want FROM>TO[:GAIN], TO a node or host)");
    let (from, rest) = s.split_once('>').ok_or_else(bad)?;
    let (to, gain) = match rest.split_once(':') {
        Some((t, g)) => (t, g.parse::<f32>().map_err(|_| bad())?),
        None => (rest, 1.0),
    };
    let idx = |n: &str| {
        names
            .iter()
            .position(|x| x == n)
            .ok_or_else(|| format!("route {s:?}: no node named {n:?}"))
    };
    Ok(Route {
        from: idx(from)?,
        to: if to == "host" {
            Dest::Host
        } else {
            Dest::Node(idx(to)?)
        },
        gain,
    })
}

/// Boots the chip of `spec` from its flat SRAM `data` and wraps it as a bus node.
pub fn build_node(
    spec: &NodeSpec,
    data: &[u8],
    msgs: &str,
    engine: Engine,
) -> Result<Box<dyn BusNode>, String> {
    let preset =
        preset_named(&spec.preset).ok_or_else(|| format!("unknown preset {:?}", spec.preset))?;
    let core = boot_core(data, &preset, &PeriphOpts::default())?;
    wrap_node(&spec.name, core, &preset, parse_msg_text(msgs)?, engine)
}

/// The bus node of a booted `core` run under `preset` (see the module table).
pub fn wrap_node(
    name: &str,
    core: Core,
    preset: &Preset,
    msgs: Vec<Msg>,
    engine: Engine,
) -> Result<Box<dyn BusNode>, String> {
    if preset.sy {
        let model = if preset.smp {
            SyModel::smp(SY_ISRS)
        } else {
            SyModel::new(SY_ISRS)
        };
        return Ok(Box::new(SyNode::new(name, core, model, msgs, 1)));
    }
    let job = PassJob::from_preset(name, core, preset, msgs, engine);
    Ok(match preset.fx {
        Some(fx) => Box::new(CoreNode::new(
            job,
            Some((fx.input_base, FX_FRAMES)),
            OutTap::of_fx(&fx),
            FX_EVERY,
        )),
        None => Box::new(CoreNode::new(job, None, tone_tap(), 1)),
    })
}

/// The output tap of a tone chip (org, pno, sy, smp presets): lane set 2 pair 0, `DM(0xB015B)`
/// (VERIFIED, `pno-full-pass.md` s1). The first version tapped set 0 and read silence; this shared
/// path also serves `lle_sharc_bus`.
pub fn tone_tap() -> OutTap {
    OutTap::Lane { set: 2, pair: 0 }
}

/// A bus over `nodes` with the `routes` of `route_specs`.
pub fn build_bus(
    names: &[String],
    nodes: Vec<Box<dyn BusNode>>,
    route_specs: &[String],
) -> Result<AudioBus, String> {
    let routes = route_specs
        .iter()
        .map(|r| parse_route(r, names))
        .collect::<Result<Vec<_>, _>>()?;
    AudioBus::new(nodes, routes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["org".into(), "fx1".into()]
    }

    #[test]
    fn node_specs_and_routes_parse() {
        let s = NodeSpec::parse("fx1=a.bin,fx1,m.script").unwrap();
        assert_eq!(
            (s.name.as_str(), s.image.as_str(), s.preset.as_str()),
            ("fx1", "a.bin", "fx1")
        );
        assert_eq!(s.msgs.as_deref(), Some("m.script"));
        assert_eq!(NodeSpec::parse("o=i.bin,org").unwrap().msgs, None);
        for bad in ["x", "=a,org", "host=a,org", "o=a", "o=a,org,m,x", "o=,org"] {
            assert!(NodeSpec::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_route("org>fx1:0.5", &names()),
            Ok(Route {
                from: 0,
                to: Dest::Node(1),
                gain: 0.5
            })
        );
        assert_eq!(parse_route("fx1>host", &names()).unwrap().to, Dest::Host);
        for bad in ["org", "org>nope", "nope>host", "org>fx1:x"] {
            assert!(parse_route(bad, &names()).is_err(), "{bad}");
        }
    }

    #[test]
    fn presets_become_the_right_node_kinds() {
        let data = vec![0u8; 0x24_0000 + 64];
        let kind = |p: &str| {
            let preset = preset_named(p).unwrap();
            let n = wrap_node("n", Core::new(&data), &preset, vec![], Engine::Interp).unwrap();
            (n.in_frames(), n.every())
        };
        assert!(
            matches!(tone_tap(), OutTap::Lane { set: 2, pair: 0 }),
            "tone chips tap output lane set 2 pair 0"
        );
        assert_eq!(kind("org"), (0, 1));
        assert_eq!(kind("pno"), (0, 1));
        assert_eq!(kind("sy"), (0, 1));
        assert_eq!(kind("smp"), (0, 1));
        assert_eq!(kind("fx1"), (4, 2));
        assert_eq!(kind("fx2"), (4, 2));
        let spec = |p: &str| NodeSpec::parse(&format!("n={p}.bin,{p}")).unwrap();
        assert!(build_node(&spec("nope"), &data, "", Engine::Interp).is_err());
        assert!(build_node(&spec("org"), &data, "--msg", Engine::Interp).is_err());
    }

    #[test]
    fn a_bus_needs_resolvable_routes() {
        let b = build_bus(&names(), vec![], &["org>fx1".to_string()]);
        assert!(b.is_err(), "two names but no nodes");
    }
}
