Set-StrictMode -Version Latest

$TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$workflowPath = Resolve-Path (Join-Path $TestRoot "../../../../..") | Join-Path -ChildPath ".github/workflows/ci.yml"

Describe "rust-quality-report workflow" {
  It "resolves workflow path robustly" {
    (Test-Path -Path $workflowPath) | Should Be $true
    $workflowPath | Should Match "[\\/]\.github[\\/]workflows[\\/]ci\.yml$"
  }

  It "installs rustfmt and clippy components" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "rustup component add llvm-tools-preview rustfmt clippy"
  }

  It "keeps cargo-llvm-cov installation step" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "Install cargo-llvm-cov"
  }

  It "runs Rust quality Pester suites explicitly in CI" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "Run Rust quality Pester suites"
    $content | Should Match "Invoke-Pester"
    $content | Should Match "rust-quality-contract.Tests.ps1"
    $content | Should Match "rust-verify-gate.Tests.ps1"
    $content | Should Match "ci-rust-quality-workflow.Tests.ps1"
  }

  It "uploads baseline coverage artifacts lcov and summary" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "Upload rust quality artifacts"
    $content | Should Match "coverage-rust/lcov.info"
    $content | Should Match "coverage-rust/summary.md"
  }
}
