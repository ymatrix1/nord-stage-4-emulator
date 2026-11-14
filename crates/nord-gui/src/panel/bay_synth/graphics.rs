//! Graphics of the Synth OLED: waveform icons, envelope outline, filter sketch, LFO cycle, gate
//! pattern. The `*_points` functions are pure (unit-tested); `paint_graphic` draws them.

use super::oled_types::{Graphic, Icon};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke};

/// OLED pixel colour.
pub(super) const FG: Color32 = Color32::from_rgb(150, 190, 230);

fn text(p: &Painter, pos: Pos2, s: &str, size: f32, al: Align2, col: Color32) {
    p.text(
        pos,
        al,
        s,
        FontId::monospace(size.max(crate::widgets::MIN_FONT_PX)),
        col,
    );
}

/// Waveform icon as `(x, y)` in `0..=1` (`y` up), `n` samples over one or two periods.
pub fn wave_points(icon: Icon, n: usize) -> Vec<(f32, f32)> {
    let periods = 2.0;
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1).max(1) as f32;
            let ph = (x * periods).fract();
            let y = match icon {
                Icon::Sine | Icon::Sample => 0.5 + 0.5 * (ph * std::f32::consts::TAU).sin(),
                Icon::Triangle => 1.0 - (2.0 * ph - 1.0).abs(),
                Icon::Saw => ph,
                Icon::Pulse(w) => {
                    if ph < w.clamp(0.05, 0.95) {
                        1.0
                    } else {
                        0.0
                    }
                }
                Icon::Multi => 0.5 * ph + 0.5 * ((ph * 1.07).fract()),
                Icon::Noise => ((i * 7919) % 13) as f32 / 12.0,
                Icon::Fm => {
                    0.5 + 0.5
                        * (ph * std::f32::consts::TAU
                            + 2.0 * (ph * 3.0 * std::f32::consts::TAU).sin())
                        .sin()
                }
                Icon::Table => {
                    0.5 + 0.35 * (ph * std::f32::consts::TAU).sin()
                        + 0.15 * (ph * 3.0 * std::f32::consts::TAU).sin()
                }
            };
            (x, y.clamp(0.0, 1.0))
        })
        .collect()
}

/// Width of an envelope stage on a log time axis, `0..=1`.
fn stage_w(t: f32) -> f32 {
    ((t.max(0.0005) / 0.0005).ln() / (60.0_f32 / 0.0005).ln()).clamp(0.02, 1.0)
}

/// Envelope outline: attack up, decay to the hold level, release down. `y` up.
pub fn env_points(a: f32, d: f32, r: f32) -> Vec<(f32, f32)> {
    let (wa, wd, wr) = (stage_w(a), stage_w(d), stage_w(r));
    let total = wa + wd + 0.25 + wr;
    let mut x = 0.0;
    let mut v = vec![(0.0, 0.0)];
    x += wa / total;
    v.push((x, 1.0));
    x += wd / total;
    v.push((x, 0.45));
    x += 0.25 / total;
    v.push((x, 0.45));
    v.push((1.0, 0.0));
    v
}

/// Filter response sketch `n` samples, `y` up. `ty` indexes `FILTER_TYPES` (3 = LP+HP, 4 = HP, 5 = BP).
pub fn filter_points(ty: u8, cutoff: f32, res: f32, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1).max(1) as f32;
            let d = (x - cutoff.clamp(0.05, 0.95)) * 6.0;
            let bump = res * (-(d * d) * 6.0).exp() * 0.4;
            let lp = 1.0 / (1.0 + (d * 2.0).exp());
            let hp = 1.0 - lp;
            let y = match ty {
                4 => hp,
                5 => (1.0 - d.abs() * 0.5).clamp(0.05, 1.0),
                3 => lp.min(hp + 0.5).max(0.1),
                _ => lp,
            };
            (x, (0.55 * y + bump).clamp(0.0, 1.0))
        })
        .collect()
}

