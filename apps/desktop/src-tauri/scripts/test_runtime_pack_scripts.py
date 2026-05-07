#!/usr/bin/env python3

from __future__ import annotations

import json
import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
BUILD_SCRIPT = SCRIPT_DIR / 'build_runtime_pack.py'
PREPARE_SCRIPT = SCRIPT_DIR / 'prepare_runtime_payload.py'
SMOKE_SCRIPT = SCRIPT_DIR / 'runtime-pack-smoke.py'
REPO_ROOT = SCRIPT_DIR.parents[3]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for chunk in iter(lambda: handle.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()


def write_fixture_file(root: Path, relative_path: str, content: str, executable: bool = False) -> dict:
    path = root / relative_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding='utf-8')
    if executable:
        path.chmod(path.stat().st_mode | 0o755)
    return {
        'path': relative_path,
        'sha256': sha256_file(path),
        'size': path.stat().st_size,
        'executable': executable or (os.name == 'nt' and path.suffix.lower() in {'.exe', '.bat', '.cmd'}),
    }


def create_fixture_root(root: Path, platform: str = 'linux-x86_64') -> Path:
    pack_root = root / platform
    python_entry = write_fixture_file(
        pack_root,
        'python/bin/python3',
        '#!/bin/sh\necho Python 3.12.0\n',
        executable=True,
    )
    uv_entry = write_fixture_file(
        pack_root,
        'uv/bin/uv',
        '#!/bin/sh\necho uv 0.6.14\n',
        executable=True,
    )
    script_entries = [
        write_fixture_file(pack_root, 'scripts/paddle_vl.py', 'print("paddle fixture")\n'),
        write_fixture_file(pack_root, 'scripts/transcribe.py', 'print("transcribe fixture")\n'),
        write_fixture_file(pack_root, 'scripts/embed.py', 'print("embed fixture")\n'),
    ]
    manifest = {
        'platform': platform,
        'pack_version': 'fixture-pack',
        'app_version': 'fixture-app',
        'payload_profile': 'fixture',
        'release_injection_required': True,
        'external_artifacts_required': ['release-payload'],
        'python_relpath': python_entry['path'],
        'uv_relpath': uv_entry['path'],
        'python_files': [python_entry],
        'uv_files': [uv_entry],
        'script_files': script_entries,
        'wheelhouse': [],
        'caches': [],
        'native_assets': [],
    }
    (pack_root / 'manifest.json').write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + '\n',
        encoding='utf-8',
    )
    return root


class RuntimePackScriptTests(unittest.TestCase):
    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, *args],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def test_fixture_build_mode_still_passes_for_ci_smoke(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture_root = create_fixture_root(Path(temp_dir) / 'fixtures')
            result = self.run_script(
                str(BUILD_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                str(Path(temp_dir) / 'out'),
                '--fixture-root',
                str(fixture_root),
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('linux-x86_64', result.stdout)

    def test_release_build_requires_payload_root(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            result = self.run_script(
                str(BUILD_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                temp_dir,
                '--require-release-payload',
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--require-release-payload requires --payload-root', result.stderr)

    def test_release_build_rejects_payload_without_overrides(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir) / 'out'
            payload_dir = Path(temp_dir) / 'payload' / 'linux-x86_64'
            fixture_root = create_fixture_root(Path(temp_dir) / 'fixtures')
            payload_dir.mkdir(parents=True)

            result = self.run_script(
                str(BUILD_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                str(output_dir),
                '--fixture-root',
                str(fixture_root),
                '--payload-root',
                str(payload_dir.parent),
                '--require-release-payload',
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn('must include manifest.overrides.json', result.stderr)

    def test_prepare_fixture_then_release_build_and_smoke_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            payload_root = root / 'payloads'
            pack_root = root / 'runtime-pack'
            fixture_root = create_fixture_root(root / 'fixtures')

            prepare_result = self.run_script(
                str(PREPARE_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                str(payload_root),
                '--pack-version',
                'test-pack',
                '--app-version',
                'test-app',
                '--fixture',
            )
            self.assertEqual(prepare_result.returncode, 0, prepare_result.stderr)
            self.assertTrue((payload_root / 'linux-x86_64' / 'PAYLOAD_FIXTURE_ONLY.txt').is_file())

            build_result = self.run_script(
                str(BUILD_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                str(pack_root),
                '--fixture-root',
                str(fixture_root),
                '--payload-root',
                str(payload_root),
                '--require-release-payload',
            )
            self.assertEqual(build_result.returncode, 0, build_result.stderr)

            smoke_result = self.run_script(
                str(SMOKE_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--root',
                str(pack_root),
                '--release',
            )
            self.assertEqual(smoke_result.returncode, 0, smoke_result.stderr)
            smoke_payload = json.loads(smoke_result.stdout)
            self.assertTrue(smoke_payload['ok'])
            self.assertEqual(smoke_payload['payload_profile'], 'release')
            self.assertEqual(smoke_payload['external_artifacts_required'], [])
            self.assertGreaterEqual(smoke_payload['entry_counts']['wheelhouse'], 6)

    def test_prepare_normal_mode_without_source_fails_honestly(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            result = self.run_script(
                str(PREPARE_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--output-dir',
                temp_dir,
                '--pack-version',
                'test-pack',
                '--app-version',
                'test-app',
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--payload-source-dir is required unless --fixture is used', result.stderr)

    def test_fixture_smoke_passes_but_release_smoke_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture_root = create_fixture_root(Path(temp_dir) / 'fixtures')
            fixture_result = self.run_script(
                str(SMOKE_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--root',
                str(fixture_root),
            )
            self.assertEqual(fixture_result.returncode, 0, fixture_result.stderr)
            fixture_payload = json.loads(fixture_result.stdout)
            self.assertTrue(fixture_payload['ok'])

            release_result = self.run_script(
                str(SMOKE_SCRIPT),
                '--platform',
                'linux-x86_64',
                '--root',
                str(fixture_root),
                '--release',
            )
            self.assertNotEqual(release_result.returncode, 0)
            release_payload = json.loads(release_result.stdout)
            self.assertFalse(release_payload['ok'])
            self.assertIn('release smoke cannot use fixture payload_profile', release_payload['release_errors'])


if __name__ == '__main__':
    unittest.main()
