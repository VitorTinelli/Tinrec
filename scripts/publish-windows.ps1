param([string]$Dotnet = "dotnet",[string]$Iscc = "ISCC.exe")
$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
 & $Dotnet publish desktop/Tinrec/Tinrec.csproj -c Release -r win-x64 -p:Platform=x64 --self-contained true -o artifacts/windows
 if ($LASTEXITCODE -ne 0) { throw "A compilação falhou." }
 Copy-Item THIRD_PARTY_NOTICES.txt artifacts/windows/THIRD_PARTY_NOTICES.txt
 Compress-Archive -Path artifacts/windows/* -DestinationPath artifacts/Tinrec-Windows-x64.zip -Force
 if (Get-Command $Iscc -ErrorAction SilentlyContinue) {
  & $Iscc installer/Tinrec.iss
  if ($LASTEXITCODE -ne 0) { throw "O instalador não foi gerado." }
 } else { Write-Warning "Instale Inno Setup para gerar o instalador .exe. O pacote portátil já está em artifacts/." }
} finally { Pop-Location }
