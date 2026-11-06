//! Requires FFmpeg with libx264 and ffprobe on PATH. No screen/mic permissions needed.
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn read_report(output: &Path) -> Value {
    serde_json::from_slice(&fs::read(format!("{}.capture.json", output.display())).unwrap())
        .unwrap()
}
fn amplitude(samples: &[f32], frequency: f64) -> f64 {
    let (sin, cos) = samples
        .iter()
        .enumerate()
        .fold((0.0, 0.0), |(sin, cos), (i, s)| {
            let angle = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
            (
                sin + f64::from(*s) * angle.sin(),
                cos + f64::from(*s) * angle.cos(),
            )
        });
    2.0 * sin.hypot(cos) / samples.len() as f64
}
fn decode_audio(output: &Path, track: usize) -> Vec<f32> {
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(output)
        .args([
            "-map",
            &format!("0:a:{track}"),
            "-t",
            "0.3",
            "-f",
            "f32le",
            "-ac",
            "1",
            "-ar",
            "48000",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect()
}

#[test]
#[ignore = "requires FFmpeg with libx264 and ffprobe"]
fn mp4_pipeline_all_audio_modes_pause_tones_and_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    for (index, flags) in [
        vec![],
        vec!["--system-audio"],
        vec!["--microphone"],
        vec!["--system-audio", "--microphone"],
    ]
    .iter()
    .enumerate()
    {
        let output = dir.path().join(format!("gravação com espaços {index}.mp4"));
        let result = Command::new(env!("CARGO_BIN_EXE_tinrec-capture"))
            .arg("synthetic")
            .arg("--output")
            .arg(&output)
            .args([
                "--duration",
                "1.2",
                "--pause-after",
                "0.4",
                "--pause-for",
                "0.4",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let report = read_report(&output);
        assert_eq!(report["mode"], "synthetic");
        assert_eq!(report["encoder"], "libx264");
        assert_eq!(report["status"], "completed");
        assert_eq!(report["checks"]["passed"], true);
        assert!(report["wall_seconds"].as_f64().unwrap() >= 1.6);
        assert!(report["checks"]["video_duration_seconds"].as_f64().unwrap() < 1.45);
        assert!(report["checks"]["video_frames"].as_u64().unwrap() >= 30);
        if flags.contains(&"--microphone") {
            let probe: Value = serde_json::from_slice(
                &fs::read(format!("{}.ffprobe.json", output.display())).unwrap(),
            )
            .unwrap();
            let microphone = probe["streams"].as_array().unwrap().last().unwrap();
            assert_eq!(microphone["channels"], 1, "voice track must be mono");
        }
        if index == 3 {
            let mix = decode_audio(&output, 0);
            let system = decode_audio(&output, 1);
            let mic = decode_audio(&output, 2);
            assert!(
                amplitude(&mix, 440.0) > 0.015 && amplitude(&mix, 880.0) > 0.015,
                "mix must include both sources"
            );
            assert!(
                amplitude(&system, 440.0) > 0.05 && amplitude(&system, 880.0) < 0.01,
                "system source must stay separate"
            );
            assert!(
                amplitude(&mic, 880.0) > 0.05 && amplitude(&mic, 440.0) < 0.01,
                "microphone source must stay separate"
            );
            let stereo = Command::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(&output)
                .args([
                    "-map", "0:a:0", "-t", "0.3", "-f", "f32le", "-ac", "2", "-ar", "48000",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(stereo.status.success());
            let samples: Vec<_> = stereo
                .stdout
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            let left: Vec<_> = samples.iter().step_by(2).copied().collect();
            let right: Vec<_> = samples.iter().skip(1).step_by(2).copied().collect();
            let (left_voice, right_voice) = (amplitude(&left, 880.0), amplitude(&right, 880.0));
            assert!(
                left_voice > 0.015 && right_voice > 0.015,
                "voice must reach both ears"
            );
            assert!(
                (left_voice - right_voice).abs() / left_voice.max(right_voice) < 0.05,
                "voice must be centered"
            );
        }
        // Destination protection is independent of the selected audio sources.
        if index == 0 {
            let before = fs::read(&output).unwrap();
            let repeat = Command::new(env!("CARGO_BIN_EXE_tinrec-capture"))
                .arg("synthetic")
                .arg("--output")
                .arg(&output)
                .output()
                .unwrap();
            assert!(!repeat.status.success());
            assert_eq!(
                fs::read(&output).unwrap(),
                before,
                "never overwrite a recording"
            );
        }
    }
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn encoder_failure_is_reported_and_never_publishes_a_completed_mp4() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("broken.mp4");
    // ffprobe supports -version but cannot encode: a portable failing encoder stand-in.
    let result = Command::new(env!("CARGO_BIN_EXE_tinrec-capture"))
        .arg("synthetic")
        .arg("--output")
        .arg(&output)
        .args(["--ffmpeg", "ffprobe", "--duration", "1"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert_eq!(read_report(&output)["status"], "failed");
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn interactive_pause_resume_and_stop_finalize_without_waiting_for_the_limit() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        process::Stdio,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("interactive.mp4");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tinrec-capture"))
        .arg("synthetic")
        .arg("--output")
        .arg(&output)
        .args(["--duration", "20", "--system-audio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let (ready, readiness) = mpsc::channel();
    // Drain stderr throughout the test; a full pipe must not prevent child exit.
    let diagnostics = thread::spawn(move || {
        let mut diagnostic = String::new();
        loop {
            let mut line = String::new();
            if stderr.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if line.starts_with("Gravando:") {
                let _ = ready.send(());
            }
            diagnostic.push_str(&line);
        }
        diagnostic
    });
    if let Err(error) = readiness.recv_timeout(Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("child never became ready: {error}");
    }
    thread::sleep(Duration::from_millis(600));
    input.write_all(b"p\n").unwrap();
    input.flush().unwrap();
    thread::sleep(Duration::from_millis(600));
    input.write_all(b"r\n").unwrap();
    input.flush().unwrap();
    thread::sleep(Duration::from_millis(600));
    input.write_all(b"q\n").unwrap();
    input.flush().unwrap();
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(70);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("interactive capture did not exit within 70 s");
        }
        thread::sleep(Duration::from_millis(20));
    }
    let result = child.wait_with_output().unwrap();
    let diagnostic = diagnostics.join().unwrap();
    let _stdout = stdout_reader.join().unwrap();
    assert!(result.status.success(), "{}", diagnostic);
    let report = read_report(&output);
    let active = report["active_seconds"].as_f64().unwrap();
    let wall = report["wall_seconds"].as_f64().unwrap();
    assert!(active > 0.5 && active < 3.0, "stop must finalize early");
    assert!(
        wall - active >= 0.5,
        "pause must be removed from media duration"
    );
    assert_eq!(report["checks"]["passed"], true);
}
