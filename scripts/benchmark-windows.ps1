param(
    [Parameter(Mandatory=$true)][string]$Baseline,
    [Parameter(Mandatory=$true)][string]$Current,
    [int]$Seconds = 8,
    [ValidateSet('software','mf')][string]$CurrentEncoder = 'software'
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $repository ('artifacts/benchmark-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$ffmpeg = (Resolve-Path "$repository/desktop-app/src-tauri/resources/windows/bin/ffmpeg.exe").Path
$ffprobe = (Resolve-Path "$repository/desktop-app/src-tauri/resources/windows/bin/ffprobe.exe").Path
$previousEncoder = $env:TINREC_ENCODER
try {
    $env:TINREC_ENCODER = 'software'
    $results = foreach ($entry in @(@{Name='baseline';Exe=$Baseline}, @{Name='current';Exe=$Current})) {
        $env:TINREC_ENCODER = if ($entry.Name -eq 'current') { $CurrentEncoder } else { 'software' }
        $file = Join-Path $destination ($entry.Name + '.mp4')
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $output = & $entry.Exe synthetic --output $file --duration $Seconds --capture-mode area --crop '160,90,320,180' --resolution 720 --system-audio --microphone --ffmpeg $ffmpeg --ffprobe $ffprobe 2> "$file.stderr.log"
        if ($LASTEXITCODE -ne 0) { throw "Falha no benchmark $($entry.Name): $file.capture.json" }
        $clock.Stop()
        $report = ($output -join "`n") | ConvertFrom-Json
        $samples = @($report.samples | Where-Object { $_.cpu_percent_one_core -gt 0.1 })
        $rawBytes = if ($entry.Name -eq 'baseline') { $report.submitted_video_frames * 640 * 360 * 4 } else { $report.encoder_video_bytes }
        [pscustomobject]@{
            Name=$entry.Name; ActiveSeconds=$report.active_seconds; WallSeconds=$clock.Elapsed.TotalSeconds
            CapturedFrames=$report.captured_video_frames; DroppedFrames=$report.encoder_dropped_video_frames
            RawEncoderBytes=$rawBytes
            FileBytes=(Get-Item -LiteralPath $file).Length
            MeanCpuOneCore=($samples | Measure-Object cpu_percent_one_core -Average).Average
            PeakCombinedRss=($report.samples | ForEach-Object { $_.rust_rss_bytes + $_.ffmpeg_rss_bytes } | Measure-Object -Maximum).Maximum
            ChecksPassed=$report.checks.passed
            Encoder=if ($entry.Name -eq 'baseline') { 'libx264' } else { $report.encoder }
        }
    }
    $results | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $destination 'comparison.json') -Encoding utf8
    $results | ConvertTo-Json -Depth 6
    Write-Output "Relatório: $destination/comparison.json"
} finally { $env:TINREC_ENCODER=$previousEncoder }
