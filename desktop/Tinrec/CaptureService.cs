using ScreenRecorderLib;
using System.Diagnostics;
namespace Tinrec;
public sealed class CaptureService : IDisposable
{
    private Recorder? recorder;
    private TaskCompletionSource? completion;
    private TaskCompletionSource? started;
    private readonly Stopwatch clock = new();
    public long ElapsedMs => clock.ElapsedMilliseconds;
    public bool Paused => recorder?.Status == RecorderStatus.Paused;
    public event Action<string>? Failed;
    public long Frames { get; private set; }
    public async Task Start(string path, string display, ScreenRect? region, Preferences preferences, bool hardware = true)
    {
        var source = new DisplayRecordingSource(display) { RecorderApi = RecorderApi.WindowsGraphicsCapture, SourceRect = region };
        var audio = new List<AudioSourceBase>();
        if (preferences.Microphone) audio.Add(string.IsNullOrEmpty(preferences.InputDevice) ? CaptureAudioSource.Default : new CaptureAudioSource(preferences.InputDevice));
        if (preferences.SystemAudio) audio.Add(string.IsNullOrEmpty(preferences.OutputDevice) ? LoopbackAudioSource.Default : new LoopbackAudioSource(preferences.OutputDevice));
        var options = new RecorderOptions
        {
            SourceOptions = new SourceOptions { RecordingSources = [source] },
            OutputOptions = new OutputOptions { RecorderMode = RecorderMode.Video, OutputFrameSize = new ScreenSize(preferences.Resolution == 720 ? 1280 : 1920, preferences.Resolution), Stretch = StretchMode.Uniform, IsVideoFramePreviewEnabled = false },
            VideoEncoderOptions = new VideoEncoderOptions { Framerate = 30, Bitrate = preferences.Resolution == 720 ? 2_500_000 : 5_000_000, IsFixedFramerate = true, IsHardwareEncodingEnabled = hardware, IsMp4FastStartEnabled = true, IsThrottlingDisabled = false, Encoder = new H264VideoEncoder() },
            AudioOptions = new AudioOptions { IsAudioEnabled = audio.Count > 0, AudioSources = audio, Bitrate = AudioBitrate.bitrate_128kbps, Channels = AudioChannels.Stereo },
            MouseOptions = new MouseOptions { IsMousePointerEnabled = true, IsMouseClicksDetected = false }
        };
        completion = new(TaskCreationOptions.RunContinuationsAsynchronously); started = new(TaskCreationOptions.RunContinuationsAsynchronously); Frames = 0;
        recorder = Recorder.CreateRecorder(options);
        recorder.OnStatusChanged += (_, e) => { if (e.Status == RecorderStatus.Recording) { clock.Start(); started.TrySetResult(); } else if (e.Status == RecorderStatus.Paused) clock.Stop(); };
        recorder.OnFrameRecorded += (_, _) => Frames++;
        recorder.OnRecordingComplete += (_, _) => { clock.Stop(); completion.TrySetResult(); };
        recorder.OnRecordingFailed += (_, e) => { clock.Stop(); var error = new Exception(e.Error); if (!started.Task.IsCompleted) started.TrySetException(error); else Failed?.Invoke(e.Error); completion.TrySetException(error); _ = completion.Task.Exception; };
        clock.Reset(); recorder.Record(path);
        try { await started.Task.WaitAsync(TimeSpan.FromSeconds(20)); }
        catch when (hardware) { Dispose(); await Start(path, display, region, preferences, false); }
    }
    public void TogglePause() { if (Paused) recorder?.Resume(); else recorder?.Pause(); }
    public async Task Stop() { if (recorder is null) return; recorder.Stop(); if (completion is not null) await completion.Task.WaitAsync(TimeSpan.FromMinutes(2)); clock.Stop(); }
    public void Dispose() { recorder?.Dispose(); recorder = null; clock.Stop(); }
}
