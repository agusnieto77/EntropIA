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
import json
import warnings
import argparse
import io

# Suppress warnings that could corrupt JSON output on stdout
warnings.filterwarnings("ignore")

# Force stdout to be unbuffered so the Rust side sees output promptly
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", line_buffering=True)

SENTINEL_BEGIN = "===EMBED_JSON_BEGIN==="
SENTINEL_END = "===EMBED_JSON_END==="

# Default model — multilingual, 384 dims, supports Spanish
DEFAULT_MODEL = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"


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
    args = parser.parse_args()

    text = args.text.strip()
    if not text:
        sys.stderr.write("Error: --text argument is empty after trimming.\n")
        sys.exit(1)

    try:
        from fastembed import TextEmbedding

        sys.stderr.write(f"[embed] Loading model '{args.model}'...\n")
        model = TextEmbedding(model_name=args.model)

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
