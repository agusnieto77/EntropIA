#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import platform as host_platform
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[4]
TAURI_ROOT = REPO_ROOT / 'apps' / 'desktop' / 'src-tauri'
RUNTIME_PACK_ROOT = TAURI_ROOT / 'resources' / 'runtime-pack'
REQUIRED_RELEASE_SCRIPTS = {'scripts/paddle_vl.py', 'scripts/transcribe.py', 'scripts/embed.py'}
REQUIRED_RELEASE_WHEELS = {
    'paddleocr',
    'paddlepaddle',
    'faster_whisper',
    'spacy',
    'fastembed',
    'es_core_news_sm',
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for chunk in iter(lambda: handle.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()


def iter_manifest_entries(manifest: dict) -> list[dict]:
    entries = []
    for key in ('python_files', 'uv_files', 'script_files', 'wheelhouse', 'caches', 'native_assets'):
        entries.extend(manifest.get(key, []))
    return entries


def resolve_pack_root(root: Path, platform: str) -> Path:
    direct = root / 'manifest.json'
    if root.name == platform and direct.exists():
        return root
    return root / platform


def load_manifest(root: Path, platform: str) -> dict:
    manifest_path = resolve_pack_root(root, platform) / 'manifest.json'
    return json.loads(manifest_path.read_text(encoding='utf-8'))


def required_paths(manifest: dict) -> list[str]:
    return [
        manifest['python_relpath'],
        manifest['uv_relpath'],
        'scripts/paddle_vl.py',
        'scripts/transcribe.py',
        'scripts/embed.py',
    ]


def current_host_pack_platform() -> str | None:
    system = host_platform.system().lower()
    machine = host_platform.machine().lower()
    if machine not in {'x86_64', 'amd64'}:
        return None
    if system == 'linux':
        return 'linux-x86_64'
    if system == 'windows':
        return 'windows-x86_64'
    return None


def run_version_probe(executable: Path, expected_platform: str) -> dict:
    probe = {
        'path': str(executable),
        'host_compatible': current_host_pack_platform() == expected_platform,
        'attempted': False,
        'ok': None,
        'stdout': '',
        'stderr': '',
        'error': None,
    }
    if not probe['host_compatible']:
        return probe

    probe['attempted'] = True
    try:
        completed = subprocess.run(
            [str(executable), '--version'],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=20,
        )
    except Exception as exc:  # noqa: BLE001 - diagnostics should preserve any probe failure
        probe['ok'] = False
        probe['error'] = str(exc)
        return probe

    probe['ok'] = completed.returncode == 0
    probe['stdout'] = completed.stdout.strip()
    probe['stderr'] = completed.stderr.strip()
    return probe


def normalized_wheel_name(path: str) -> str:
    name = Path(path).name.lower().replace('-', '_')
    return name


def missing_release_wheels(manifest: dict) -> list[str]:
    wheel_names = [normalized_wheel_name(entry['path']) for entry in manifest.get('wheelhouse', [])]
    missing = []
    for required in sorted(REQUIRED_RELEASE_WHEELS):
        if required == 'paddlepaddle':
            if not any(name.startswith('paddlepaddle') for name in wheel_names):
                missing.append('paddlepaddle_or_paddlepaddle_gpu')
            continue
        if not any(name.startswith(required) for name in wheel_names):
            missing.append(required)
    return missing


def run_smoke(platform: str, root: Path, release: bool = False) -> dict:
    pack_root = resolve_pack_root(root, platform)
    manifest = load_manifest(root, platform)
    missing = [rel for rel in required_paths(manifest) if not (pack_root / rel).exists()]
    manifest_missing = []
    manifest_mismatched = []
    for entry in iter_manifest_entries(manifest):
        target = pack_root / entry['path']
        if not target.exists():
            manifest_missing.append(entry['path'])
            continue
        if target.stat().st_size != entry['size']:
            manifest_mismatched.append(f"size:{entry['path']}")
        if sha256_file(target) != entry['sha256']:
            manifest_mismatched.append(f"sha256:{entry['path']}")

    contract_errors = []
    python_relpath = manifest.get('python_relpath')
    uv_relpath = manifest.get('uv_relpath')
    if not python_relpath:
        contract_errors.append('missing python_relpath')
    if not uv_relpath:
        contract_errors.append('missing uv_relpath')
    if python_relpath not in {entry['path'] for entry in manifest.get('python_files', [])}:
        contract_errors.append('python_relpath missing from python_files')
    if uv_relpath not in {entry['path'] for entry in manifest.get('uv_files', [])}:
        contract_errors.append('uv_relpath missing from uv_files')

    release_errors = []
    missing_wheels = []
    version_probes = {}
    if release:
        if manifest.get('payload_profile') != 'release':
            release_errors.append('release smoke requires payload_profile=release')
        if manifest.get('payload_profile') == 'fixture':
            release_errors.append('release smoke cannot use fixture payload_profile')
        if manifest.get('release_injection_required') is not False:
            release_errors.append('release smoke requires release_injection_required=false')
        if manifest.get('external_artifacts_required') != []:
            release_errors.append('release smoke requires external_artifacts_required=[]')
        for key in ('python_files', 'uv_files', 'wheelhouse', 'script_files'):
            if not manifest.get(key):
                release_errors.append(f'release smoke requires non-empty {key}')
        script_paths = {entry['path'] for entry in manifest.get('script_files', [])}
        for script in sorted(REQUIRED_RELEASE_SCRIPTS):
            if script not in script_paths:
                release_errors.append(f'release smoke missing script_files entry: {script}')
        missing_wheels = missing_release_wheels(manifest)
        for wheel in missing_wheels:
            release_errors.append(f'release smoke missing wheelhouse package: {wheel}')
        if python_relpath and (pack_root / python_relpath).exists():
            version_probes['python'] = run_version_probe(pack_root / python_relpath, platform)
        if uv_relpath and (pack_root / uv_relpath).exists():
            version_probes['uv'] = run_version_probe(pack_root / uv_relpath, platform)
        for name, probe in version_probes.items():
            if probe['attempted'] and not probe['ok']:
                release_errors.append(f'{name} --version failed')

    if manifest.get('payload_profile') == 'release' and manifest.get('external_artifacts_required'):
        contract_errors.append('release pack still declares external_artifacts_required')

    return {
        'platform': platform,
        'root': str(pack_root),
        'release': release,
        'payload_profile': manifest.get('payload_profile'),
        'release_injection_required': manifest.get('release_injection_required'),
        'external_artifacts_required': manifest.get('external_artifacts_required', []),
        'entry_counts': {
            key: len(manifest.get(key, []))
            for key in ('python_files', 'uv_files', 'script_files', 'wheelhouse', 'caches', 'native_assets')
        },
        'missing': missing,
        'manifest_missing': manifest_missing,
        'manifest_mismatched': manifest_mismatched,
        'contract_errors': contract_errors,
        'release_errors': release_errors,
        'missing_release_wheels': missing_wheels,
        'version_probes': version_probes,
        'ok': not missing
        and not manifest_missing
        and not manifest_mismatched
        and not contract_errors
        and not release_errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description='Smoke-check EntropIA runtime-pack fixture structure.')
    parser.add_argument('--platform', required=True)
    parser.add_argument(
        '--root',
        default=str(RUNTIME_PACK_ROOT),
        help='Runtime-pack parent directory or the platform-specific assembled directory to inspect.',
    )
    parser.add_argument('--release', action='store_true', help='Enforce release payload hardening checks.')
    args = parser.parse_args()

    result = run_smoke(args.platform, Path(args.root), release=args.release)
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
