#define AppVersion "0.1.0"
[Setup]
AppId={{1057DBCE-FB66-4F49-89DB-52A43E684D8D}
AppName=Tinrec
AppVersion={#AppVersion}
AppPublisher=Tinrec
DefaultDirName={localappdata}\Programs\Tinrec
DefaultGroupName=Tinrec
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\artifacts
OutputBaseFilename=Tinrec-Setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\Tinrec.exe
CloseApplications=yes
[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
[Files]
Source: "..\artifacts\windows\*"; DestDir: "{app}"; Excludes: "*.pdb"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\.tools\vc_redist.x64.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall; Check: NeedsVCRuntime
[Icons]
Name: "{group}\Tinrec"; Filename: "{app}\Tinrec.exe"
[Run]
Filename: "{tmp}\vc_redist.x64.exe"; Parameters: "/install /quiet /norestart"; StatusMsg: "Preparando componentes de captura Microsoft..."; Flags: waituntilterminated; Check: NeedsVCRuntime
Filename: "{app}\Tinrec.exe"; Description: "Abrir Tinrec"; Flags: nowait postinstall skipifsilent

[Code]
function NeedsVCRuntime: Boolean;
var Installed: Cardinal;
begin
  Result := not RegQueryDWordValue(HKLM64, 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64', 'Installed', Installed);
  if not Result then Result := Installed <> 1;
end;
