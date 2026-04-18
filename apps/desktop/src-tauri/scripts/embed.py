"""
EntropIA Embedding Engine — fastembed subprocess.

Called by the Rust backend via std::process::Command.
Receives text content, outputs embedding vector as JSON to stdout.

Uses paraphrase-multilingual-MiniLM-L12-v2 (384 dims, 50+ languages including Spanish).
This replaces the Rust fastembed crate which fails on Windows due to ORT/MSVC linker issues.

Usage:
    python embed.py --text "some text to embed" [--model paraphrase-multilingual-MiniLM-L12-v2]

Output (stdout): Sentinelled JSON — starts with EMBED_JSON_BEGIN\n and ends with \nEMBED_JSON_END
                  This allows the Rust side to extract clean JSON even if other
                  libraries write to stdout unexpectedly.
Errors (stderr): Human-readable error messages.
Exit codes: 0=success, 1=error
"""

import sys
import os
import json
import shutil
import warnings
import argparse
import io

# Suppress warnings that could corrupt JSON output on stdout
warnings.filterwarnings("ignore")

# Disable HuggingFace Hub symlink storage — Windows has issues resolving
# symlinks from MSI-installed app contexts (WinError 448: "untrusted mount point").
os.environ["HF_HUB_ENABLE_HF_TRANSFER"] = "0"
os.environ["HF_HUB_SYMLINK_STORAGE"] = "0"

# If --cache-dir is provided (from Rust), redirect the entire HuggingFace cache
# to that directory. This avoids the broken symlinks in the default
# ~/.cache/huggingface/ cache on Windows.
_pre_cache_dir = None
for _i, _arg in enumerate(sys.argv):
    if _arg == "--cache-dir" and _i + 1 < len(sys.argv):
        _pre_cache_dir = sys.argv[_i + 1]
        break

if _pre_cache_dir:
    os.makedirs(_pre_cache_dir, exist_ok=True)
    os.environ["HF_HUB_CACHE"] = _pre_cache_dir
    os.environ["HF_HOME"] = _pre_cache_dir

# Force stdout to be unbuffered so the Rust side sees output promptly
# and to avoid partial writes on crash.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", line_buffering=True)

SENTINEL_BEGIN = "===EMBED_JSON_BEGIN==="
SENTINEL_END = "===EMBED_JSON_END==="

# Default model — multilingual, 384 dims, supports Spanish
DEFAULT_MODEL = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"

# fastembed may resolve high-level model names to Qdrant ONNX repos internally.
# In MSI contexts we must seed the *actual* repo fastembed will load,
# otherwise it falls back to symlinked cache entries and fails.
FASTEMBED_REPO_MAP = {
    "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2": [
        "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2",
        "qdrant/paraphrase-multilingual-MiniLM-L12-v2-onnx-Q",
    ],
}


def _resolve_seed_repos(model_name: str) -> list[str]:
    repos = FASTEMBED_REPO_MAP.get(model_name)
    if repos:
        return repos
    return [model_name]


def _seed_model_cache(model_name: str, cache_dir: str) -> bool:
    """Pre-download model and seed into HuggingFace cache structure as regular files.

    On Windows MSI-installed app contexts, symlinks in the HuggingFace cache
    can't be read (WinError 448: "untrusted mount point"). This function
    downloads model files as regular files using snapshot_download(local_dir=...),
    then manually builds the HuggingFace cache structure (refs/main + snapshots/hash/)
    using regular file copies instead of symlinks.

    After seeding, TextEmbedding(model_name=..., cache_dir=cache_dir) will find
    the model in the cache with regular files — no symlinks to trip over.

    Returns True if the model was successfully seeded or already cached,
    False if caller should fall back to normal download.
    """
    try:
        from huggingface_hub import snapshot_download, model_info
    except ImportError:
        sys.stderr.write(
            "[embed] huggingface_hub not available for cache seeding, "
            "letting fastembed handle download\n"
        )
        return False

    # Determine cache directory for HuggingFace structure.
    # Use HF_HUB_CACHE env var if set (from --cache-dir), otherwise fall back
    # to the provided cache_dir (which is always a str in this function).
    env_cache = os.environ.get("HF_HUB_CACHE")
    hf_cache: str = env_cache if env_cache else cache_dir

    # Get the commit hash — needed to build the HuggingFace cache structure.
    # The cache expects: models--org--name/snapshots/{commit_hash}/
    try:
        info = model_info(model_name)
        commit_hash = info.sha
    except Exception as e:
        sys.stderr.write(f"[embed] Cannot get model info for cache seeding: {e}\n")
        return False

    if not commit_hash:
        sys.stderr.write("[embed] Model info returned no commit hash\n")
        return False

    model_cache_dir = os.path.join(hf_cache, f"models--{model_name.replace('/', '--')}")
    snapshot_dir = os.path.join(model_cache_dir, "snapshots", commit_hash)

    # Check if already cached with regular files (no symlinks)
    if os.path.isdir(snapshot_dir):
        # Accept either HuggingFace-style config or ONNX payload as signal.
        config_path = os.path.join(snapshot_dir, "config.json")
        onnx_path = os.path.join(snapshot_dir, "model_optimized.onnx")
        if (os.path.isfile(config_path) and not os.path.islink(config_path)) or (
            os.path.isfile(onnx_path) and not os.path.islink(onnx_path)
        ):
            sys.stderr.write(
                f"[embed] Model already cached as regular files: {snapshot_dir}\n"
            )
            return True

    # Download model files as regular files to a temporary directory.
    # snapshot_download(local_dir=...) creates regular file copies, not symlinks.
    temp_dir = os.path.join(hf_cache, "_temp_download_" + model_name.replace("/", "--"))
    if os.path.isdir(temp_dir):
        shutil.rmtree(temp_dir, ignore_errors=True)
    os.makedirs(temp_dir, exist_ok=True)

    try:
        sys.stderr.write(
            f"[embed] Downloading model '{model_name}' as regular files "
            f"(symlink-safe)...\n"
        )
        snapshot_download(model_name, local_dir=temp_dir)
    except Exception as e:
        sys.stderr.write(f"[embed] Download failed: {e}\n")
        shutil.rmtree(temp_dir, ignore_errors=True)
        return False

    # Build HuggingFace cache structure with regular file copies.
    #
    # HuggingFace cache expects this structure:
    #   models--org--name/
    #     refs/
    #       main              (contains commit hash)
    #     snapshots/
    #       {commit_hash}/
    #         config.json     (regular file, NOT symlink to blobs/)
    #         model.onnx      (regular file)
    #         tokenizer.json  (regular file)
    #         ...etc
    #
    # Normally, the files in snapshots/ are symlinks pointing to blobs/.
    # We place regular file copies instead, which avoids WinError 448.
    # The blobs/ directory is NOT needed — huggingface_hub reads files
    # from snapshots/ directly when they're regular files.
    os.makedirs(snapshot_dir, exist_ok=True)

    for item in os.listdir(temp_dir):
        # Skip .huggingface metadata and .cache directories
        if item.startswith("."):
            continue
        src = os.path.join(temp_dir, item)
        dst = os.path.join(snapshot_dir, item)
        if os.path.isdir(src):
            # Handle nested directories (e.g., ONNX model directories)
            shutil.copytree(src, dst, dirs_exist_ok=True)
        else:
            shutil.copy2(src, dst)

    # Create refs/main so huggingface_hub can find the correct snapshot
    refs_dir = os.path.join(model_cache_dir, "refs")
    os.makedirs(refs_dir, exist_ok=True)
    with open(os.path.join(refs_dir, "main"), "w", encoding="utf-8") as f:
        f.write(commit_hash)

    # Clean up temporary download directory
    shutil.rmtree(temp_dir, ignore_errors=True)

    sys.stderr.write(f"[embed] Model seeded into cache: {snapshot_dir}\n")
    return True


