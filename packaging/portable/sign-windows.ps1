<#
.SYNOPSIS
  Authenticode-signs the Windows executables of a Delta release, only when signing secrets are present.

.DESCRIPTION
  Reads WINDOWS_SIGNING_CERT (Base64 PFX) and WINDOWS_SIGNING_PASSWORD from the environment.

  - Neither is set: logs that signing is skipped and returns. Nothing is signed and nothing is claimed.
  - Both are set: each file is signed with SHA-256 and an RFC 3161 timestamp, then checked with
    `signtool verify /pa /all`. Any failure throws, so an unverified signature never reaches a release.
  - Only one is set: throws. A half-configured secret pair is a mistake, not "signing not configured".

  The PFX is decoded into a temporary file and imported into the current user's certificate store so the
  password never appears on a command line; both are removed again in `finally`.

  -VerifyOnly checks signatures that are already there (used on the final extracted ZIP contents).
  See docs/operations/windows-code-signing.md.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path,
    [switch]$VerifyOnly
)
$ErrorActionPreference = "Stop"

$TimestampUrl = if ($env:WINDOWS_SIGNING_TIMESTAMP_URL) { $env:WINDOWS_SIGNING_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }

$hasCert = -not [string]::IsNullOrEmpty($env:WINDOWS_SIGNING_CERT)
$hasPassword = -not [string]::IsNullOrEmpty($env:WINDOWS_SIGNING_PASSWORD)

if (-not $hasCert -and -not $hasPassword) {
    Write-Host "code signing: skipped, WINDOWS_SIGNING_CERT and WINDOWS_SIGNING_PASSWORD are not set (build stays unsigned)" -ForegroundColor Yellow
    return
}
if ($hasCert -ne $hasPassword) {
    throw "code signing: WINDOWS_SIGNING_CERT and WINDOWS_SIGNING_PASSWORD must be set together; only one of them is present"
}

foreach ($file in $Path) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        throw "code signing: file not found: $file"
    }
}

function Find-SignTool {
    $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $found = @(
        Get-ChildItem -LiteralPath $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object FullName -Descending
    )
    if ($found.Count) { return $found[0].FullName }
    throw "code signing: signtool.exe was not found (install the Windows SDK)"
}

function Invoke-SignTool([string[]]$Arguments) {
    & $script:SignTool @Arguments
    if ($LASTEXITCODE -ne 0) { throw "signtool $($Arguments[0]) failed (exit $LASTEXITCODE)" }
}

$script:SignTool = Find-SignTool

if ($VerifyOnly) {
    foreach ($file in $Path) {
        Invoke-SignTool @("verify", "/pa", "/all", $file)
        Write-Host "code signing: verified $file" -ForegroundColor Green
    }
    return
}

$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$pfxPath = Join-Path $tempRoot ("delta-signing-" + [guid]::NewGuid().ToString("N") + ".pfx")
$thumbprint = $null
try {
    [System.IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:WINDOWS_SIGNING_CERT))
    $securePassword = ConvertTo-SecureString -String $env:WINDOWS_SIGNING_PASSWORD -AsPlainText -Force
    $imported = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $securePassword
    $thumbprint = $imported.Thumbprint

    foreach ($file in $Path) {
        $signed = $false
        for ($attempt = 1; $attempt -le 3 -and -not $signed; $attempt++) {
            try {
                Invoke-SignTool @("sign", "/sha1", $thumbprint, "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256", $file)
                $signed = $true
            }
            catch {
                if ($attempt -eq 3) { throw }
                Write-Host "code signing: attempt $attempt for $file failed (timestamp service?), retrying" -ForegroundColor Yellow
                Start-Sleep -Seconds (5 * $attempt)
            }
        }
        Invoke-SignTool @("verify", "/pa", "/all", $file)
        Write-Host "code signing: signed and verified $file" -ForegroundColor Green
    }
}
finally {
    if (Test-Path -LiteralPath $pfxPath) { Remove-Item -LiteralPath $pfxPath -Force }
    if ($thumbprint) { Remove-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint" -Force -ErrorAction SilentlyContinue }
}
