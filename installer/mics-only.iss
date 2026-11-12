#define MyAppName "MICS Only"
#define MyAppVersion "0.1.0"
[Setup]
AppId={{B8C9B4A1-A07A-4287-B2CC-431381211D25}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=maedandori
DefaultDirName={localappdata}\Programs\MICS-Only
DefaultGroupName=MICS Only
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=MICS-Only-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\mics-only.exe

[Files]
Source: "..\target\release\mics-only.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\mics.ini.example"; DestDir: "{app}"; DestName: "mics.ini"; Flags: onlyifdoesntexist
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\MICS Only - Readme"; Filename: "{app}\README.md"
Name: "{group}\MICS Only - Config"; Filename: "{app}\mics.ini"
Name: "{group}\MICS Only - Check Config"; Filename: "{app}\mics-only.exe"; Parameters: "--check-config"; WorkingDir: "{app}"

[Run]
Filename: "{app}\README.md"; Description: "Read installation instructions"; Flags: postinstall shellexec skipifsilent
