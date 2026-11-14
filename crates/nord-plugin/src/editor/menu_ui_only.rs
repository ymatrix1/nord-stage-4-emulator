//! Menu items that intentionally have no host parameter (the allow-list of the mapping test).
//!
//! Reasons are part of the data so a reader of `menu_table::ROWS` sees why an item is absent.

/// `Bound` menu items that intentionally have no host parameter: `(pattern, reason)`. A pattern
/// is an id where `?` matches one character and `*` any run. A stale entry (it matches a mapped
/// item, or nothing) fails the test.
pub const UI_ONLY: &[(&str, &str)] = &[
    (
        "midi.1.local_control",
        "reverts to On at every power-up (manual p.57); a persisted host parameter would defeat that (midi_menu_params.rs)",
    ),
    (
        "midi.2.global_channel",
        "no host parameter: the plugin has no per-channel MIDI routing, the host owns the channel (midi_mode)",
    ),
    (
        "midi.[345].*_?",
        "per-layer MIDI receive channel: no host parameter, the host owns the channel",
    ),
    (
        "midi.6.extern_?",
        "Extern layers send MIDI out of the instrument; the plugin has no MIDI output parameters",
    ),
    (
        "extern.[3-7].*",
        "Extern bank / program / CC send values need MIDI output: no host parameter",
    ),
    (
        "extern.13.*",
        "Extern send-on-load needs MIDI output: no host parameter",
    ),
];

/// Glob match: `?` one character, `*` any run, `[abc]` / `[a-c]` one character of a set.
fn glob(pattern: &str, id: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => (0..=s.len()).any(|i| go(&p[1..], &s[i..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(b'['), Some(&ch)) => {
                let Some(end) = p.iter().position(|&b| b == b']') else {
                    return false;
                };
                let set = &p[1..end];
                let hit = if set.len() == 3 && set[1] == b'-' {
                    (set[0]..=set[2]).contains(&ch)
                } else {
                    set.contains(&ch)
                };
                hit && go(&p[end + 1..], &s[1..])
            }
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), id.as_bytes())
}

/// Reason the `Bound` item `id` has no host parameter, if it is allow-listed.
pub fn ui_only_reason(id: &str) -> Option<&'static str> {
    UI_ONLY
        .iter()
        .find(|(pattern, _)| glob(pattern, id))
        .map(|(_, why)| *why)
}

/// `true` when some allow-list pattern matches `id` (used to find stale patterns).
pub fn pattern_matches(pattern: &str, id: &str) -> bool {
    glob(pattern, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_ids_and_classes() {
        assert!(glob("midi.[345].*_?", "midi.3.organ_a"));
        assert!(glob("midi.[345].*_?", "midi.5.synth_c"));
        assert!(!glob("midi.[345].*_?", "midi.3.organ_sect"));
        assert!(glob("extern.[3-7].*", "extern.6.cc1_b"));
        assert!(!glob("extern.[3-7].*", "extern.8.mod_wheel_a"));
        assert!(glob("midi.1.local_control", "midi.1.local_control"));
        assert!(!glob("midi.1.local_control", "midi.1.local_controls"));
        assert!(ui_only_reason("extern.13.send_on_load_c").is_some());
        assert!(ui_only_reason("system.2.transpose").is_none());
    }
}
