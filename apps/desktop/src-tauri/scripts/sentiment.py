"""
EntropIA Sentiment Analysis — pysentimiento subprocess.

Called by the Rust backend via std::process::Command.
Receives a JSON array of texts, outputs sentiment/emotion/hate-speech analysis as JSON.

Uses pysentimiento (HuggingFace transformers) optimized for Spanish social media.

Usage:
    python sentiment.py --input texts.json [--tasks sentiment,emotion,hate_speech] [--cache-dir /path]

Input JSON format: [{"id": "entry-uuid", "text": "..."}, ...]
Output (stdout): Sentinelled JSON — SENTIMENT_JSON_BEGIN / SENTIMENT_JSON_END

Exit codes: 0=success, 1=error
"""

import sys
import os
import json
import warnings
import argparse
import io

# Suppress warnings that could corrupt JSON output
warnings.filterwarnings("ignore")
os.environ["HF_HUB_ENABLE_HF_TRANSFER"] = "0"
os.environ["HF_HUB_SYMLINK_STORAGE"] = "0"
os.environ["TOKENIZERS_PARALLELISM"] = "false"

_pre_cache_dir = None
for _i, _arg in enumerate(sys.argv):
    if _arg == "--cache-dir" and _i + 1 < len(sys.argv):
        _pre_cache_dir = sys.argv[_i + 1]
        break

if _pre_cache_dir:
    os.environ["HF_HOME"] = _pre_cache_dir
    os.environ["HUGGINGFACE_HUB_CACHE"] = os.path.join(_pre_cache_dir, "hub")
    os.environ["TRANSFORMERS_CACHE"] = os.path.join(_pre_cache_dir, "hub")

BEGIN_SENTINEL = "===SENTIMENT_JSON_BEGIN==="
END_SENTINEL = "===SENTIMENT_JSON_END==="


def analyze_batch(entries, tasks):
    """Run pysentimiento analysis on a batch of entries."""
    from pysentimiento import create_analyzer

    analyzers = {}
    for task in tasks:
        try:
            analyzers[task] = create_analyzer(task=task, lang="es")
        except Exception as e:
            print(f"Warning: could not create analyzer for {task}: {e}", file=sys.stderr)

    results = []
    for entry in entries:
        entry_id = entry["id"]
        text = entry["text"]
        entry_result = {"id": entry_id}

        for task, analyzer in analyzers.items():
            try:
                output = analyzer.predict(text)
                entry_result[task] = {
                    "output": output.output,
                    "probas": {k: round(v, 4) for k, v in output.probas.items()},
                }
            except Exception as e:
                entry_result[task] = {"error": str(e)}

        results.append(entry_result)

    return results


def main():
    parser = argparse.ArgumentParser(description="Sentiment analysis via pysentimiento")
    parser.add_argument("--input", required=True, help="Path to JSON file with entries")
    parser.add_argument(
        "--tasks",
        default="sentiment,emotion,hate_speech",
        help="Comma-separated list of tasks (sentiment, emotion, hate_speech, irony)",
    )
    parser.add_argument("--cache-dir", help="HuggingFace cache directory")
    args = parser.parse_args()

    # Redirect stdout to capture any stray prints from libraries
    real_stdout = sys.stdout
    sys.stdout = io.StringIO()

    try:
        with open(args.input, "r", encoding="utf-8") as f:
            entries = json.load(f)

        tasks = [t.strip() for t in args.tasks.split(",") if t.strip()]
        results = analyze_batch(entries, tasks)

        sys.stdout = real_stdout
        print(BEGIN_SENTINEL)
        print(json.dumps(results, ensure_ascii=False))
        print(END_SENTINEL)

    except Exception as e:
        sys.stdout = real_stdout
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
