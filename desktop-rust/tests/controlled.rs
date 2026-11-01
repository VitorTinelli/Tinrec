use std::{path::Path, sync::mpsc, thread, time::Duration};
use tinrec_capture::{
    RecordOptions,
    recording::{self, Control},
};

fn options(output: &Path) -> RecordOptions {
    RecordOptions {
        fast_finalize: true,
        output: output.to_owned(),
        duration: 10.0,
        system_audio: true,
        microphone: true,
        microphone_device: None,
        display: "auto".into(),
        backend: "auto".into(),
        resolution: 720,
        microphone_volume: 50,
        system_volume: 80,
        ffmpeg: "ffmpeg".into(),
        ffprobe: "ffprobe".into(),
        pause_after: None,
        pause_for: 2.0,
    }
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn desktop_control_channel_can_pause_resume_stop_and_start_a_second_session() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..2 {
        let output = dir.path().join(format!("session-{index}.mp4"));
        let config = options(&output);
        let (controls, receiver) = mpsc::channel();
        let (progress, events) = mpsc::channel();
        let engine = thread::spawn(move || {
            recording::run_controlled(&config, true, receiver, |event| {
                let _ = progress.send(event);
            })
        });
        let wait_for = |state: &str| {
            loop {
                let event = events.recv_timeout(Duration::from_secs(10)).unwrap();
                if event.state == state {
                    break event;
                }
            }
        };
        wait_for("recording");
        thread::sleep(Duration::from_millis(350));
        controls.send(Control::Pause).unwrap();
        let paused = wait_for("paused");
        let still_paused = wait_for("paused");
        assert!((paused.active_seconds - still_paused.active_seconds).abs() < 0.01);
        controls.send(Control::Resume).unwrap();
        wait_for("recording");
        thread::sleep(Duration::from_millis(350));
        controls.send(Control::Stop).unwrap();
        wait_for("finalizing");
        let report = engine.join().unwrap().unwrap();
        assert_eq!(report.status, "completed", "{:?}", report.error);
        assert!(report.active_seconds < 3.0);
        assert!((report.audio_peak[0] - 0.08).abs() < 0.001);
        assert!((report.audio_peak[1] - 0.05).abs() < 0.001);
        assert!(output.exists());
    }
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn live_preview_and_mute_controls_affect_the_recorded_audio() {
    use std::sync::{Arc, Mutex};
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("live-controls.mp4");
    let config = options(&output);
    let (controls, receiver) = mpsc::channel();
    let (progress, events) = mpsc::channel();
    let previews = Arc::new(Mutex::new(Vec::new()));
    let captured = previews.clone();
    let engine = thread::spawn(move || {
        recording::run_observed(
            &config,
            true,
            receiver,
            |event| {
                let _ = progress.send(event);
            },
            Some(&|frame| {
                captured.lock().unwrap().push(frame);
            }),
        )
        .unwrap()
    });
    let wait = |predicate: &dyn Fn(&recording::Progress) -> bool| loop {
        let event = events.recv_timeout(Duration::from_secs(10)).unwrap();
        if predicate(&event) {
            break event;
        }
    };
    wait(&|p| p.state == "recording" && p.active_seconds > 0.3);
    controls.send(Control::SetMicrophoneMuted(true)).unwrap();
    controls.send(Control::SetSystemMuted(true)).unwrap();
    let muted = wait(&|p| p.microphone_muted && p.system_muted);
    wait(&|p| p.active_seconds > muted.active_seconds + 0.8);
    controls.send(Control::SetSystemMuted(false)).unwrap();
    let system_only = wait(&|p| p.microphone_muted && !p.system_muted);
    wait(&|p| p.active_seconds > system_only.active_seconds + 0.8);
    controls.send(Control::SetMicrophoneMuted(false)).unwrap();
    let both = wait(&|p| {
        !p.microphone_muted && !p.system_muted && p.active_seconds > system_only.active_seconds
    });
    let end = wait(&|p| p.active_seconds > both.active_seconds + 0.6);
    controls.send(Control::Stop).unwrap();
    let report = engine.join().unwrap();
    assert_eq!(report.status, "completed", "{:?}", report.error);
    let frames = previews.lock().unwrap();
    assert!(!frames.is_empty());
    assert!(frames.len() <= ((report.wall_seconds * 2.).ceil() as usize) + 1);
    assert!(frames.iter().all(|f| f.width <= 480
        && f.height <= 270
        && f.rgb.len() == (f.width * f.height * 3) as usize
        && f.source_width == 640
        && f.source_height == 360));
    let decode = |index: u8| {
        let result = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-map",
                &format!("0:a:{index}"),
                "-ac",
                "1",
                "-ar",
                "48000",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(result.status.success());
        result
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|s| f32::from_le_bytes(*s))
            .collect::<Vec<_>>()
    };
    let rms = |samples: &[f32], start: f64, end: f64| {
        let window = &samples[(start * 48000.) as usize..(end * 48000.) as usize];
        (window.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / window.len() as f64).sqrt()
    };
    let system = decode(1);
    let microphone = decode(2);
    // Progress acknowledges controls at 200 ms intervals; stay away from AAC and acknowledgment boundaries.
    for track in [&system, &microphone] {
        assert!(
            rms(
                track,
                muted.active_seconds + 0.15,
                system_only.active_seconds - 0.35
            ) < 0.001
        );
        assert!(rms(track, both.active_seconds + 0.15, end.active_seconds - 0.1) > 0.01);
    }
    assert!(
        rms(
            &system,
            system_only.active_seconds + 0.15,
            both.active_seconds - 0.35
        ) > 0.01
    );
    assert!(
        rms(
            &microphone,
            system_only.active_seconds + 0.15,
            both.active_seconds - 0.35
        ) < 0.001
    );
}
