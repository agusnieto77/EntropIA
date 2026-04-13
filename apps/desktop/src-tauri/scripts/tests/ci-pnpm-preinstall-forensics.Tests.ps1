Set-StrictMode -Version Latest

Describe "ci pnpm pre-install forensics workflow contracts" {
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

    $script:TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
    $script:RepoRoot = (Resolve-Path -Path (Join-Path $script:TestRoot "../../../../..")).Path
    $script:workflowPath = Join-Path -Path $script:RepoRoot -ChildPath ".github/workflows/ci.yml"
    $script:workflow = Get-Content -Path $script:workflowPath -Raw
  }

  It "has lint-typecheck pre-install forensics step before install" {
    $jobIndex = $script:workflow.IndexOf("lint-typecheck:")
    $forensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (lint-typecheck)")
    $installIndex = $script:workflow.IndexOf("name: Install dependencies", $jobIndex)

    Assert-True -Condition ($jobIndex -ge 0) -Message "workflow must contain lint-typecheck job"
    Assert-True -Condition ($forensicsIndex -gt $jobIndex) -Message "workflow must include lint-typecheck pre-install forensic step"
    Assert-True -Condition ($installIndex -gt $forensicsIndex) -Message "lint-typecheck forensic step must be before install"
  }

  It "runs lint-typecheck post-checkout forensics before pnpm setup" {
    $jobIndex = $script:workflow.IndexOf("lint-typecheck:")
    $checkoutIndex = $script:workflow.IndexOf("- uses: actions/checkout@v6", $jobIndex)
    $postCheckoutForensicsIndex = $script:workflow.IndexOf("Run pnpm post-checkout forensics (lint-typecheck)", $jobIndex)
    $pnpmSetupIndex = $script:workflow.IndexOf("- uses: pnpm/action-setup@v6", $jobIndex)
    $nodeSetupIndex = $script:workflow.IndexOf("- uses: actions/setup-node@v6", $jobIndex)

    Assert-True -Condition ($jobIndex -ge 0) -Message "workflow must contain lint-typecheck job"
    Assert-True -Condition ($checkoutIndex -gt $jobIndex) -Message "lint-typecheck job must include checkout step"
    Assert-True -Condition ($postCheckoutForensicsIndex -gt $checkoutIndex) -Message "lint-typecheck post-checkout forensic step must run after checkout"
    Assert-True -Condition ($pnpmSetupIndex -gt $postCheckoutForensicsIndex) -Message "lint-typecheck post-checkout forensic step must run before pnpm/action-setup"
    Assert-True -Condition ($nodeSetupIndex -gt $postCheckoutForensicsIndex) -Message "lint-typecheck post-checkout forensic step must run before actions/setup-node"
  }

  It "uploads lint-typecheck post-checkout forensics artifact with distinct path" {
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-checkout forensics \(lint-typecheck\)" -Message "workflow must include lint-typecheck post-checkout forensics upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-checkout forensics \(lint-typecheck\)[\s\S]*?if:\s*always\(\)" -Message "lint-typecheck post-checkout upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-checkout forensics \(lint-typecheck\)[\s\S]*?\.ci-evidence/pnpm-preinstall/lint-typecheck-post-checkout/" -Message "lint-typecheck post-checkout upload must publish distinct post-checkout evidence path"
  }

  It "runs lint-typecheck post-pnpm-setup forensics between pnpm and setup-node" {
    $jobIndex = $script:workflow.IndexOf("lint-typecheck:")
    $pnpmSetupIndex = $script:workflow.IndexOf("- uses: pnpm/action-setup@v6", $jobIndex)
    $postPnpmForensicsIndex = $script:workflow.IndexOf("Run pnpm post-pnpm-setup forensics (lint-typecheck)", $jobIndex)
    $nodeSetupIndex = $script:workflow.IndexOf("- uses: actions/setup-node@v6", $jobIndex)

    Assert-True -Condition ($jobIndex -ge 0) -Message "workflow must contain lint-typecheck job"
    Assert-True -Condition ($pnpmSetupIndex -gt $jobIndex) -Message "lint-typecheck job must include pnpm/action-setup"
    Assert-True -Condition ($postPnpmForensicsIndex -gt $pnpmSetupIndex) -Message "lint-typecheck post-pnpm-setup forensics must run after pnpm/action-setup"
    Assert-True -Condition ($nodeSetupIndex -gt $postPnpmForensicsIndex) -Message "lint-typecheck post-pnpm-setup forensics must run before actions/setup-node"
  }

  It "uploads lint-typecheck post-pnpm-setup forensics artifact with distinct path" {
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-pnpm-setup forensics \(lint-typecheck\)" -Message "workflow must include lint-typecheck post-pnpm-setup upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-pnpm-setup forensics \(lint-typecheck\)[\s\S]*?if:\s*always\(\)" -Message "lint-typecheck post-pnpm-setup upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-pnpm-setup forensics \(lint-typecheck\)[\s\S]*?\.ci-evidence/pnpm-preinstall/lint-typecheck-post-pnpm-setup/" -Message "lint-typecheck post-pnpm-setup upload must publish distinct evidence path"
  }

  It "runs lint-typecheck post-setup-node forensics between setup-node and pre-install" {
    $jobIndex = $script:workflow.IndexOf("lint-typecheck:")
    $nodeSetupIndex = $script:workflow.IndexOf("- uses: actions/setup-node@v6", $jobIndex)
    $postNodeForensicsIndex = $script:workflow.IndexOf("Run pnpm post-setup-node forensics (lint-typecheck)", $jobIndex)
    $preInstallForensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (lint-typecheck)", $jobIndex)

    Assert-True -Condition ($jobIndex -ge 0) -Message "workflow must contain lint-typecheck job"
    Assert-True -Condition ($nodeSetupIndex -gt $jobIndex) -Message "lint-typecheck job must include actions/setup-node"
    Assert-True -Condition ($postNodeForensicsIndex -gt $nodeSetupIndex) -Message "lint-typecheck post-setup-node forensics must run after actions/setup-node"
    Assert-True -Condition ($preInstallForensicsIndex -gt $postNodeForensicsIndex) -Message "lint-typecheck post-setup-node forensics must run before pre-install forensics"
  }

  It "uploads lint-typecheck post-setup-node forensics artifact with distinct path" {
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-setup-node forensics \(lint-typecheck\)" -Message "workflow must include lint-typecheck post-setup-node upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-setup-node forensics \(lint-typecheck\)[\s\S]*?if:\s*always\(\)" -Message "lint-typecheck post-setup-node upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm post-setup-node forensics \(lint-typecheck\)[\s\S]*?\.ci-evidence/pnpm-preinstall/lint-typecheck-post-setup-node/" -Message "lint-typecheck post-setup-node upload must publish distinct evidence path"
  }

  It "uploads lint-typecheck forensics artifact with always policy" {
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(lint-typecheck\)" -Message "workflow must include lint-typecheck forensics upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(lint-typecheck\)[\s\S]*?if:\s*always\(\)" -Message "lint-typecheck forensics upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(lint-typecheck\)[\s\S]*?uses:\s*actions/upload-artifact@v7" -Message "lint-typecheck forensics upload must use actions/upload-artifact@v7"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(lint-typecheck\)[\s\S]*?\.ci-evidence/pnpm-preinstall/lint-typecheck/" -Message "lint-typecheck forensics upload must publish lint-typecheck evidence folder"
  }

  It "keeps frozen lockfile install command and forbids bypass flags" {
    Assert-Match -Value $script:workflow -Pattern "lint-typecheck:[\s\S]*?pnpm install --frozen-lockfile" -Message "lint-typecheck must keep pnpm install --frozen-lockfile"
    Assert-Match -Value $script:workflow -Pattern "rust-quality-report:[\s\S]*?pnpm install --frozen-lockfile" -Message "rust-quality-report must keep pnpm install --frozen-lockfile"
    Assert-Match -Value $script:workflow -Pattern "test:[\s\S]*?pnpm install --frozen-lockfile" -Message "test must keep pnpm install --frozen-lockfile"
    Assert-Match -Value $script:workflow -Pattern "build:[\s\S]*?pnpm install --frozen-lockfile" -Message "build must keep pnpm install --frozen-lockfile"

    Assert-True -Condition (-not ($script:workflow -match "--no-frozen-lockfile")) -Message "workflow must not include --no-frozen-lockfile"
    Assert-True -Condition (-not ($script:workflow -match "--fix-lockfile")) -Message "workflow must not include --fix-lockfile"
  }

  It "contains mandatory forensic evidence keys in workflow invocation" {
    Assert-Match -Value $script:workflow -Pattern "node_version" -Message "forensics contract must include node_version"
    Assert-Match -Value $script:workflow -Pattern "pnpm_version" -Message "forensics contract must include pnpm_version"
    Assert-Match -Value $script:workflow -Pattern "pnpm_exec_path" -Message "forensics contract must include pnpm_exec_path"
    Assert-Match -Value $script:workflow -Pattern "store_dir" -Message "forensics contract must include store_dir"
    Assert-Match -Value $script:workflow -Pattern "virtual_store_dir" -Message "forensics contract must include virtual_store_dir"
    Assert-Match -Value $script:workflow -Pattern "sha256" -Message "forensics contract must include lockfile sha256"
    Assert-Match -Value $script:workflow -Pattern "size_bytes" -Message "forensics contract must include lockfile size"
    Assert-Match -Value $script:workflow -Pattern "line_count" -Message "forensics contract must include lockfile line count"
    Assert-Match -Value $script:workflow -Pattern "first_bytes_hex" -Message "forensics contract must include lockfile first bytes sample"
    Assert-Match -Value $script:workflow -Pattern "has_bom" -Message "forensics contract must include BOM detection"
    Assert-Match -Value $script:workflow -Pattern "has_nul" -Message "forensics contract must include NUL detection"
    Assert-Match -Value $script:workflow -Pattern "has_yaml_multidoc_separator" -Message "forensics contract must include YAML multi-doc detection"
    Assert-Match -Value $script:workflow -Pattern "git_blob_sha256" -Message "forensics contract must include git blob sha256"
    Assert-Match -Value $script:workflow -Pattern "git_blob_first_bytes_hex" -Message "forensics contract must include git blob first-bytes sample"
    Assert-Match -Value $script:workflow -Pattern "lockfile_comparison_status" -Message "forensics contract must include lockfile comparison status"
  }

  It "runs optional pre-install classification before install in targeted jobs" {
    $lintForensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (lint-typecheck)")
    $lintClassificationIndex = $script:workflow.IndexOf("Run pnpm pre-install classification (lint-typecheck)")
    $lintInstallIndex = $script:workflow.IndexOf("name: Install dependencies", $script:workflow.IndexOf("lint-typecheck:"))

    $rustForensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (rust-quality-report)")
    $rustClassificationIndex = $script:workflow.IndexOf("Run pnpm pre-install classification (rust-quality-report)")
    $rustInstallIndex = $script:workflow.IndexOf("name: Install dependencies", $script:workflow.IndexOf("rust-quality-report:"))

    Assert-True -Condition ($lintForensicsIndex -ge 0) -Message "lint-typecheck forensics step must exist"
    Assert-True -Condition ($lintClassificationIndex -gt $lintForensicsIndex) -Message "lint-typecheck classification step must run after forensics"
    Assert-True -Condition ($lintInstallIndex -gt $lintClassificationIndex) -Message "lint-typecheck classification must run before install"

    Assert-True -Condition ($rustForensicsIndex -ge 0) -Message "rust-quality-report forensics step must exist"
    Assert-True -Condition ($rustClassificationIndex -gt $rustForensicsIndex) -Message "rust-quality-report classification step must run after forensics"
    Assert-True -Condition ($rustInstallIndex -gt $rustClassificationIndex) -Message "rust-quality-report classification must run before install"

    Assert-Match -Value $script:workflow -Pattern "classification\.md" -Message "forensics artifact bundle must include classification.md"
  }

  It "adds pre-install forensics and artifact upload to test and build jobs" {
    $testJobIndex = $script:workflow.IndexOf("test:")
    $testForensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (test)")
    $testInstallIndex = $script:workflow.IndexOf("name: Install dependencies", $testJobIndex)

    $buildJobIndex = $script:workflow.IndexOf("build:")
    $buildForensicsIndex = $script:workflow.IndexOf("Run pnpm pre-install forensics (build)")
    $buildInstallIndex = $script:workflow.IndexOf("name: Install dependencies", $buildJobIndex)

    Assert-True -Condition ($testJobIndex -ge 0) -Message "workflow must contain test job"
    Assert-True -Condition ($testForensicsIndex -gt $testJobIndex) -Message "workflow must include test pre-install forensic step"
    Assert-True -Condition ($testInstallIndex -gt $testForensicsIndex) -Message "test forensic step must be before install"

    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(test\)" -Message "workflow must include test forensics upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(test\)[\s\S]*?if:\s*always\(\)" -Message "test forensics upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(test\)[\s\S]*?\.ci-evidence/pnpm-preinstall/test/" -Message "test forensics upload must publish test evidence folder"

    Assert-True -Condition ($buildJobIndex -ge 0) -Message "workflow must contain build job"
    Assert-True -Condition ($buildForensicsIndex -gt $buildJobIndex) -Message "workflow must include build pre-install forensic step"
    Assert-True -Condition ($buildInstallIndex -gt $buildForensicsIndex) -Message "build forensic step must be before install"

    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(build\)" -Message "workflow must include build forensics upload step"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(build\)[\s\S]*?if:\s*always\(\)" -Message "build forensics upload must run with if: always()"
    Assert-Match -Value $script:workflow -Pattern "Upload pnpm pre-install forensics \(build\)[\s\S]*?\.ci-evidence/pnpm-preinstall/build/" -Message "build forensics upload must publish build evidence folder"
  }

  It "uses pwsh shell in Linux pre-install forensic/classification steps" {
    Assert-Match -Value $script:workflow -Pattern "Run pnpm pre-install forensics \(lint-typecheck\)[\s\S]*?shell:\s*pwsh" -Message "lint-typecheck forensics step must use pwsh on Linux"
    Assert-Match -Value $script:workflow -Pattern "Run pnpm pre-install classification \(lint-typecheck\)[\s\S]*?shell:\s*pwsh" -Message "lint-typecheck classification step must use pwsh on Linux"
    Assert-Match -Value $script:workflow -Pattern "Run pnpm pre-install forensics \(test\)[\s\S]*?shell:\s*pwsh" -Message "test forensics step must use pwsh on Linux"
    Assert-Match -Value $script:workflow -Pattern "Run pnpm pre-install forensics \(build\)[\s\S]*?shell:\s*pwsh" -Message "build forensics step must use pwsh on Linux"
  }
}
