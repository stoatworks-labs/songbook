//! The MIDI-over-TCP wire format shared by every Allen & Heath desk.
//!
//! All of them speak raw MIDI bytes on TCP 51325 (dLive also on 51328 for the
//! surface, and TLS variants on 51327/51329 that this crate does not use).
//! Two dialects sit on top:
//!
//! - **SQ / Qu / CQ**: everything is an NRPN with a 14-bit parameter number
//!   and a 14-bit value (`BN 63 MSB, BN 62 LSB, BN 06 coarse, BN 26 fine`);
//!   a `get` is the same NRPN with a data-increment of `7F`; scenes are bank
//!   select + program change. No running status.
//! - **dLive / Avantis**: the MIDI channel selects the *type* of audio channel
//!   (inputs on N, groups on N+1, auxes on N+2, matrices on N+3, everything
//!   else on N+4) and the note number selects which one; mutes are Note On,
//!   faders are NRPN parameter `17` with a single 7-bit value, sends, names,
//!   colours and preamps are SysEx under the A&H header
//!   `F0 00 00 1A 50 10 01 00`, and the desk uses running status.
//!
//! This module holds the byte-level pieces both dialects need: builders for
//! each message, and a stream parser that turns bytes from the socket into
//! [`Event`]s while tolerating running status and interleaved SysEx.

/// A&H SysEx header: manufacturer id, `50 10`, protocol major/minor.
pub const SYSEX_HEADER: [u8; 8] = [0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00];

/// Port every A&H desk listens on for unencrypted MIDI over TCP.
pub const PORT: u16 = 51325;

/// A parsed message from the desk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Note On with velocity (Note Off arrives as velocity 0).
    Note { channel: u8, note: u8, velocity: u8 },
    /// A complete NRPN: parameter (14-bit) and value. `coarse` is data entry
    /// MSB (CC 6); `fine` is CC 38 when it was sent, else `None`.
    Nrpn { channel: u8, param: u16, coarse: u8, fine: Option<u8> },
    /// A data increment (CC 96) / decrement (CC 97) on the current NRPN.
    NrpnStep { channel: u8, param: u16, increment: bool, value: u8 },
    /// Bank select (CC 0).
    BankSelect { channel: u8, bank: u8 },
    ProgramChange { channel: u8, program: u8 },
    /// Pitch bend, as dLive uses it for preamp gain: `EN MP GV`.
    PitchBend { channel: u8, lsb: u8, msb: u8 },
    /// A SysEx body under the A&H header (the header and `F7` stripped),
    /// so `body[0]` is `0N` and `body[1]` the opcode.
    SysEx { body: Vec<u8> },
    /// Any other control change.
    Control { channel: u8, controller: u8, value: u8 },
}

/// Incremental parser over the TCP byte stream.
#[derive(Debug, Default)]
pub struct Parser {
    status: Option<u8>,
    data: Vec<u8>,
    sysex: Option<Vec<u8>>,
    /// NRPN state per MIDI channel: (param msb, param lsb, data coarse).
    nrpn: [(Option<u8>, Option<u8>, Option<u8>); 16],
}

