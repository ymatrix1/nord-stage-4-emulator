//! Series words of the committed capture `docs/research/data/fx-param-fits.csv` (PR #119) that the
//! closed forms of `fw_map` are checked against: `(UI value, 32-bit word)` pairs, copied verbatim.
//! Source: the `samples` column of each named series (slot, type, id, list, entry, ctx).

pub(super) const DLY_SEND: [(i32, u32); 11] = [
    (0, 0x00000000),
    (1, 0x39800000),
    (2, 0x3a800000),
    (15, 0x3d610000),
    (31, 0x3e704000),
    (47, 0x3f0a1000),
    (63, 0x3f781000),
    (79, 0x3f800000),
    (95, 0x3f800000),
    (111, 0x3f800000),
    (127, 0x3f800000),
];

pub(super) const DLY_DRY: [(i32, u32); 11] = [
    (0, 0x3f800000),
    (1, 0x3f800000),
    (2, 0x3f800000),
    (15, 0x3f800000),
    (31, 0x3f800000),
    (47, 0x3f800000),
    (63, 0x3f800000),
    (79, 0x3f161000),
    (95, 0x3e882000),
    (111, 0x3d908000),
    (127, 0x39800000),
];

pub(super) const DLY_FBK: [(i32, u32); 11] = [
    (0, 0x00000000),
    (1, 0x3c010204),
    (2, 0x3c810204),
    (15, 0x3df1e3c8),
    (31, 0x3e79f3e8),
    (47, 0x3ebd7af6),
    (63, 0x3efdfbf8),
    (79, 0x3f1f3e7d),
    (95, 0x3f3f7efe),
    (111, 0x3f5fbf7f),
    (127, 0x3f800000),
];

pub(super) const CMP_MAKEUP: [(i32, u32); 11] = [
    (0, 0x3f800000),
    (1, 0x3f800b9f),
    (2, 0x3f802e7c),
    (15, 0x3f8a36b2),
    (31, 0x3fab9fa7),
    (47, 0x3fe4467f),
    (63, 0x401a159c),
    (79, 0x404da6ea),
    (95, 0x40866b95),
    (111, 0x40abd32d),
    (127, 0x40d70a3d),
];

pub(super) const CMP_THRESHOLD: [(i32, u32); 11] = [
    (0, 0x80000000),
    (1, 0xbe912244),
    (2, 0xbf112244),
    (15, 0xc0881020),
    (31, 0xc10c9932),
    (47, 0xc1552a55),
    (63, 0xc18eddbc),
    (79, 0xc1b3264d),
    (95, 0xc1d76ede),
    (111, 0xc1fbb76f),
    (127, 0xc2100000),
];

pub(super) const AMP_DRIVE: [(i32, u32); 11] = [
    (0, 0x00000000),
    (1, 0x3c010204),
    (2, 0x3c810204),
    (15, 0x3df1e3c8),
    (31, 0x3e79f3e8),
    (47, 0x3ebd7af6),
    (63, 0x3efdfbf8),
    (79, 0x3f1f3e7d),
    (95, 0x3f3f7efe),
    (111, 0x3f5fbf7f),
    (127, 0x3f800000),
];

pub(super) const AP_RATE_T2: [(i32, u32); 11] = [
    (0, 0x00003200),
    (1, 0x00007200),
    (2, 0x0000b200),
    (15, 0x0003f200),
    (31, 0x0007f200),
    (47, 0x000bf200),
    (63, 0x000ff200),
    (79, 0x0013f200),
    (95, 0x0017f200),
    (111, 0x001bf200),
    (127, 0x001ff200),
];

pub(super) const AP_RATE_T3: [(i32, u32); 11] = [
    (0, 0x00003200),
    (1, 0x00007200),
    (2, 0x0000b200),
    (15, 0x0003f200),
    (31, 0x0007f200),
    (47, 0x000bf200),
    (63, 0x000ff200),
    (79, 0x0013f200),
    (95, 0x0017f200),
    (111, 0x001bf200),
    (127, 0x001ff200),
];

pub(super) const AP_WET_T2: [(i32, u32); 11] = [
    (0, 0x00000000),
    (1, 0x3b810204),
    (2, 0x3c010204),
    (15, 0x3d71e3c8),
    (31, 0x3df9f3e8),
    (47, 0x3e3d7af6),
    (63, 0x3e7dfbf8),
    (79, 0x3e9f3e7d),
    (95, 0x3ebf7efe),
    (111, 0x3edfbf7f),
    (127, 0x3f000000),
];