/// One LFO period. `shape` indexes `LFO_SHAPES` (0 triangle, 1 saw down, 2 saw up, 3 square, 4 S&H).
pub fn lfo_points(shape: u8, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1).max(1) as f32;
            let y = match shape {
                0 => 1.0 - (2.0 * x - 1.0).abs(),
                1 => 1.0 - x,
                2 => x,
                3 => {
                    if x < 0.5 {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => [0.7, 0.2, 0.9, 0.4][((x * 4.0) as usize).min(3)],
            };
            (x, y)
        })
        .collect()
}

fn poly(p: &Painter, area: Rect, pts: &[(f32, f32)]) {
    let map = |&(x, y): &(f32, f32)| {
        pos2(
            area.left() + x * area.width(),
            area.bottom() - y * area.height(),
        )
    };
    for w in pts.windows(2) {
        p.line_segment([map(&w[0]), map(&w[1])], Stroke::new(1.0_f32, FG));
    }
}

/// Draw `g` inside `area`.
pub(super) fn paint_graphic(p: &Painter, area: Rect, g: &Graphic) {
    match g {
        Graphic::Wave { name, ctrl, icon } => {
            let icon_area = Rect::from_min_size(
                pos2(area.right() - 22.0, area.top() + 2.0),
                vec2(20.0, area.height() - 4.0),
            );
            poly(p, icon_area, &wave_points(*icon, 24));
            text(
                p,
                area.left_top() + vec2(1.0, 1.0),
                name,
                6.5,
                Align2::LEFT_TOP,
                FG,
            );
            text(
                p,
                area.left_top() + vec2(1.0, 10.0),
                ctrl,
                6.0,
                Align2::LEFT_TOP,
                FG,
            );
        }
        Graphic::Env { a, d, r } => poly(p, area.shrink2(vec2(2.0, 2.0)), &env_points(*a, *d, *r)),
        Graphic::Filter { ty, cutoff, res } => poly(
            p,
            area.shrink2(vec2(2.0, 2.0)),
            &filter_points(*ty, *cutoff, *res, 32),
        ),
        Graphic::Lfo(s) => poly(p, area.shrink2(vec2(4.0, 3.0)), &lfo_points(*s, 24)),
        Graphic::Pattern { steps, len, cursor } => {
            let len = usize::from((*len).clamp(2, 16));
            let w = area.width() / len as f32;
            for (i, &step) in steps.iter().take(len).enumerate() {
                let g = f32::from(step) / 8.0;
                let x = area.left() + i as f32 * w;
                let h = (area.height() - 2.0) * g.max(0.08);
                let r = Rect::from_min_max(
                    pos2(x + 0.5, area.bottom() - h),
                    pos2(x + w - 0.5, area.bottom()),
                );
                let on = *cursor == Some(i as u8);
                p.rect_filled(
                    r,
                    0.0,
                    if on {
                        Color32::WHITE
                    } else {
                        FG.gamma_multiply(0.7)
                    },
                );
            }
        }
        Graphic::Direction(d) => {
            let s = ["UP", "DOWN", "UP/DOWN", "RANDOM"][usize::from(*d) % 4];
            text(p, area.center(), s, 8.0, Align2::CENTER_CENTER, FG);
        }
        Graphic::Vibrato { rate, amount } => {
            let cycles = (rate / 2.0).clamp(1.0, 4.0);
            let amp = 0.15 + 0.35 * (amount / 10.0).clamp(0.0, 1.0);
            let pts: Vec<_> = (0..40)
                .map(|i| {
                    let x = i as f32 / 39.0;
                    (x, 0.5 + amp * (x * cycles * std::f32::consts::TAU).sin())
                })
                .collect();
            poly(p, area.shrink2(vec2(2.0, 2.0)), &pts);
        }
        Graphic::Big(s) => text(p, area.center(), s, 9.0, Align2::CENTER_CENTER, FG),
        Graphic::List(rows) => {
            let h = area.height() / 3.0;
            for (i, r) in rows.iter().enumerate() {
                let y = area.top() + i as f32 * h;
                let row = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), h));
                if i == 1 {
                    p.rect_filled(row, 0.0, FG);
                }
                text(
                    p,
                    row.left_center() + vec2(2.0, 0.0),
                    r,
                    6.0,
                    Align2::LEFT_CENTER,
                    if i == 1 { Color32::BLACK } else { FG },
                );
            }
        }
        Graphic::Lines(lines) => {
            for (i, l) in lines.iter().enumerate() {
                text(
                    p,
                    area.left_top() + vec2(1.0, 1.0 + i as f32 * 8.0),
                    l,
                    6.0,
                    Align2::LEFT_TOP,
                    FG,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn square_icon_has_two_levels_and_saw_rises() {
        let sq = wave_points(Icon::Pulse(0.5), 40);
        assert!(sq.iter().all(|&(_, y)| y == 0.0 || y == 1.0));
        let narrow = wave_points(Icon::Pulse(0.1), 40);
        let high = |v: &[(f32, f32)]| v.iter().filter(|p| p.1 > 0.5).count();
        assert!(high(&narrow) < high(&sq));
        let saw = wave_points(Icon::Saw, 20);
        assert!(saw[1].1 > saw[0].1);
    }

    #[test]
    fn every_icon_stays_inside_the_unit_box() {
        for icon in [
            Icon::Sine,
            Icon::Triangle,
            Icon::Saw,
            Icon::Pulse(0.3),
            Icon::Multi,
            Icon::Noise,
            Icon::Fm,
            Icon::Table,
            Icon::Sample,
        ] {
            let v = wave_points(icon, 30);
            assert_eq!(v.len(), 30);
            assert!(v
                .iter()
                .all(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
        }
    }

    #[test]
    fn envelope_outline_peaks_after_the_attack_and_ends_at_zero() {
        let v = env_points(0.01, 0.3, 0.25);
        assert_eq!(v[0], (0.0, 0.0));
        assert_eq!(v[1].1, 1.0);
        assert_eq!(v.last().unwrap(), &(1.0, 0.0));
        let slow = env_points(5.0, 0.3, 0.25);
        assert!(slow[1].0 > v[1].0, "longer attack moves the peak right");
    }

    #[test]
    fn lowpass_falls_and_highpass_rises_across_the_cutoff() {
        let lp = filter_points(0, 0.5, 0.0, 32);
        assert!(lp[0].1 > lp[31].1);
        let hp = filter_points(4, 0.5, 0.0, 32);
        assert!(hp[0].1 < hp[31].1);
        let bp = filter_points(5, 0.5, 0.0, 33);
        assert!(bp[16].1 > bp[0].1 && bp[16].1 > bp[32].1);
        let res = filter_points(0, 0.5, 1.0, 32);
        assert!(
            res.iter().map(|p| p.1).fold(0.0, f32::max)
                > lp.iter().map(|p| p.1).fold(0.0, f32::max)
        );
    }

    #[test]
    fn lfo_shapes_have_their_characteristic_form() {
        let tri = lfo_points(0, 21);
        assert!(tri[10].1 > tri[0].1);
        let down = lfo_points(1, 11);
        assert!(down[0].1 > down[10].1);
        let sq = lfo_points(3, 10);
        assert!(sq.iter().all(|&(_, y)| y == 0.0 || y == 1.0));
        assert_eq!(lfo_points(4, 8).len(), 8);
    }

    #[test]
    fn every_graphic_kind_paints_something() {
        let kinds = [
            Graphic::Wave {
                name: "Saw".into(),
                ctrl: "OSC CTRL: 3.0".into(),
                icon: Icon::Saw,
            },
            Graphic::Env {
                a: 0.01,
                d: 0.3,
                r: 0.2,
            },
            Graphic::Filter {
                ty: 0,
                cutoff: 0.5,
                res: 0.3,
            },
            Graphic::Lfo(0),
            Graphic::Pattern {
                steps: [4; 16],
                len: 8,
                cursor: Some(2),
            },
            Graphic::Direction(2),
            Graphic::Vibrato {
                rate: 5.0,
                amount: 5.0,
            },
            Graphic::Big("+3 st".into()),
            Graphic::List(["a".into(), "b".into(), "c".into()]),
            Graphic::Lines(vec!["x".into(), "y".into()]),
        ];
        for g in kinds {
            let n = shapes_painted(|_ui, p, rect, _s, _a| {
                paint_graphic(p, Rect::from_min_size(rect.min, vec2(70.0, 20.0)), &g)
            });
            assert!(n > 0, "{g:?}");
        }
    }
}
