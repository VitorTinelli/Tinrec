param([string]$ArchivePath)
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$version = '9.0.2'
$expectedHash = '60F467265B1E312373DBCD92200C2618A74850F98D3D078E94296BB3FA2047BA'
$tools = Join-Path $repository '.tools'
New-Item -ItemType Directory -Force -Path $tools | Out-Null
if (!$ArchivePath) {
    $ArchivePath = Join-Path $tools "ffmpeg-$version.zip"
    if (!(Test-Path -LiteralPath $ArchivePath)) {
        Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-$version-essentials_build.zip" -OutFile $ArchivePath
    }
}
if ((Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'O SHA256 do FFmpeg difere do build homologado. O pacote não será utilizado.'
}
$expanded = Join-Path $tools "ffmpeg-$version"
Expand-Archive -LiteralPath $ArchivePath -DestinationPath $expanded -Force
$root = Join-Path $expanded "ffmpeg-$version-essentials_build"
$destination = Join-Path $repository 'desktop-app/src-tauri/resources/windows'
New-Item -ItemType Directory -Force -Path "$destination/bin", "$destination/licenses" | Out-Null
foreach ($tool in @('ffmpeg', 'ffprobe')) {
    Copy-Item -LiteralPath "$root/bin/$tool.exe" -Destination "$destination/bin/$tool.exe" -Force
    $versionOutput = & "$destination/bin/$tool.exe" -version
    $toolExit = $LASTEXITCODE
    $versionOutput | Select-Object -First 1
    if ($toolExit -ne 0) { throw "$tool não executou corretamente" }
}
Copy-Item -LiteralPath "$root/LICENSE" -Destination "$destination/licenses/FFmpeg-LICENSE.txt" -Force
@("FFmpeg $version", 'https://ffmpeg.org/', 'https://www.gyan.dev/ffmpeg/builds/', 'Source/build instructions: https://github.com/GyanD/codexffmpeg', "Archive SHA256: $expectedHash") | Set-Content "$destination/licenses/FFmpeg-NOTICE.txt" -Encoding utf8
Write-Output "FFmpeg $version verificado e preparado em $destination"