fn data_len(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1,
        _ => 2,
    }
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Feed bytes; returns every complete event they finish.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut out = vec![];
        for &b in bytes {
            if let Some(sx) = &mut self.sysex {
                if b == 0xF7 {
                    let body = std::mem::take(sx);
                    self.sysex = None;
                    if body.len() >= SYSEX_HEADER.len() - 1 && body[..7] == SYSEX_HEADER[1..] {
                        out.push(Event::SysEx { body: body[7..].to_vec() });
                    }
                    // Any other manufacturer's SysEx is dropped.
                } else if b & 0x80 != 0 {
                    // A status byte inside SysEx aborts it (real-time messages excepted).
                    if b < 0xF8 {
                        self.sysex = None;
                    }
                } else {
                    sx.push(b);
                }
                continue;
            }
            if b == 0xF0 {
                self.sysex = Some(vec![]);
                self.data.clear();
                continue;
            }
            if b >= 0xF8 {
                continue; // real-time: clock, active sensing
            }
            if b & 0x80 != 0 {
                self.status = Some(b);
                self.data.clear();
                if (0xF1..=0xF7).contains(&b) {
                    self.status = None;
                }
                continue;
            }
            let Some(status) = self.status else { continue };
            self.data.push(b);
            if self.data.len() == data_len(status) {
                let d = std::mem::take(&mut self.data);
                if let Some(e) = self.message(status, &d) {
                    out.push(e);
                }
            }
        }
        out
    }

    fn message(&mut self, status: u8, d: &[u8]) -> Option<Event> {
        let channel = status & 0x0F;
        match status & 0xF0 {
            0x90 => Some(Event::Note { channel, note: d[0], velocity: d[1] }),
            0x80 => Some(Event::Note { channel, note: d[0], velocity: 0 }),
            0xC0 => Some(Event::ProgramChange { channel, program: d[0] }),
            0xE0 => Some(Event::PitchBend { channel, lsb: d[0], msb: d[1] }),
            0xB0 => {
                let (cc, v) = (d[0], d[1]);
                let st = &mut self.nrpn[channel as usize];
                match cc {
                    0x00 => Some(Event::BankSelect { channel, bank: v }),
                    0x63 => {
                        st.0 = Some(v);
                        st.2 = None;
                        None
                    }
                    0x62 => {
                        st.1 = Some(v);
                        st.2 = None;
                        None
                    }
                    0x06 => {
                        st.2 = Some(v);
                        // A dLive fader arrives with only CC 6. Report it now;
                        // if CC 38 follows, report the complete pair as well.
                        let (Some(m), Some(l)) = (st.0, st.1) else { return None };
                        Some(Event::Nrpn { channel, param: param14(m, l), coarse: v, fine: None })
                    }
                    0x26 => {
                        let (Some(m), Some(l), Some(c)) = (st.0, st.1, st.2) else { return None };
                        Some(Event::Nrpn { channel, param: param14(m, l), coarse: c, fine: Some(v) })
                    }
                    0x60 | 0x61 => {
                        let (Some(m), Some(l)) = (st.0, st.1) else { return None };
                        Some(Event::NrpnStep { channel, param: param14(m, l), increment: cc == 0x60, value: v })
                    }
                    _ => Some(Event::Control { channel, controller: cc, value: v }),
                }
            }
            _ => None,
        }
    }
}

/// 14-bit parameter number from its MSB/LSB bytes (each 7-bit).
pub fn param14(msb: u8, lsb: u8) -> u16 {
    ((msb as u16 & 0x7F) << 7) | (lsb as u16 & 0x7F)
}

pub fn split14(v: u16) -> (u8, u8) {
    (((v >> 7) & 0x7F) as u8, (v & 0x7F) as u8)
}

// ---------------------------------------------------------------- builders

/// SQ-style NRPN with a 14-bit value: `BN 63 MB, BN 62 LB, BN 06 VC, BN 26 VF`.
pub fn nrpn14(channel: u8, param: u16, value: u16) -> Vec<u8> {
    let (mb, lb) = split14(param);
    let (vc, vf) = split14(value);
    let s = 0xB0 | (channel & 0x0F);
    vec![s, 0x63, mb, s, 0x62, lb, s, 0x06, vc, s, 0x26, vf]
}

/// SQ-style NRPN carrying a boolean (mute on/off, assign on/off): the fine
/// byte is the flag.
pub fn nrpn_flag(channel: u8, param: u16, on: bool) -> Vec<u8> {
    let (mb, lb) = split14(param);
    let s = 0xB0 | (channel & 0x0F);
    vec![s, 0x63, mb, s, 0x62, lb, s, 0x06, 0x00, s, 0x26, u8::from(on)]
}

/// SQ-style `get`: the NRPN followed by data increment `7F`.
pub fn nrpn_get(channel: u8, param: u16) -> Vec<u8> {
    let (mb, lb) = split14(param);
    let s = 0xB0 | (channel & 0x0F);
    vec![s, 0x63, mb, s, 0x62, lb, s, 0x60, 0x7F]
}

/// dLive-style NRPN with one data byte: `BN 63 CH, BN 62 param, BN 06 value`.
pub fn nrpn7(channel: u8, note: u8, param: u8, value: u8) -> Vec<u8> {
    let s = 0xB0 | (channel & 0x0F);
    vec![s, 0x63, note & 0x7F, s, 0x62, param & 0x7F, s, 0x06, value & 0x7F]
}

/// Note On then Note Off, as dLive mutes and SQ soft keys want.
pub fn note_pulse(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    let s = 0x90 | (channel & 0x0F);
    vec![s, note & 0x7F, velocity & 0x7F, s, note & 0x7F, 0x00]
}

/// Bank select + program change: scene recall on every A&H desk.
pub fn scene_recall(channel: u8, bank: u8, program: u8) -> Vec<u8> {
    vec![0xB0 | (channel & 0x0F), 0x00, bank & 0x7F, 0xC0 | (channel & 0x0F), program & 0x7F]
}

/// A SysEx under the A&H header: `F0 <header> 0N <body…> F7`.
pub fn sysex(channel: u8, body: &[u8]) -> Vec<u8> {
    let mut v = SYSEX_HEADER.to_vec();
    v.push(channel & 0x0F);
    v.extend_from_slice(body);
    v.push(0xF7);
    v
}

