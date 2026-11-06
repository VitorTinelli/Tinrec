use std::{
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tinrec_capture::{
    RecordOptions,
    recording::{self, Control},
};

fn options(output: &Path) -> RecordOptions {
    RecordOptions {
        capture_mode: "screen".into(),
        crop: None,
        webcam: None,
        webcam_disabled: false,
        webcam_visible_in_capture: false,
        webcam_x: 0.0,
        webcam_y: 1.0,
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

// The deadline covers the entire transition: unrelated progress must not reset it.
fn wait_for_progress(
    events: &mpsc::Receiver<recording::Progress>,
    label: &str,
    predicate: impl Fn(&recording::Progress) -> bool,
) -> recording::Progress {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "Timed out waiting for {label}");
        let event = events
            .recv_timeout(remaining)
            .unwrap_or_else(|error| panic!("Waiting for {label}: {error}"));
        if predicate(&event) {
            return event;
        }
    }
}

fn finish_engine<T>(engine: thread::JoinHandle<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(70);
    while !engine.is_finished() {
        assert!(
            Instant::now() < deadline,
            "Recording engine did not finalize within 70 s"
        );
        thread::sleep(Duration::from_millis(20));
    }
    engine
        .join()
        .unwrap_or_else(|error| std::panic::resume_unwind(error))
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
        let wait_for =
            |state: &str| wait_for_progress(&events, state, |event| event.state == state);
        wait_for("recording");
        wait_for_progress(&events, "active capture before pause", |event| {
            event.active_seconds >= 0.35
        });
        controls.send(Control::Pause).unwrap();
        let paused = wait_for("paused");
        let still_paused = wait_for("paused");
        assert!((paused.active_seconds - still_paused.active_seconds).abs() < 0.01);
        controls.send(Control::Resume).unwrap();
        wait_for("recording");
        wait_for_progress(&events, "active capture after resume", |event| {
            event.active_seconds >= paused.active_seconds + 0.35
        });
        controls.send(Control::Stop).unwrap();
        wait_for("finalizing");
        let report = finish_engine(engine).unwrap();
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
    let wait = |predicate: &dyn Fn(&recording::Progress) -> bool| {
        wait_for_progress(&events, "mute control acknowledgment", predicate)
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
    let report = finish_engine(engine);
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

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn area_selection_happens_before_recording_and_output_is_cropped() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("area.mp4");
    let mut config = options(&output);
    config.capture_mode = "area".into();
    config.system_audio = false;
    config.microphone = false;
    config.duration = 0.5;
    let (sender, receiver) = mpsc::channel();
    let report = recording::run_observed(
        &config,
        true,
        receiver,
        |_| {},
        Some(&|frame| {
            assert_eq!((frame.source_width, frame.source_height), (640, 360));
            thread::sleep(Duration::from_millis(350));
            sender
                .send(Control::SelectArea(pinray::Rect {
                    x: 101,
                    y: 50,
                    width: 161,
                    height: 160,
                }))
                .unwrap();
        }),
    )
    .unwrap();
    assert_eq!(report.status, "completed", "{:?}", report.error);
    assert!(report.active_seconds < 0.7);
    assert_eq!(
        report.encoder_video_bytes,
        report.submitted_video_frames * 161 * 160 * 4
    );
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["width"], 724);
    assert_eq!(probe["streams"][0]["height"], 720);
}
#[test]
fn cancelling_area_selection_does_not_create_a_video() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("cancel.mp4");
    let mut config = options(&output);
    config.capture_mode = "area".into();
    let (sender, receiver) = mpsc::channel();
    let report = recording::run_observed(
        &config,
        true,
        receiver,
        |_| {},
        Some(&|_| {
            sender.send(Control::Stop).unwrap();
        }),
    )
    .unwrap();
    assert_eq!(report.status, "failed");
    assert!(!output.exists());
    assert!(!tinrec_capture::encoder::sidecar(&output, ".partial.mp4").exists());
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn webcam_is_cropped_and_can_toggle_move_and_toggle_back() {
    use std::cell::Cell;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("webcam-controls.mp4");
    let mut config = options(&output);
    config.capture_mode = "area".into();
    config.crop = Some("100,50,320,240".into());
    config.webcam = Some("generated".into());
    config.webcam_disabled = true;
    config.system_audio = false;
    config.microphone = false;
    config.duration = 2.4;
    let (sender, receiver) = mpsc::channel();
    let stage = Cell::new(0);
    let previews = Cell::new(0);
    let report = recording::run_observed_with_webcam(
        &config,
        true,
        receiver,
        |progress| {
            let schedule = [
                (0.2, Control::SetWebcamEnabled(true)),
                (0.7, Control::SetWebcamPosition(1., 0.)),
                (1.2, Control::SetWebcamEnabled(false)),
                (1.7, Control::SetWebcamEnabled(true)),
            ];
            let index = stage.get();
            if index < schedule.len() && progress.active_seconds >= schedule[index].0 {
                sender.send(schedule[index].1).unwrap();
                stage.set(index + 1);
            }
        },
        None,
        Some(&|frame| {
            assert_eq!(&frame.rgb[..3], &[0, 0, 255]);
            previews.set(previews.get() + 1);
        }),
    )
    .unwrap();
    assert_eq!(report.status, "completed", "{:?}", report.error);
    assert_eq!(stage.get(), 4);
    assert!(previews.get() > 5);
    for (time, bottom_left, top_right) in [
        (0.05, false, false),
        (0.5, true, false),
        (1.0, false, true),
        (1.5, false, false),
        (2.1, false, true),
    ] {
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-ss", &time.to_string(), "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 960 * 720 * 3);
        for (x, y, expected) in [(90, 630, bottom_left), (870, 90, top_right)] {
            let offset = (y * 960 + x) * 3;
            let pixel = &decoded.stdout[offset..offset + 3];
            let blue = pixel[0] < 20 && pixel[1] < 20 && pixel[2] > 230;
            assert_eq!(blue, expected, "t={time}, ({x},{y}), {pixel:?}");
        }
    }
}
