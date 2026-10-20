//! SLIP framing (RFC 1055, double-END), which OSC 1.1 and QLab use for OSC
//! over TCP: every packet is sent as `END escaped-bytes END`.

const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

/// One packet, framed.
pub fn encode(packet: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packet.len() + 2);
    out.push(END);
    for &b in packet {
        match b {
            END => out.extend([ESC, ESC_END]),
            ESC => out.extend([ESC, ESC_ESC]),
            _ => out.push(b),
        }
    }
    out.push(END);
    out
}

/// Reassembles packets from a byte stream that arrives in arbitrary chunks.
#[derive(Default)]
pub struct Decoder {
    packet: Vec<u8>,
    escaped: bool,
}

impl Decoder {
    /// Feed bytes; returns every packet they complete. Empty frames (the
    /// back-to-back ENDs of double-END framing) are skipped.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut done = Vec::new();
        for &b in bytes {
            if self.escaped {
                self.escaped = false;
                self.packet.push(match b {
                    ESC_END => END,
                    ESC_ESC => ESC,
                    // Protocol violation; keep the byte rather than drop data.
                    other => other,
                });
                continue;
            }
            match b {
                END => {
                    if !self.packet.is_empty() {
                        done.push(std::mem::take(&mut self.packet));
                    }
                }
                ESC => self.escaped = true,
                _ => self.packet.push(b),
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_with_escapes() {
        let packet = vec![1, END, 2, ESC, 3];
        let framed = encode(&packet);
        assert_eq!(framed, vec![END, 1, ESC, ESC_END, 2, ESC, ESC_ESC, 3, END]);
        assert_eq!(Decoder::default().feed(&framed), vec![packet]);
    }

    #[test]
    fn test_packets_split_across_reads() {
        let mut framed = encode(b"one");
        framed.extend(encode(b"two"));
        let mut d = Decoder::default();
        let (a, b) = framed.split_at(3);
        assert_eq!(d.feed(a), Vec::<Vec<u8>>::new());
        assert_eq!(d.feed(b), vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[test]
    fn test_escape_split_across_reads() {
        let framed = encode(&[END]);
        let mut d = Decoder::default();
        assert!(d.feed(&framed[..2]).is_empty());
        assert_eq!(d.feed(&framed[2..]), vec![vec![END]]);
    }
}