pub fn pitch_bend(channel: u8, lsb: u8, msb: u8) -> Vec<u8> {
    vec![0xE0 | (channel & 0x0F), lsb & 0x7F, msb & 0x7F]
}

/// Scene number (1-based) to bank + program, 128 per bank.
pub fn scene_bank_program(scene: u32) -> (u8, u8) {
    let z = scene.saturating_sub(1);
    ((z / 128) as u8, (z % 128) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sq_examples_from_the_protocol_document() {
        // Ip1 to LR, 0dB, Ch1: B0 63 40 B0 62 00 B0 06 76 B0 26 5C
        assert_eq!(nrpn14(0, param14(0x40, 0x00), (0x76 << 7) | 0x5C), [0xB0, 0x63, 0x40, 0xB0, 0x62, 0x00, 0xB0, 0x06, 0x76, 0xB0, 0x26, 0x5C]);
        // Mute Grp 4, Mute On, Ch7: B6 63 04 B6 62 03 B6 06 00 B6 26 01
        assert_eq!(nrpn_flag(6, param14(0x04, 0x03), true), [0xB6, 0x63, 0x04, 0xB6, 0x62, 0x03, 0xB6, 0x06, 0x00, 0xB6, 0x26, 0x01]);
        // LR Mute get, Ch1: B0 63 00 B0 62 00 B0 60 7F
        assert_eq!(nrpn_get(0, 0), [0xB0, 0x63, 0x00, 0xB0, 0x62, 0x00, 0xB0, 0x60, 0x7F]);
        // Scene 156, Ch3: B2 00 01 C2 1B
        let (b, p) = scene_bank_program(156);
        assert_eq!(scene_recall(2, b, p), [0xB2, 0x00, 0x01, 0xC2, 0x1B]);
        assert_eq!(scene_bank_program(7), (0, 6));
        assert_eq!(scene_bank_program(264), (2, 7));
    }

    #[test]
    fn dlive_examples_from_the_protocol_document() {
        // Fader: BN 63 CH, BN 62 17, BN 06 LV
        assert_eq!(nrpn7(0xB, 0x05, 0x17, 0x6B), [0xBB, 0x63, 0x05, 0xBB, 0x62, 0x17, 0xBB, 0x06, 0x6B]);
        // Mute on: 9N CH 7F, 9N CH 00
        assert_eq!(note_pulse(0xB, 0x00, 0x7F), [0x9B, 0x00, 0x7F, 0x9B, 0x00, 0x00]);
        // Get channel name: SysEx Header, 0N, 01, CH, F7
        assert_eq!(sysex(0, &[0x01, 0x07]), [0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00, 0x00, 0x01, 0x07, 0xF7]);
    }

    #[test]
    fn parses_nrpn_pairs_and_running_status() {
        let mut p = Parser::new();
        let ev = p.feed(&nrpn14(0, param14(0x40, 0x27), 0x3FFF));
        assert_eq!(ev, vec![Event::Nrpn { channel: 0, param: param14(0x40, 0x27), coarse: 0x7F, fine: None }, Event::Nrpn { channel: 0, param: param14(0x40, 0x27), coarse: 0x7F, fine: Some(0x7F) }]);
        // dLive running status: 9B 00 7F 01 7F 02 7F
        let ev = p.feed(&[0x9B, 0x00, 0x7F, 0x01, 0x7F, 0x02, 0x7F]);
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[2], Event::Note { channel: 0xB, note: 2, velocity: 0x7F });
        // A name reply split across two reads.
        let mut msg = sysex(0, &[0x02, 0x03]);
        msg.splice(msg.len() - 1..msg.len() - 1, b"Kick".iter().copied());
        let (a, b) = msg.split_at(6);
        assert!(p.feed(a).is_empty());
        let ev = p.feed(b);
        assert_eq!(ev, vec![Event::SysEx { body: vec![0x00, 0x02, 0x03, b'K', b'i', b'c', b'k'] }]);
        // Bank + program.
        let ev = p.feed(&scene_recall(0, 1, 0x1B));
        assert_eq!(ev, vec![Event::BankSelect { channel: 0, bank: 1 }, Event::ProgramChange { channel: 0, program: 0x1B }]);
        // Pitch bend (dLive preamp gain).
        assert_eq!(p.feed(&pitch_bend(0, 0x05, 0x7F)), vec![Event::PitchBend { channel: 0, lsb: 5, msb: 0x7F }]);
    }

    #[test]
    fn foreign_sysex_and_realtime_are_ignored() {
        let mut p = Parser::new();
        let ev = p.feed(&[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7, 0xFE, 0x90, 0x01, 0x7F]);
        assert_eq!(ev, vec![Event::Note { channel: 0, note: 1, velocity: 0x7F }]);
    }
}
