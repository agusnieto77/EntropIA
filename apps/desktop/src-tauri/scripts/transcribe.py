"""
EntropIA Transcription Engine — faster-whisper subprocess.

Called by the Rust backend via std::process::Command.
Receives an audio file path, outputs JSON to stdout.

Usage:
    python transcribe.py <audio_path> [--model base] [--language es] [--compute-type int8]

Output (stdout): Sentinelled JSON — starts with SENTINEL_BEGIN\n and ends with \nSENTINEL_END
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
# and to avoid partial writes on crash.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", line_buffering=True)

SENTINEL_BEGIN = "===TRANSCRIPTION_JSON_BEGIN==="
SENTINEL_END = "===TRANSCRIPTION_JSON_END==="


def main():
    parser = argparse.ArgumentParser(
        description="Transcribe audio using faster-whisper"
    )
    parser.add_argument("audio_path", help="Path to the audio file")
    parser.add_argument(
        "--model",
        default="base",
        help="Whisper model size (tiny, base, small, medium, large-v3)",
    )
    parser.add_argument(
        "--language",
        default="es",
        help="Language code (es, en, etc.) or 'auto' for detection",
    )
    parser.add_argument(
        "--compute-type",
        default="int8",
        help="Compute type: int8, int8_float16, float16, float32",
    )
    parser.add_argument(
        "--model-dir",
        default=None,
        help="Directory to cache/download models (default: ~/.cache/faster-whisper)",
    )
    args = parser.parse_args()

    try:
        from faster_whisper import WhisperModel

        # Configure model download directory if specified
        model_kwargs = {
            "device": "cpu",
            "compute_type": args.compute_type,
        }
        if args.model_dir:
            model_kwargs["download_root"] = args.model_dir

        # Load model — downloads on first run, cached afterward
        sys.stderr.write(
            f"[transcribe] Loading model '{args.model}' with compute_type={args.compute_type}\n"
        )
        model = WhisperModel(args.model, **model_kwargs)

        # Configure transcription
        language = args.language if args.language != "auto" else None
        transcribe_kwargs = {
            "language": language,
            "condition_on_previous_text": False,  # Avoid hallucination on long audio
            "vad_filter": True,  # Voice activity detection — skip silence
        }

        sys.stderr.write(f"[transcribe] Transcribing: {args.audio_path}\n")
        segments_iter, info = model.transcribe(args.audio_path, **transcribe_kwargs)

        # Collect segments
        result = []
        for segment in segments_iter:
            result.append(
                {
                    "start": round(segment.start, 3),
                    "end": round(segment.end, 3),
                    "text": segment.text.strip(),
                }
            )

        # Output JSON to stdout wrapped in sentinels for safe parsing.
        # The sentinels allow the Rust side to extract JSON even if other
        # libraries write spurious output to stdout.
        json_str = json.dumps(result, ensure_ascii=False)
        sys.stdout.write(f"{SENTINEL_BEGIN}\n{json_str}\n{SENTINEL_END}\n")
        sys.stdout.flush()
        sys.stderr.write(
            f"[transcribe] Done: {len(result)} segments, "
            f"language={info.language}, duration={info.duration:.1f}s\n"
        )

    except ImportError:
        sys.stderr.write(
            "Error: faster-whisper not installed. Run: pip install faster-whisper\n"
        )
        sys.exit(1)
    except Exception as e:
        sys.stderr.write(f"Error: {e}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
