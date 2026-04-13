param(
  [Parameter(Mandatory = $true)]
  [string]$JobName,

  [string]$LockfilePath = "pnpm-lock.yaml",

  [string]$OutputRoot = ".ci-evidence/pnpm-preinstall",

  [string[]]$RequiredKeys = @(
    "node_version",
    "pnpm_version",
    "pnpm_exec_path",
    "store_dir",
    "virtual_store_dir",
    "sha256",
    "size_bytes",
    "line_count",
    "first_bytes_hex",
    "has_bom",
    "has_nul",
    "has_yaml_multidoc_separator"
  )
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-CommandOutput {
  param([Parameter(Mandatory = $true)][string]$Command)

  $result = Invoke-Expression $Command
  if ($null -eq $result) {
    return ""
  }

  return (($result | Out-String).Trim())
}

$repoRoot = (Resolve-Path -Path (Join-Path $PSScriptRoot "../../../../..")).Path

$resolvedLockfilePath = if ([System.IO.Path]::IsPathRooted($LockfilePath)) {
  (Resolve-Path -Path $LockfilePath).Path
} else {
  (Resolve-Path -Path (Join-Path $repoRoot $LockfilePath)).Path
}

$resolvedOutputRoot = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
  $OutputRoot
} else {
  Join-Path $repoRoot $OutputRoot
}

$jobOutputDir = Join-Path $resolvedOutputRoot $JobName

New-Item -Path $jobOutputDir -ItemType Directory -Force | Out-Null

$lockfileBytes = [System.IO.File]::ReadAllBytes($resolvedLockfilePath)
$sha256 = (Get-FileHash -Path $resolvedLockfilePath -Algorithm SHA256).Hash.ToLowerInvariant()
$lineCount = (Get-Content -Path $resolvedLockfilePath).Count

$firstBytesLength = [Math]::Min(32, $lockfileBytes.Length)
$firstBytesHex = if ($firstBytesLength -gt 0) {
  ($lockfileBytes[0..($firstBytesLength - 1)] | ForEach-Object { $_.ToString("x2") }) -join ""
} else {
  ""
}

$rawProbeLength = [Math]::Min(64, $lockfileBytes.Length)
$rawHeadBytes = if ($rawProbeLength -gt 0) {
  [byte[]]($lockfileBytes[0..($rawProbeLength - 1)])
} else {
  [byte[]]@()
}

$rawHeadHex = if ($rawProbeLength -gt 0) {
  ($rawHeadBytes | ForEach-Object { $_.ToString("x2") }) -join ""
} else {
  ""
}

$rawHeadPreview = if ($rawProbeLength -gt 0) {
  [System.Text.Encoding]::UTF8.GetString($rawHeadBytes)
} else {
  ""
}

$hasBom = $lockfileBytes.Length -ge 3 -and $lockfileBytes[0] -eq 0xEF -and $lockfileBytes[1] -eq 0xBB -and $lockfileBytes[2] -eq 0xBF
$hasNul = $lockfileBytes -contains 0
$lockfileText = Get-Content -Path $resolvedLockfilePath -Raw
$hasYamlMultidocSeparator = $lockfileText -match "(?m)^---\s*$"

$classificationHint = if ($hasBom -or $hasNul -or $hasYamlMultidocSeparator) { "lockfile_material" } else { "inconclusive" }

$evidence = [ordered]@{
  schema_version = "1"
  job = [ordered]@{
    name = $JobName
    runner_os = $env:RUNNER_OS
    runner_arch = $env:RUNNER_ARCH
    run_id = $env:GITHUB_RUN_ID
  }
  runtime = [ordered]@{
    node_version = (Get-CommandOutput -Command "node -v")
    pnpm_version = (Get-CommandOutput -Command "pnpm -v")
    pnpm_exec_path = (Get-Command pnpm).Source
  }
  tooling = [ordered]@{
    store_dir = (Get-CommandOutput -Command "pnpm config get store-dir")
    virtual_store_dir = (Get-CommandOutput -Command "pnpm config get virtual-store-dir")
  }
  lockfile = [ordered]@{
    path = $LockfilePath
    sha256 = $sha256
    size_bytes = $lockfileBytes.Length
    line_count = $lineCount
    first_bytes_hex = $firstBytesHex
    raw_head_probe_length = $rawProbeLength
    raw_head_bin_file = "lockfile-head-64.bin"
    raw_head_hex_file = "lockfile-head-64.hex.txt"
    raw_head_preview_file = "lockfile-head-64.preview.txt"
    has_bom = $hasBom
    has_nul = $hasNul
    has_yaml_multidoc_separator = $hasYamlMultidocSeparator
  }
  classification_hint = $classificationHint
  required_keys = $RequiredKeys
}

$jsonPath = Join-Path $jobOutputDir "preinstall-evidence.json"
$summaryPath = Join-Path $jobOutputDir "summary.md"
$rawHeadBinPath = Join-Path $jobOutputDir "lockfile-head-64.bin"
$rawHeadHexPath = Join-Path $jobOutputDir "lockfile-head-64.hex.txt"
$rawHeadPreviewPath = Join-Path $jobOutputDir "lockfile-head-64.preview.txt"

[System.IO.File]::WriteAllBytes($rawHeadBinPath, $rawHeadBytes)
$rawHeadHex | Set-Content -Path $rawHeadHexPath -Encoding utf8
$rawHeadPreview | Set-Content -Path $rawHeadPreviewPath -Encoding utf8

$evidence | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding utf8

$summary = @(
  "# PNPM pre-install evidence ($JobName)",
  "",
  "- runner_os: $($evidence.job.runner_os)",
  "- runner_arch: $($evidence.job.runner_arch)",
  "- run_id: $($evidence.job.run_id)",
  "- node_version: $($evidence.runtime.node_version)",
  "- pnpm_version: $($evidence.runtime.pnpm_version)",
  "- pnpm_exec_path: $($evidence.runtime.pnpm_exec_path)",
  "- store_dir: $($evidence.tooling.store_dir)",
  "- virtual_store_dir: $($evidence.tooling.virtual_store_dir)",
  "- lockfile.sha256: $($evidence.lockfile.sha256)",
  "- lockfile.size_bytes: $($evidence.lockfile.size_bytes)",
  "- lockfile.line_count: $($evidence.lockfile.line_count)",
  "- lockfile.first_bytes_hex: $($evidence.lockfile.first_bytes_hex)",
  "- lockfile.raw_head_probe_length: $($evidence.lockfile.raw_head_probe_length)",
  "- lockfile.raw_head_bin_file: $($evidence.lockfile.raw_head_bin_file)",
  "- lockfile.raw_head_hex_file: $($evidence.lockfile.raw_head_hex_file)",
  "- lockfile.raw_head_preview_file: $($evidence.lockfile.raw_head_preview_file)",
  "- lockfile.has_bom: $($evidence.lockfile.has_bom)",
  "- lockfile.has_nul: $($evidence.lockfile.has_nul)",
  "- lockfile.has_yaml_multidoc_separator: $($evidence.lockfile.has_yaml_multidoc_separator)",
  "- classification_hint: $($evidence.classification_hint)"
)

$summary -join [Environment]::NewLine | Set-Content -Path $summaryPath -Encoding utf8

Write-Host "PNPM pre-install evidence written to: $jobOutputDir"
