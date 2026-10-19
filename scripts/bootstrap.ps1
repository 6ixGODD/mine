param(
    [string]$Repository = $(if ($env:MINE_REPO) { $env:MINE_REPO } else { 'https://github.com/6ixGODD/mine.git' }),
    [string]$Ref = $(if ($env:MINE_REF) { $env:MINE_REF } else { 'latest' }),
    [string]$BinDirectory = $(if ($env:MINE_BIN_DIR) { $env:MINE_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\mine' }),
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$SetupArgs
)
# https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.ps1
$ErrorActionPreference = 'Stop'
if (-not [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::Windows)) { throw 'unsupported OS; use bootstrap.sh on Linux; macOS delivery is deferred in 0.2.0' }
if ($Repository -notmatch '^https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?$') { throw 'invalid GitHub repository' }
$ReleaseAccount = if ($env:MINE_RELEASE_ACCOUNT) { $env:MINE_RELEASE_ACCOUNT } else { $Matches[1] }
$ReleaseRepo = if ($env:MINE_RELEASE_REPO) { $env:MINE_RELEASE_REPO } else { $Matches[2] }
foreach ($identity in @($ReleaseAccount,$ReleaseRepo)) { if ($identity -notmatch '^[A-Za-z0-9_-][A-Za-z0-9_.-]*$') { throw 'invalid release identity' } }
# BEGIN MINE PLATFORMS
switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { $Asset = "mine-windows-x86_64.zip"; $BinaryName = "mine.exe" }
    default { throw "unsupported Windows architecture" }
}
# END MINE PLATFORMS
New-Item -ItemType Directory -Force -Path $BinDirectory | Out-Null
$BinDirectory = (Get-Item -LiteralPath $BinDirectory).FullName
$Destination = Join-Path $BinDirectory $BinaryName
if (Test-Path -LiteralPath $Destination) {
    $item = Get-Item -LiteralPath $Destination -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'installation target is not a regular executable' }
}
$Stage = Join-Path $BinDirectory '.mine-binary-swap'
if (Test-Path -LiteralPath $Stage) { throw "pending/busy binary swap at $Stage; inspect candidate.exe/previous and record before retry" }
New-Item -ItemType Directory -Path $Stage -ErrorAction Stop | Out-Null
$Candidate = Join-Path $Stage 'candidate.exe'
$Previous = Join-Path $Stage 'previous'
@{schema_version=1;destination=$Destination;candidate='candidate.exe';backup='previous'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Stage 'record.json') -Encoding UTF8
$Swapping = $false

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','') }
    finally { $stream.Dispose(); $sha.Dispose() }
}

function Get-ReleaseFile([string]$Url,[string]$Output,[long]$Limit,[switch]$Head) {
    if (-not $Url.StartsWith('https://')) { throw 'HTTPS required' }
    $headers = Join-Path $Stage 'headers'
    $arguments = @('--silent','--show-error','--location','--max-redirs','5','--proto','=https','--proto-redir','=https','--connect-timeout','10','--max-time','90','--retry','0','--max-filesize',"$Limit",'--dump-header',$headers,'--output',$Output,'--write-out',"%{http_code}`n%{url_effective}")
    if ($Head) { $arguments += '--head' }
    $result = @(& curl.exe @arguments $Url)
    if ($LASTEXITCODE -ne 0) { throw 'network/timeout/oversized release response; no automatic retry' }
    $status = [int]$result[0]
    if ($status -ne 200) {
        $headerText = Get-Content -LiteralPath $headers -Raw
        if ($status -eq 404) { throw 'HTTP 404: no published release or requested asset; check tag/platform' }
        if ($status -eq 429 -or ($status -eq 403 -and $headerText -match '(?im)^(x-ratelimit-remaining:\s*0\s*$|retry-after:\s*\d+)')) {
            $safeHeaders = [regex]::Matches($headerText,'(?im)^(?:retry-after|x-ratelimit-reset):\s*\d{1,20}\s*$') | ForEach-Object {$_.Value.Trim()}
            throw "HTTP ${status}: rate limit; $($safeHeaders -join '; '); no automatic retry"
        }
        if ($status -eq 401 -or $status -eq 403) { throw "HTTP ${status}: authorization/access denied (not proven rate limiting)" }
        throw "HTTP ${status}: release request failed"
    }
    if ((Get-Item -LiteralPath $Output).Length -gt $Limit) { throw 'response exceeded byte limit' }
    if ($Head) { return $result[1] }
}

function Test-CandidateVersion {
    $stdout = Join-Path $Stage 'version.stdout'
    $stderr = Join-Path $Stage 'version.stderr'
    $process = Start-Process -FilePath $Candidate -ArgumentList '--version' -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    # Hold the native handle before polling; Windows PowerShell otherwise may
    # dispose it on Refresh and lose the exit code of a short-lived process.
    $handle = $process.Handle
    while (-not $process.WaitForExit(10)) {
        if ([DateTime]::UtcNow -ge $deadline -or (Get-Item $stdout).Length -gt 4096 -or (Get-Item $stderr).Length -gt 4096) {
            $process.Kill(); $process.WaitForExit(); throw 'candidate version timeout/oversized output'
        }
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0 -or (Get-Item $stdout).Length -gt 4096 -or (Get-Item $stderr).Length -gt 4096 -or (Get-Content -LiteralPath $stdout -Raw).Trim() -cne "mine $Version" -or (Get-Item $stderr).Length -ne 0) { throw "candidate native version does not match pinned release (exit=$($process.ExitCode), expected=$Version)" }
}

