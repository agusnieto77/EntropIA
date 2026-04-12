Set-StrictMode -Version Latest

$TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$RepoRoot = (Resolve-Path -Path (Join-Path $TestRoot "../../../../..")).Path
$workflowPath = Join-Path -Path $RepoRoot -ChildPath ".github/workflows/ci.yml"

function Assert-True {
  param(
    [bool]$Condition,
    [string]$Message
  )

  if (-not $Condition) {
    throw $Message
  }
}

function Assert-Match {
  param(
    [string]$Value,
    [string]$Pattern,
    [string]$Message
  )

  if ($Value -notmatch $Pattern) {
    throw $Message
  }
}

Describe "rust-quality-report workflow" {
  It "resolves workflow path robustly" {
    Assert-True -Condition (-not [string]::IsNullOrWhiteSpace($workflowPath)) -Message "workflowPath should not be null or empty"
    Assert-True -Condition (Test-Path -Path $workflowPath) -Message "workflowPath should exist"
    Assert-Match -Value $workflowPath -Pattern "[\\/]\.github[\\/]workflows[\\/]ci\.yml$" -Message "workflowPath should target .github/workflows/ci.yml"
  }

  It "installs rustfmt and clippy components" {
    $content = Get-Content -Path $workflowPath -Raw

    Assert-Match -Value $content -Pattern "rustup component add llvm-tools-preview rustfmt clippy" -Message "workflow must install llvm-tools-preview rustfmt clippy"
  }

  It "keeps cargo-llvm-cov installation step" {
    $content = Get-Content -Path $workflowPath -Raw

    Assert-Match -Value $content -Pattern "Install cargo-llvm-cov" -Message "workflow must keep cargo-llvm-cov installation step"
  }

  It "runs Rust quality Pester suites explicitly in CI" {
    $content = Get-Content -Path $workflowPath -Raw

    Assert-Match -Value $content -Pattern "Run Rust quality Pester suites" -Message "workflow must contain explicit Pester suites step"
    Assert-Match -Value $content -Pattern "Invoke-Pester" -Message "workflow must invoke Pester explicitly"
    Assert-Match -Value $content -Pattern "rust-quality-contract.Tests.ps1" -Message "workflow must run rust-quality-contract suite"
    Assert-Match -Value $content -Pattern "rust-verify-gate.Tests.ps1" -Message "workflow must run rust-verify-gate suite"
    Assert-Match -Value $content -Pattern "ci-rust-quality-workflow.Tests.ps1" -Message "workflow must run workflow contract suite"
  }

  It "uploads baseline coverage artifacts lcov and summary" {
    $content = Get-Content -Path $workflowPath -Raw

    Assert-Match -Value $content -Pattern "Upload rust quality artifacts" -Message "workflow must upload rust quality artifacts"
    Assert-Match -Value $content -Pattern "coverage-rust/lcov.info" -Message "workflow artifacts must include lcov.info"
    Assert-Match -Value $content -Pattern "coverage-rust/summary.md" -Message "workflow artifacts must include summary.md"
  }
}
