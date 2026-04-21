#!/usr/bin/env python3
# py -3.13 entorno_pruebas/bert_ner.py -i entorno_pruebas/prueba.txt -o entorno_pruebas/salida_ner.json
"""Ejecuta NER en español sobre un .txt usando mrm8488/bert-spanish-cased-finetuned-ner."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Iterable

from transformers import AutoTokenizer, pipeline

MODEL_NAME = "mrm8488/bert-spanish-cased-finetuned-ner"
ENCODINGS = ("utf-8", "utf-8-sig", "cp1252", "latin-1")


def read_text(path: Path) -> str:
    last_error: Exception | None = None
    for enc in ENCODINGS:
        try:
            text = path.read_text(encoding=enc)
            # Arreglo simple para texto mojibake típico (UTF-8 leído como latin-1/cp1252).
            if "Ã" in text:
                try:
                    repaired = text.encode("latin-1").decode("utf-8")
                    if repaired.count("Ã") < text.count("Ã"):
                        text = repaired
                except UnicodeError:
                    pass
            return text
        except UnicodeDecodeError as exc:
            last_error = exc
    raise RuntimeError(f"No pude leer {path} con codificaciones soportadas") from last_error


def paragraph_spans(text: str) -> Iterable[tuple[int, int]]:
    start = 0
    for m in re.finditer(r"\n\s*\n", text):
        end = m.start()
        if end > start:
            yield start, end
        start = m.end()
    if start < len(text):
        yield start, len(text)


def split_long_span(text: str, start: int, end: int, tokenizer, max_tokens: int) -> list[tuple[int, int]]:
    chunk = text[start:end]
    pieces = list(re.finditer(r"\S+\s*", chunk))
    if not pieces:
        return [(start, end)]

    spans: list[tuple[int, int]] = []
    current: list[re.Match[str]] = []
    current_tokens = 0

    for p in pieces:
        piece_text = p.group(0)
        piece_tokens = len(tokenizer.tokenize(piece_text))

        if current and current_tokens + piece_tokens > max_tokens:
            s = start + current[0].start()
            e = start + current[-1].end()
            spans.append((s, e))
            current = []
            current_tokens = 0

        current.append(p)
        current_tokens += piece_tokens

    if current:
        s = start + current[0].start()
        e = start + current[-1].end()
        spans.append((s, e))

    return spans


def build_chunks(text: str, tokenizer, max_tokens: int) -> list[tuple[int, int]]:
    chunks: list[tuple[int, int]] = []
    for start, end in paragraph_spans(text):
        span_text = text[start:end]
        token_count = len(tokenizer.tokenize(span_text))
        if token_count <= max_tokens:
            chunks.append((start, end))
        else:
            chunks.extend(split_long_span(text, start, end, tokenizer, max_tokens))
    return chunks


def _join_wordpiece(left: str, right: str) -> str:
    right_clean = right[2:] if right.startswith("##") else right
    return f"{left}{right_clean}"


def _should_merge_entities(prev: dict, curr: dict) -> bool:
    if prev.get("entity") != curr.get("entity"):
        return False

    curr_text = (curr.get("text") or "").strip()
    if curr_text.startswith("##"):
        return True

    prev_end = prev.get("end")
    curr_start = curr.get("start")
    if isinstance(prev_end, int) and isinstance(curr_start, int):
        return curr_start <= prev_end + 1

    return False


def collapse_entities(entities: list[dict]) -> list[dict]:
    if not entities:
        return []

    collapsed: list[dict] = []
    for ent in entities:
        text = ent.get("text") or ""
        if not collapsed:
            collapsed.append({**ent, "text": text})
            continue

        prev = collapsed[-1]
        if _should_merge_entities(prev, ent):
            prev_text = prev.get("text") or ""
            prev["text"] = _join_wordpiece(prev_text, text)
            prev["score"] = float((float(prev.get("score", 0.0)) + float(ent.get("score", 0.0))) / 2.0)
            prev["start"] = prev.get("start") if prev.get("start") is not None else ent.get("start")
            prev["end"] = ent.get("end") if ent.get("end") is not None else prev.get("end")
        else:
            collapsed.append({**ent, "text": text})

    return collapsed


def run_ner(text: str, device: int) -> list[dict]:
    try:
        tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, use_fast=True)
    except Exception:
        # Fallback por compatibilidad si no está tokenizers/fast tokenizer.
        tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, use_fast=False)
    nlp = pipeline(
        "ner",
        model=MODEL_NAME,
        tokenizer=tokenizer,
        aggregation_strategy="simple",
        device=device,
    )

    max_tokens = max(32, int(getattr(tokenizer, "model_max_length", 512)) - 2)
    chunks = build_chunks(text, tokenizer, max_tokens)

    entities: list[dict] = []
    for chunk_start, chunk_end in chunks:
        chunk_text = text[chunk_start:chunk_end]
        results = nlp(chunk_text)
        for ent in results:
            start = ent.get("start")
            end = ent.get("end")
            word = ent.get("word")
            if isinstance(start, int) and isinstance(end, int) and 0 <= start < end <= len(chunk_text):
                # Recupera texto exacto desde el original, incluso cuando el token sea [UNK].
                recovered_text = chunk_text[start:end]
            else:
                recovered_text = word
            entities.append(
                {
                    "entity": ent.get("entity_group", ent.get("entity")),
                    "score": float(ent.get("score", 0.0)),
                    "text": recovered_text,
                    "start": (chunk_start + start) if isinstance(start, int) else None,
                    "end": (chunk_start + end) if isinstance(end, int) else None,
                }
            )

    return entities


def main() -> None:
    parser = argparse.ArgumentParser(description="NER en español con BETO fine-tuned")
    parser.add_argument("-i", "--input", default="entorno_pruebas/prueba.txt", help="Ruta del .txt")
    parser.add_argument("-o", "--output", help="Ruta para guardar JSON (opcional)")
    parser.add_argument("--device", type=int, default=-1, help="-1 CPU, 0 GPU")
    args = parser.parse_args()

    input_path = Path(args.input)
    if not input_path.exists():
        raise FileNotFoundError(f"No existe el archivo: {input_path}")

    text = read_text(input_path)
    entities = run_ner(text, device=args.device)
    entities = collapse_entities(entities)

    payload = {
        "model": MODEL_NAME,
        "input_file": str(input_path),
        "entities": entities,
        "count": len(entities),
    }

    if args.output:
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"OK: resultado guardado en {output_path} ({len(entities)} entidades)")
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
