Set-StrictMode -Version Latest

$workflowPath = Resolve-Path (Join-Path $PSScriptRoot "../../../../../.github/workflows/ci.yml")

Describe "rust-quality-report workflow" {
  It "installs rustfmt and clippy components" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "rustup component add llvm-tools-preview rustfmt clippy"
  }

  It "keeps cargo-llvm-cov installation step" {
    $content = Get-Content -Path $workflowPath -Raw

    $content | Should Match "Install cargo-llvm-cov"
  }
}
