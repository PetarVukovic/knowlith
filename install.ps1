<#
Knowlith installer for Windows.

    irm https://raw.githubusercontent.com/OWNER/knowlith/main/install.ps1 | iex

No administrator rights. Everything lands under the user's own profile, so
uninstalling is deleting two folders and one scheduled task.

Windows PowerShell 5.1 is what ships with Windows 10 and 11, so nothing here
uses syntax newer than that — an installer that needs PowerShell 7 fails on
exactly the machines that most need an installer.
#>

$ErrorActionPreference = 'Stop'

$Repo    = if ($env:KNOWLITH_REPO)    { $env:KNOWLITH_REPO }    else { 'OWNER/knowlith' }
$Version = if ($env:KNOWLITH_VERSION) { $env:KNOWLITH_VERSION } else { 'latest' }
$BinDir  = if ($env:KNOWLITH_BIN_DIR) { $env:KNOWLITH_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Knowlith\bin' }

function Say  { param($Text) Write-Host "  $Text" }
function Fail { param($Text) Write-Host ""; Write-Host "knowlith: $Text" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "Knowlith"

# ---------------------------------------------------------------- platform --

$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    'AMD64' { $target = 'x86_64-pc-windows-msvc' }
    'ARM64' {
        # There is no ARM64 build yet, and the x64 one runs under emulation
        # on Windows on ARM. Saying so beats a download that does nothing.
        Say 'Windows on ARM — installing the x64 build, which runs under emulation'
        $target = 'x86_64-pc-windows-msvc'
    }
    default { Fail "$arch is not supported." }
}
Say "Windows $arch"

# TLS 1.2 is not the default in Windows PowerShell 5.1, and GitHub refuses
# anything older. Without this line the download fails with a connection
# error that points nowhere.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# ----------------------------------------------------------------- version --

if ($Version -eq 'latest') {
    Say 'asking GitHub for the latest release'
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        $Version = $release.tag_name
    } catch {
        Fail "could not work out the latest version. Set KNOWLITH_VERSION and try again."
    }
}
Say "version $Version"

$archive = "knowlith-$Version-$target.zip"
$url     = "https://github.com/$Repo/releases/download/$Version/$archive"

# ---------------------------------------------------------------- download --

$work = Join-Path ([IO.Path]::GetTempPath()) ("knowlith-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work -Force | Out-Null

try {
    Say 'downloading'
    $zip = Join-Path $work $archive
    try {
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    } catch {
        Fail "could not download $url`n  If that version has no Windows build, the release page lists what there is."
    }

    # A mismatch stops the install. A missing checksums file does not: an
    # older release may predate it.
    try {
        $sums = Join-Path $work 'checksums.txt'
        Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Version/checksums.txt" -OutFile $sums -UseBasicParsing
        $line = Select-String -Path $sums -Pattern ([Regex]::Escape($archive)) | Select-Object -First 1
        if ($line) {
            $expected = ($line.Line -split '\s+')[0]
            $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLower()
            if ($actual -ne $expected.ToLower()) {
                Fail 'the download does not match its published checksum. Nothing was installed.'
            }
            Say 'checksum matches'
        }
    } catch {
        # No checksums published for this release.
    }

    Expand-Archive -Path $zip -DestinationPath $work -Force
    $binary = Join-Path $work 'knowlith.exe'
    if (-not (Test-Path $binary)) { Fail 'the archive did not contain knowlith.exe.' }

    # ------------------------------------------------------------- install --

    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $target_exe = Join-Path $BinDir 'knowlith.exe'

    # Windows will not overwrite a running executable. Renaming the old one
    # out of the way works while it runs, and the stale copy is cleaned up
    # on the next install.
    if (Test-Path $target_exe) {
        $old = Join-Path $BinDir 'knowlith.old.exe'
        Remove-Item $old -Force -ErrorAction SilentlyContinue
        try { Rename-Item -Path $target_exe -NewName 'knowlith.old.exe' -Force } catch { }
    }
    Copy-Item -Path $binary -Destination $target_exe -Force

    # Downloaded files are marked as coming from the internet, and SmartScreen
    # then refuses the first run with a dialog that does not say why.
    Unblock-File -Path $target_exe -ErrorAction SilentlyContinue
    Say "installed to $target_exe"

    # ---------------------------------------------------------------- path --

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$BinDir*") {
        $updated = if ([string]::IsNullOrEmpty($userPath)) { $BinDir } else { "$userPath;$BinDir" }
        [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
        # The variable above reaches new processes only; this one makes the
        # rest of this script work in the window it is running in.
        $env:Path = "$env:Path;$BinDir"
        Say "added $BinDir to your PATH — open a new terminal for it to take effect"
    }

    # -------------------------------------------------------------- set up --

    Write-Host ""
    & $target_exe --version
    if ($LASTEXITCODE -ne 0) { Fail 'the binary was installed but will not run.' }

    Write-Host ""
    Write-Host "Next:"
    Write-Host "  knowlith scan C:\Users\you\Documents\YourCompany   read a folder"
    Write-Host "  knowlith serve                                     open the interface on http://127.0.0.1:7717"
    Write-Host "  knowlith connect                                   hand it to Claude and Codex"
    Write-Host "  knowlith autostart on                              keep it running when you close the window"
    Write-Host ""
    Write-Host "Everything it reads stays in your Knowlith folder. Nothing is sent anywhere."
    Write-Host ""
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
