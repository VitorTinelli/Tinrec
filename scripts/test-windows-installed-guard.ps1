$ErrorActionPreference = 'Stop'
# Override discovery only. The real installer script must abort before resolving
# an installer, creating directories or starting any process.
function Get-Process { param($Name, $ErrorAction) return $null }
function Test-Path { param($LiteralPath) return $LiteralPath -eq $global:tinrecGuardTestKey }
foreach ($hive in @('HKCU', 'HKLM')) {
    foreach ($branch in @('Software', 'Software/WOW6432Node')) {
        $global:tinrecGuardTestKey = "${hive}:/$branch/Microsoft/Windows/CurrentVersion/Uninstall/Tinrec"
        $blocked = $false
        try { & "$PSScriptRoot/test-windows-installed.ps1" -Installer 'MUST-NOT-BE-OPENED.exe' }
        catch { $blocked = $_.Exception.Message -like '*Tinrec já está registrado*' }
        if (!$blocked) { throw "Existing installation was not blocked: $global:tinrecGuardTestKey" }
    }
}
Write-Output 'Installer registration guard: 4 scenarios passed'
