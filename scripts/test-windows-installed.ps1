param([Parameter(Mandatory=$true)][string]$Installer, [string]$TestRoot, [string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
if (Get-Process Tinrec,tinrec-desktop -ErrorAction SilentlyContinue) {
    throw 'Feche as outras instâncias do Tinrec antes do autoteste instalado.'
}
# A separate directory does not isolate NSIS registration. Fail before mutation.
foreach ($hive in @('HKCU', 'HKLM')) {
    foreach ($branch in @('Software', 'Software/WOW6432Node')) {
        $key = "${hive}:/$branch/Microsoft/Windows/CurrentVersion/Uninstall/Tinrec"
        if (Test-Path -LiteralPath $key) {
            throw 'Tinrec já está registrado. Execute o autoteste em Windows Sandbox ou em um runner descartável.'
        }
    }
}
$repository = Split-Path $PSScriptRoot -Parent
if (!$TestRoot) { $TestRoot = Join-Path $repository ('.tools/installed-test-' + [guid]::NewGuid().ToString('N')) }
$TestRoot = [IO.Path]::GetFullPath($TestRoot)
New-Item -ItemType Directory -Force -Path $TestRoot | Out-Null
$installation = if ($InstallDirectory) { [IO.Path]::GetFullPath($InstallDirectory) } else { Join-Path $TestRoot 'app' }
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
# Run on an isolated CI/Windows Sandbox user: NSIS registers the application.
$setup = Start-Process -FilePath $installerPath -ArgumentList @('/S', ('/D=' + $installation)) -WindowStyle Hidden -PassThru
$installDeadline = [DateTime]::UtcNow.AddMinutes(10)
while (!$setup.WaitForExit(1000)) {
    if ([DateTime]::UtcNow -ge $installDeadline) { Stop-Process -Id $setup.Id; throw 'O instalador excedeu dez minutos' }
}
if ($setup.ExitCode -ne 0) { throw "O instalador falhou: $($setup.ExitCode)" }
$app = Get-ChildItem -LiteralPath $installation -Filter '*.exe' | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
if (!$app) { throw 'Aplicativo não foi instalado' }
foreach ($tool in @('ffmpeg', 'ffprobe')) {
    if (!(Test-Path -LiteralPath "$installation/bin/$tool.exe")) { throw "$tool ausente no instalador" }
}
$previousData = $env:TINREC_DATA_DIR
try {
    $env:TINREC_DATA_DIR = Join-Path $TestRoot 'data'
    $process = Start-Process -FilePath $app.FullName -ArgumentList '--self-test' -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(120000)) { Stop-Process -Id $process.Id; throw 'O autoteste instalado excedeu 120 segundos' }
    if ($process.ExitCode -ne 0) { throw "Autoteste falhou: $($process.ExitCode)" }
    $report = Get-Content -LiteralPath "$env:TINREC_DATA_DIR/self-test-result.json" -Raw | ConvertFrom-Json
    if ($report.status -ne 'passed' -or !$report.frontendIpc -or !$report.nativeController -or !$report.nativePrints -or !$report.selectorClose -or !$report.bundledTools) { throw 'Relatório de autoteste inválido' }
    $report | ConvertTo-Json -Depth 8
} finally { $env:TINREC_DATA_DIR = $previousData }