try {
    if ($Ref -eq 'latest') {
        $resolved = Get-ReleaseFile "https://github.com/$ReleaseAccount/$ReleaseRepo/releases/latest" (Join-Path $Stage 'latest') 1048576 -Head
        $prefix = "https://github.com/$ReleaseAccount/$ReleaseRepo/releases/tag/"
        if (-not $resolved.StartsWith($prefix)) { throw 'malformed latest release resolution' }
        $Tag = $resolved.Substring($prefix.Length)
    } else { $Tag = $Ref }
    if ($Tag -cnotmatch '^v?[0-9]{1,18}\.[0-9]{1,18}\.[0-9]{1,18}$') { throw 'malformed release tag' }
    $Version = $Tag -replace '^v',''
    $url = "https://github.com/$ReleaseAccount/$ReleaseRepo/releases/download/$Tag/$Asset"
    $archivePath = Join-Path $Stage $Asset
    Get-ReleaseFile $url $archivePath 134217728
    $sumPath = Join-Path $Stage 'checksum'
    Get-ReleaseFile "$url.sha256" $sumPath 4096
    $sumLines = @(Get-Content -LiteralPath $sumPath)
    if ($sumLines.Count -ne 1 -or $sumLines[0] -cnotmatch ('^([0-9a-fA-F]{64})\s+\*?'+[regex]::Escape($Asset)+'$')) { throw 'malformed/ambiguous checksum or wrong filename' }
    $expected = $Matches[1]
    if ((Get-Sha256 $archivePath) -ne $expected) { throw 'archive SHA-256 mismatch' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $selected = $null; [long]$total = 0
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName
            if ($name.Length -gt 1024 -or $name -match '^[\/]|[\\:\x00-\x1f\x7f]|(^|/)\.\.(/|$)') { throw 'unsafe archive path' }
            $normalized = ($name -split '/' | Where-Object {$_ -ne '' -and $_ -ne '.'}) -join '/'
            if (-not $seen.Add($normalized) -or $seen.Count -gt 10000) { throw 'duplicate/too many archive entries' }
            $kind = ($entry.ExternalAttributes -shr 16) -band 0xF000
            if ($kind -notin @(0,0x8000,0x4000) -or ($kind -eq 0x4000 -and -not $name.EndsWith('/'))) { throw 'archive links/special or inconsistent entries forbidden' }
            $total += $entry.Length
            if ($total -gt 536870912) { throw 'unpacked archive exceeds limit' }
            if ($entry.Name -ceq $BinaryName) {
                if ($null -ne $selected -or $entry.Length -eq 0 -or $entry.Length -gt 268435456) { throw 'ambiguous/empty/oversized binary' }
                $selected = $entry
            }
        }
        if ($null -eq $selected) { throw 'archive has no native binary' }
        $reader = $selected.Open(); $writer = [IO.File]::Open($Candidate,[IO.FileMode]::CreateNew)
        try {
            $buffer = New-Object byte[] 65536; [long]$count = 0
            while (($read = $reader.Read($buffer,0,$buffer.Length)) -gt 0) {
                $count += $read; if ($count -gt 268435456) { throw 'oversized extracted binary' }
                $writer.Write($buffer,0,$read)
            }
            if ($count -ne $selected.Length) { throw 'truncated native binary' }
            $writer.Flush($true)
        } finally { $reader.Dispose(); $writer.Dispose() }
    } finally { $zip.Dispose() }
    Test-CandidateVersion
    $candidateHash = Get-Sha256 $Candidate
    $Swapping = $true
    if (Test-Path -LiteralPath $Destination) { [IO.File]::Move($Destination,$Previous) }
    try { [IO.File]::Move($Candidate,$Destination) }
    catch {
        if (Test-Path -LiteralPath $Previous) { [IO.File]::Move($Previous,$Destination) }
        throw 'replacement failed; prior install restored; owned swap retained'
    }
    if ((Get-Sha256 $Destination) -ne $candidateHash) {
        [IO.File]::Move($Destination,$Candidate)
        if (Test-Path -LiteralPath $Previous) { [IO.File]::Move($Previous,$Destination) }
        throw 'installed binary verification failed; prior install restored; swap retained'
    }
    if (Test-Path -LiteralPath $Previous) {
        try { Remove-Item -LiteralPath $Previous -ErrorAction Stop }
        catch { throw "new binary installed; old backup cleanup pending at $Stage after the running process exits; inspect record.json before retry" }
    }
    Remove-Item -LiteralPath $Stage -Recurse -ErrorAction Stop
    $Swapping = $false
} finally {
    if (-not $Swapping -and (Test-Path -LiteralPath $Stage)) { Remove-Item -LiteralPath $Stage -Recurse -ErrorAction Stop }
}
if ($env:MINE_NO_PERSIST_PATH -ne '1') {
    $userPath = [Environment]::GetEnvironmentVariable('Path','User')
    if ($BinDirectory -notin ($userPath -split ';')) {
        [Environment]::SetEnvironmentVariable('Path',("$BinDirectory;$userPath"),'User')
    }
}
$env:PATH = "$BinDirectory;$env:PATH"
$env:MINE_REF = $Tag
Write-Host "mine $Tag binary installed; running Agent setup separately"
& $Destination setup @SetupArgs
if ($LASTEXITCODE -ne 0) { throw "binary installed; Agent setup failed with exit $LASTEXITCODE" }
