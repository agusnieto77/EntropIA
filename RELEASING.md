# Release workflow

EntropIA releases are intentionally gated: installers must not be published with fixture runtime packs.

## Quick path

1. Prepare or upload the real runtime payload source for each target platform.
2. Run **Runtime Payload** (`.github/workflows/runtime-payload.yml`) with:
   - `fixture=false`
   - `pack_version=<runtime pack version>`
   - `app_version=<EntropIA app version>`
   - `payload_source_artifact=<artifact with windows-x86_64/ and/or linux-x86_64 payload files>` when the payload comes from a previous run
   - or `payload_source_release_tag=<previous release tag>` + `platform=windows-x86_64` to reuse the runtime-pack from an existing Windows installer asset
3. Copy the Runtime Payload workflow **run ID**.
4. Run **Release** (`.github/workflows/release.yml`) via `workflow_dispatch` with:
   - `runtime_payload_artifact=runtime-payloads`
   - `runtime_payload_run_id=<run ID from step 3>`
   - `release_tag=<new release tag>` when dispatching from `main` instead of a tag ref
   - `release_platform=windows` or `all`
5. Review the draft release assets before publishing.

## Why tag pushes fail

Pushing `v*` tags still triggers the Release workflow, but it fails closed because tag events cannot provide `runtime_payload_artifact` and `runtime_payload_run_id` inputs.

That failure is expected. It prevents publishing an installer that accidentally bundles the small in-repo fixture runtime pack instead of the real self-contained payload.

## GitHub Actions version policy

Current workflow action versions are accepted as release infrastructure debt, not as a reproducibility guarantee.

- `tauri-apps/tauri-action@v0` is allowed temporarily because it is the existing release path.
- `actions/checkout@v6` stays in place unless CI/release evidence points to checkout-specific breakage.
- If release breaks without app/runtime-pack changes, first rollback candidates are:
  1. `actions/checkout@v6` → `actions/checkout@v4`
  2. `tauri-apps/tauri-action@v0` → a reviewed pinned SHA

Do not pin one action in isolation as a drive-by fix. If strict reproducibility becomes the priority, pin every release-critical action by SHA in one dedicated hardening change.

## Runtime payload contract

The runtime payload artifact must contain either a direct runtime-pack layout or one directory per platform:

```text
runtime-payloads/
├── windows-x86_64/
│   ├── manifest.overrides.json
│   ├── python/
│   ├── uv/
│   ├── scripts/
│   ├── wheelhouse/
│   ├── caches/
│   └── resources/lib/
└── linux-x86_64/
    └── ...
```

Windows release payloads must include both native runtime DLLs under `resources/lib/`:

- `pdfium.dll`
- `onnxruntime.dll`

Linux release payloads must include:

- `libpdfium.so`
- `libonnxruntime.so`

## Reusing a previous Windows release payload

For a Windows-only release candidate, the Runtime Payload workflow can extract the real runtime payload from an existing installer release asset:

```text
platform=windows-x86_64
fixture=false
payload_source_release_tag=v0.0.22
payload_source_release_asset=EntropIA_0.0.22_x64-setup.exe
pack_version=<new pack version>
app_version=<new app version>
```

This is only valid when the previous installer already contains the audited real runtime-pack. The workflow extracts `resources/runtime-pack/windows-x86_64`, overwrites repo-owned scripts from the current checkout, restamps manifest overrides, and then the Release workflow re-smokes the assembled pack before building the installer.

## Verification gates

The Release workflow:

- downloads the payload from the explicit `runtime_payload_run_id`;
- assembles the runtime pack in `apps/desktop/src-tauri/target/runtime-pack`;
- runs `runtime-pack-smoke.py --release --install-probe`;
- clears the bundled fixture directory before injecting the assembled runtime pack into Tauri resources.

If any of those steps fails, do not publish the installer.