def main():
    parser = argparse.ArgumentParser(
        description="Compute text embedding using fastembed"
    )
    parser.add_argument(
        "--text",
        required=True,
        help="Text content to embed",
    )
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        help=f"Embedding model name (default: {DEFAULT_MODEL})",
    )
    parser.add_argument(
        "--cache-dir",
        default=None,
        help="Directory to cache HuggingFace models (avoids broken symlinks on Windows)",
    )
    args = parser.parse_args()

    text = args.text.strip()
    if not text:
        sys.stderr.write("Error: --text argument is empty after trimming.\n")
        sys.exit(1)

    # Determine cache directory for seeding.
    # If --cache-dir was provided, use it; otherwise use the current HF_HUB_CACHE
    # (which was set from --cache-dir at the top of this script) or fall back
    # to a sensible default.
    cache_dir = args.cache_dir
    if not cache_dir:
        cache_dir = os.environ.get("HF_HUB_CACHE")
    if not cache_dir:
        cache_dir = os.path.join(os.path.expanduser("~"), ".cache", "huggingface")

    try:
        from fastembed import TextEmbedding

        # Pre-download model(s) and seed into HuggingFace cache structure with
        # regular files (not symlinks). This avoids WinError 448 on Windows
        # MSI-installed app contexts where symlinks can't be read.
        #
        # IMPORTANT: fastembed may internally resolve `args.model` to a different
        # HuggingFace repo (e.g. qdrant/*-onnx-Q). We seed all known repos.
        seed_repos = _resolve_seed_repos(args.model)
        seeded = False
        for repo in seed_repos:
            ok = _seed_model_cache(repo, cache_dir)
            seeded = seeded or ok

        # Load the model. If cache was seeded, TextEmbedding will find the model
        # in the cache with regular files. If seeding failed, it falls back to
        # normal download (which may create symlinks and fail on Windows MSI).
        model_kwargs = {"model_name": args.model, "cache_dir": cache_dir}

        if seeded:
            sys.stderr.write(
                f"[embed] Loading model '{args.model}' from seeded cache "
                f"(symlink-safe)\n"
            )
        else:
            sys.stderr.write(
                f"[embed] Loading model '{args.model}' "
                f"(cache seeding unavailable, using normal download)\n"
            )

        model = TextEmbedding(**model_kwargs)

        sys.stderr.write(
            f"[embed] Computing embedding for {len(text)} chars of text...\n"
        )
        embeddings = list(model.embed([text]))

        if not embeddings:
            sys.stderr.write("Error: fastembed returned empty embeddings.\n")
            sys.exit(1)

        vector = embeddings[0].tolist()
        dim = len(vector)

        result = {
            "vector": vector,
            "dim": dim,
            "model": args.model,
        }

        json_str = json.dumps(result, ensure_ascii=False)
        sys.stdout.write(f"{SENTINEL_BEGIN}\n{json_str}\n{SENTINEL_END}\n")
        sys.stdout.flush()
        sys.stderr.write(f"[embed] Done: {dim} dimensions, model={args.model}\n")

    except ImportError:
        sys.stderr.write("Error: fastembed not installed. Run: pip install fastembed\n")
        sys.exit(1)
    except Exception as e:
        sys.stderr.write(f"Error: {e}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
