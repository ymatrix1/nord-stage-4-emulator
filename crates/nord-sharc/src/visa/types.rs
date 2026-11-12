//! Instruction types and the instruction record of the VISA decoder.

/// Instruction type of the Programming Reference (letters: a = 48, b = 32, c = 16 bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    T1a,
    T1b,
    T2a,
    T2b,
    T2c,
    /// First byte `0x02`: undocumented 48-bit conditional compute (ASSUMED), see docs 5.9.
    /// Opcode `0x02`: conditional shift immediate (`Rn = FEXT Rx BY bit6:len6`, ...), no data move.
    T6n,
    T3a,
    T3b,
    T3c,
    T4a,
    T4b,
    T5a,
    T5b,
    T6a,
    T7a,
    T7b,
    T8a,
    T9a,
    T9b,
    T10a,
    T11a,
    T11c,
    T12a,
    T13a,
    T14a,
    T15a,
    T15b,
    T16a,
    T16b,
    T17a,
    T17b,
    T18a,
    T19a,
    T20a,
    T25,
    /// `000 00000` first byte, 48-bit form: NOP, idle or data.
    T21,
    /// `000 00000` first byte ending in a 1 bit: 16-bit NOP or idle.
    T21c,
    Unknown,
}

impl Kind {
    /// Name used by the Python tool (`"17a"`, `"21/22"`, `"??"`, ...).
    pub fn name(self) -> &'static str {
        use Kind::*;
        match self {
            T1a => "1a",
            T1b => "1b",
            T2a => "2a",
            T2b => "2b",
            T2c => "2c",
            T6n => "6n",
            T3a => "3a",
            T3b => "3b",
            T3c => "3c",
            T4a => "4a",
            T4b => "4b",
            T5a => "5a",
            T5b => "5b",
            T6a => "6a",
            T7a => "7a",
            T7b => "7b",
            T8a => "8a",
            T9a => "9a",
            T9b => "9b",
            T10a => "10a",
            T11a => "11a",
            T11c => "11c",
            T12a => "12a",
            T13a => "13a",
            T14a => "14a",
            T15a => "15a",
            T15b => "15b",
            T16a => "16a",
            T16b => "16b",
            T17a => "17a",
            T17b => "17b",
            T18a => "18a",
            T19a => "19a",
            T20a => "20a",
            T25 => "25",
            T21 => "21/22",
            T21c => "21c",
            Unknown => "??",
        }
    }
}

/// One instruction: its type and the 1 to 3 parcels it spans (shorter at the end of a stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insn {
    pub kind: Kind,
    pub parcels: [u16; 3],
    pub len: usize,
}

impl Insn {
    /// The parcels of this instruction.
    pub fn words(&self) -> &[u16] {
        &self.parcels[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insn_words_are_the_parcels_in_use() {
        let insn = Insn {
            kind: Kind::T17b,
            parcels: [0x0FA4, 0x0070, 0],
            len: 2,
        };
        assert_eq!(insn.words(), &[0x0FA4, 0x0070]);
        assert_eq!(Kind::T17b.name(), "17b");
    }
}