pub(super) const AP_DRY_T2: [(i32, u32); 11] = [
    (0, 0x3f800000),
    (1, 0x3f7efdfc),
    (2, 0x3f7dfbf8),
    (15, 0x3f70e1c4),
    (31, 0x3f60c183),
    (47, 0x3f50a142),
    (63, 0x3f408102),
    (79, 0x3f3060c2),
    (95, 0x3f204081),
    (111, 0x3f102040),
    (127, 0x3f000000),
];

pub(super) const AP_WET_T3: [(i32, u32); 9] = [
    (2, 0x3c010204),
    (15, 0x3d71e3c8),
    (31, 0x3df9f3e8),
    (47, 0x3e3d7af6),
    (63, 0x3e7dfbf8),
    (79, 0x3e9f3e7d),
    (95, 0x3ebf7efe),
    (111, 0x3edfbf7f),
    (127, 0x3f000000),
];

pub(super) const AP_DRY_T3: [(i32, u32); 11] = [
    (0, 0x3f800000),
    (1, 0x3f7efdfc),
    (2, 0x3f7dfbf8),
    (15, 0x3f70e1c4),
    (31, 0x3f60c183),
    (47, 0x3f50a142),
    (63, 0x3f408102),
    (79, 0x3f3060c2),
    (95, 0x3f204081),
    (111, 0x3f102040),
    (127, 0x3f000000),
];

pub(super) const TONE_F: [(i32, u32); 13] = [
    (-60, 0xbf5279d0),
    (-59, 0xbf512560),
    (-58, 0xbf4fc700),
    (-45, 0xbf39e570),
    (-30, 0xbf140bb0),
    (-15, 0xbeb38440),
    (-1, 0xbce90400),
    (0, 0x00000000),
    (1, 0x3cead800),
    (15, 0x3f0798e0),
    (30, 0x3fab7ec8),
    (45, 0x40250b7c),
    (60, 0x408f2892),
];

pub(super) const TONE_E: [(i32, u32); 13] = [
    (-60, 0xbf663db6),
    (-59, 0xbf658708),
    (-58, 0xbf64cb98),
    (-45, 0xbf5961d4),
    (-30, 0xbf46dbc6),
    (-15, 0xbf2d01de),
    (-1, 0xbf0d1328),
    (0, 0xbf0a7b80),
    (1, 0xbf0a7b80),
    (15, 0xbf0a7b80),
    (30, 0xbf0a7b80),
    (45, 0xbf0a7b80),
    (60, 0xbf0a7b80),
];

pub(super) const TONE_A: [(i32, u32); 13] = [
    (-60, 0xbf5279d0),
    (-59, 0xbf512560),
    (-58, 0xbf4fc700),
    (-45, 0xbf39e570),
    (-30, 0xbf140bb0),
    (-15, 0xbeb38440),
    (-1, 0xbce90400),
    (0, 0x00000000),
    (1, 0x3cead800),
    (15, 0x3f0798e0),
    (30, 0x3fab7ec8),
    (45, 0x40250b7c),
    (60, 0x408f2892),
];

pub(super) const TONE_BASS_A: [(i32, u32); 13] = [
    (-60, 0x3da03430),
    (-59, 0x3d9bd470),
    (-58, 0x3d979220),
    (-45, 0x3d52f5c0),
    (-30, 0x3d0a3e00),
    (-15, 0x3cb49d00),
    (-1, 0x3c725400),
    (0, 0x3c6b7c00),
    (1, 0x3c6b7c00),
    (15, 0x3c6b7c00),
    (30, 0x3c6b7c00),
    (45, 0x3c6b7c00),
    (60, 0x3c6b7c00),
];

#[test]
fn series_are_sorted_by_ui_value_and_complete() {
    let all: [&[(i32, u32)]; 16] = [
        &DLY_SEND,
        &DLY_DRY,
        &DLY_FBK,
        &CMP_MAKEUP,
        &CMP_THRESHOLD,
        &AMP_DRIVE,
        &AP_RATE_T2,
        &AP_RATE_T3,
        &AP_WET_T2,
        &AP_DRY_T2,
        &AP_WET_T3,
        &AP_DRY_T3,
        &TONE_F,
        &TONE_E,
        &TONE_A,
        &TONE_BASS_A,
    ];
    for s in all {
        assert!(s.len() >= 9);
        assert!(s.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
