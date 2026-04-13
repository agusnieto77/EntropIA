Set-StrictMode -Version Latest

Describe "pnpm pre-install forensics script raw probe" {
  BeforeAll {
    function Assert-True {
      param(
        [bool]$Condition,
        [string]$Message
      )

      if (-not $Condition) {
        throw $Message
      }
    }

    $script:TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
    $script:RepoRoot = (Resolve-Path -Path (Join-Path $script:TestRoot "../../../../..")).Path
    $script:ForensicsPath = Join-Path -Path $script:RepoRoot -ChildPath "apps/desktop/src-tauri/scripts/ci/pnpm-preinstall-forensics.ps1"
    $script:LockfilePath = Join-Path -Path $script:RepoRoot -ChildPath "pnpm-lock.yaml"
  }

  It "emits raw lockfile head probe files next to evidence json" {
    $fixtureRoot = Join-Path -Path $env:TEMP -ChildPath ("pnpm-preinstall-raw-" + [guid]::NewGuid().ToString("N"))
    New-Item -Path $fixtureRoot -ItemType Directory -Force | Out-Null

    $outputRoot = Join-Path -Path $fixtureRoot -ChildPath ".ci-evidence/pnpm-preinstall"
    & $script:ForensicsPath -JobName "raw-probe" -LockfilePath $script:LockfilePath -OutputRoot $outputRoot

    $jobDir = Join-Path -Path $outputRoot -ChildPath "raw-probe"
    $jsonPath = Join-Path -Path $jobDir -ChildPath "preinstall-evidence.json"
    $rawHeadBinPath = Join-Path -Path $jobDir -ChildPath "lockfile-head-64.bin"
    $rawHeadHexPath = Join-Path -Path $jobDir -ChildPath "lockfile-head-64.hex.txt"
    $rawHeadPreviewPath = Join-Path -Path $jobDir -ChildPath "lockfile-head-64.preview.txt"

    Assert-True -Condition (Test-Path -Path $jsonPath) -Message "script must create preinstall-evidence.json"
    Assert-True -Condition (Test-Path -Path $rawHeadBinPath) -Message "script must emit binary head probe"
    Assert-True -Condition (Test-Path -Path $rawHeadHexPath) -Message "script must emit hex head probe"
    Assert-True -Condition (Test-Path -Path $rawHeadPreviewPath) -Message "script must emit text preview head probe"
  }

  It "keeps raw probe bytes consistent between bin file and hex file" {
    $fixtureRoot = Join-Path -Path $env:TEMP -ChildPath ("pnpm-preinstall-raw-" + [guid]::NewGuid().ToString("N"))
    New-Item -Path $fixtureRoot -ItemType Directory -Force | Out-Null

    $customLockfilePath = Join-Path -Path $fixtureRoot -ChildPath "probe-lock.yaml"
    [System.IO.File]::WriteAllBytes($customLockfilePath, [byte[]](0x2d, 0x2d, 0x2d, 0x0a, 0x23, 0x20, 0x70, 0x72, 0x6f, 0x62, 0x65))

    $outputRoot = Join-Path -Path $fixtureRoot -ChildPath ".ci-evidence/pnpm-preinstall"
    & $script:ForensicsPath -JobName "raw-probe-custom" -LockfilePath $customLockfilePath -OutputRoot $outputRoot

    $jobDir = Join-Path -Path $outputRoot -ChildPath "raw-probe-custom"
    $rawHeadBinPath = Join-Path -Path $jobDir -ChildPath "lockfile-head-64.bin"
    $rawHeadHexPath = Join-Path -Path $jobDir -ChildPath "lockfile-head-64.hex.txt"

    $binBytes = [System.IO.File]::ReadAllBytes($rawHeadBinPath)
    $hexFromBin = ($binBytes | ForEach-Object { $_.ToString("x2") }) -join ""
    $hexFromFile = (Get-Content -Path $rawHeadHexPath -Raw).Trim()

    Assert-True -Condition ($hexFromBin.Length -gt 0) -Message "binary head probe must contain bytes"
    Assert-True -Condition ($hexFromBin -eq $hexFromFile) -Message "hex probe must match binary head probe bytes"
  }
}
