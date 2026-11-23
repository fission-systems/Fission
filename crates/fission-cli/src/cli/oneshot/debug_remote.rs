//! Read-only GDB Remote Serial Protocol (RSP) probe for QEMU/gdbserver.
//! A probe is not an interactive debugger backend: it never steps a guest,
//! changes registers or sets/removes breakpoints.

use crate::cli::args::DebugRspProbeArgs;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const MAX_PACKET: usize = 128 * 1024;

fn checksum(payload: &[u8]) -> u8 {
    payload.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn outgoing(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.push(b'$');
    frame.extend_from_slice(payload);
    frame.extend_from_slice(format!("#{:02x}", checksum(payload)).as_bytes());
    frame
}

fn decode_response(packet: &[u8]) -> Result<Vec<u8>> {
    if packet.first() != Some(&b'$') {
        bail!("RSP packet must begin with '$'");
    }
    let hash = packet.iter().position(|b| *b == b'#')
        .context("RSP packet has no checksum")?;
    if hash + 3 != packet.len() {
        bail!("RSP packet has an incomplete checksum or trailing bytes");
    }
    let checksum_text = std::str::from_utf8(&packet[hash + 1..hash + 3])?;
    let expected = u8::from_str_radix(checksum_text, 16)?;
    let actual = checksum(&packet[1..hash]);
    if actual != expected {
        bail!("RSP checksum mismatch (expected {expected:02x}, calculated {actual:02x})");
    }
    Ok(packet[1..hash].to_vec())
}

fn send_packet(stream: &mut TcpStream, command: &str) -> Result<String> {
    stream.write_all(&outgoing(command.as_bytes()))?;
    stream.flush()?;

    // Remote stubs can prefix a response with '+' acknowledgment bytes.
    // Bound the scan and payload to avoid hanging on a malformed endpoint.
    let mut prefix = [0u8; 1];
    let mut scan = 0usize;
    loop {
        stream.read_exact(&mut prefix)?;
        if prefix[0] == b'$' {
            break;
        }
        scan += 1;
        if scan > 32 || (prefix[0] != b'+' && prefix[0] != b'-') {
            bail!("unexpected GDB RSP acknowledgment before response");
        }
    }

    let mut packet = vec![b'$'];
    loop {
        stream.read_exact(&mut prefix)?;
        packet.push(prefix[0]);
        if packet.len() > MAX_PACKET {
            bail!("GDB RSP response exceeds {MAX_PACKET} byte budget");
        }
        if prefix[0] == b'#' {
            let mut digits = [0u8; 2];
            stream.read_exact(&mut digits)?;
            packet.extend_from_slice(&digits);
            break;
        }
    }
    match decode_response(&packet) {
        Ok(payload) => {
            stream.write_all(b"+")?;
            String::from_utf8(payload).context("GDB RSP response was not UTF-8")
        }
        Err(error) => {
            let _ = stream.write_all(b"-");
            Err(error)
        }
    }
}

pub(super) fn probe(args: &DebugRspProbeArgs) -> Result<Value> {
    let address = args.endpoint.to_socket_addrs()
        .with_context(|| format!("resolve RSP endpoint {}", args.endpoint))?
        .next().context("no endpoint address resolved")?;
    let timeout = Duration::from_millis(args.timeout_ms);
    if !(100..=10000).contains(&args.timeout_ms) {
        bail!("--timeout-ms must be between 100 and 10000");
    }
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .with_context(|| format!("connect to RSP endpoint {}", args.endpoint))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    // Query only: '?' asks for the target's stop reason. No code is run.
    let stop_reply = send_packet(&mut stream, "?")?;
    let features = send_packet(&mut stream, "qSupported")?;
    let registers = if args.read_registers {
        let text = send_packet(&mut stream, "g")?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit() || b == b'x') {
            bail!("remote g register reply is not a hexadecimal or unavailable-byte sequence");
        }
        Some(text)
    } else {
        None
    };

    Ok(json!({
        "schema_version": 1,
        "action": "rsp_probe",
        "endpoint": args.endpoint,
        "stop_reply": stop_reply,
        "features": features,
        "raw_register_hex": registers,
        "register_layout": "unknown_without_target_description",
        "read_only": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsp_checksum_roundtrip_and_corruption_detection() {
        let frame = outgoing(b"qSupported");
        assert_eq!(decode_response(&frame).unwrap(), b"qSupported");
        let mut altered = frame;
        altered[1] = b'Q';
        assert!(decode_response(&altered).is_err());
    }

    #[test]
    fn malformed_frames_fail_closed() {
        assert!(decode_response(b"bad").is_err());
        assert!(decode_response(b"$a#01oops").is_err());
        assert!(decode_response(b"$a#").is_err());
    }
}
