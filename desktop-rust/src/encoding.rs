//! Hardware encoding stays disabled until recovery can preserve every input frame.
//! A successful capability probe cannot guarantee a later recording will succeed.
use std::path::Path;

pub fn prepare(_ffmpeg: &Path) {}

pub fn hardware(_ffmpeg: &Path, _width: u32, _height: u32, _resolution: u32) -> bool {
    false
}
