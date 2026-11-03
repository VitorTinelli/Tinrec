//! Small streaming Matroska transport to FFmpeg, with explicit millisecond PTS.
//! One sized cluster per packet permits interleaved tracks without buffering a recording.
use std::io::{self, Write};

fn element(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    let value = body.len() as u64;
    let size_bytes = (1..=8).find(|n| value < (1u64 << (7 * n)) - 1).unwrap();
    let size = value | (1 << (7 * size_bytes));
    out.extend_from_slice(&size.to_be_bytes()[8 - size_bytes..]);
    out.extend_from_slice(body);
    out
}
fn uint(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|b| *b != 0).unwrap_or(7);
    element(id, &bytes[start..])
}
fn text(id: &[u8], value: &str) -> Vec<u8> {
    element(id, value.as_bytes())
}
fn master(id: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    element(id, &children.concat())
}

#[derive(Clone, Copy, Debug)]
pub struct AudioTrack {
    pub number: u8,
    pub rate: u32,
    pub channels: u16,
    pub name: &'static str,
}

pub fn header(w: &mut impl Write, width: u32, height: u32, audio: &[AudioTrack]) -> io::Result<()> {
    w.write_all(&master(
        &[0x1a, 0x45, 0xdf, 0xa3],
        &[
            uint(&[0x42, 0x86], 1),
            uint(&[0x42, 0xf7], 1),
            uint(&[0x42, 0xf2], 4),
            uint(&[0x42, 0xf3], 8),
            text(&[0x42, 0x82], "matroska"),
            uint(&[0x42, 0x87], 4),
            uint(&[0x42, 0x85], 2),
        ],
    ))?;
    w.write_all(&[
        0x18, 0x53, 0x80, 0x67, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    ])?;
    w.write_all(&master(
        &[0x15, 0x49, 0xa9, 0x66],
        &[
            uint(&[0x2a, 0xd7, 0xb1], 1_000_000),
            text(&[0x4d, 0x80], "Tinrec"),
            text(&[0x57, 0x41], "Tinrec"),
        ],
    ))?;
    // BITMAPINFOHEADER: top-down BGRA, BI_RGB. FFmpeg decodes it as bgr0.
    let mut bitmap = Vec::new();
    bitmap.extend_from_slice(&40u32.to_le_bytes());
    bitmap.extend_from_slice(&width.to_le_bytes());
    bitmap.extend_from_slice(&(-(height as i32)).to_le_bytes());
    bitmap.extend_from_slice(&1u16.to_le_bytes());
    bitmap.extend_from_slice(&32u16.to_le_bytes());
    bitmap.extend_from_slice(&[0; 24]);
    let mut tracks = vec![master(
        &[0xae],
        &[
            uint(&[0xd7], 1),
            uint(&[0x73, 0xc5], 1),
            uint(&[0x83], 1),
            // Capture is capped at 30 Hz, but damage-driven frames may be sparse.
            // Give the final frame a known duration instead of letting FFmpeg
            // extend it by the preceding (possibly multi-second) frame gap.
            uint(&[0x23, 0xe3, 0x83], 33_333_333),
            text(&[0x86], "V_MS/VFW/FOURCC"),
            element(&[0x63, 0xa2], &bitmap),
            master(
                &[0xe0],
                &[uint(&[0xb0], width as u64), uint(&[0xba], height as u64)],
            ),
        ],
    )];
    for track in audio {
        tracks.push(master(
            &[0xae],
            &[
                uint(&[0xd7], track.number as u64),
                uint(&[0x73, 0xc5], track.number as u64),
                uint(&[0x83], 2),
                text(&[0x86], "A_PCM/FLOAT/IEEE"),
                text(&[0x53, 0x6e], track.name),
                master(
                    &[0xe1],
                    &[
                        element(&[0xb5], &(track.rate as f64).to_be_bytes()),
                        uint(&[0x9f], track.channels as u64),
                        uint(&[0x62, 0x64], 32),
                    ],
                ),
            ],
        ));
    }
    w.write_all(&master(&[0x16, 0x54, 0xae, 0x6b], &tracks))
}

pub fn packet(w: &mut impl Write, track: u8, pts_ms: u64, data: &[u8]) -> io::Result<()> {
    // Avoid copying multi-megabyte video frames just to prepend a cluster header.
    let timestamp = uint(&[0xe7], pts_ms);
    let block_size = data.len() + 4;
    let mut block_prefix = vec![0xa3];
    let size = (block_size as u64) | (1u64 << 56);
    block_prefix.extend_from_slice(&size.to_be_bytes());
    block_prefix.extend_from_slice(&[0x80 | track, 0, 0, 0x80]);
    let cluster_size = (timestamp.len() + block_prefix.len() + data.len()) as u64 | (1u64 << 56);
    w.write_all(&[0x1f, 0x43, 0xb6, 0x75])?;
    w.write_all(&cluster_size.to_be_bytes())?;
    w.write_all(&timestamp)?;
    w.write_all(&block_prefix)?;
    w.write_all(data)
}
