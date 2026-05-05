import argparse
import json
import sys

BEGIN = "===NER_JSON_BEGIN==="
END = "===NER_JSON_END==="


def sanitize_surrogates(text: str) -> str:
    """Remove lone surrogates that crash spaCy's tokenizer.

    OCR-extracted text from corrupted PDFs/images may contain lone surrogates
    (e.g., \\udc9d) which cause UnicodeEncodeError: 'utf-8' codec can't encode
    character: surrogates not allowed.  Replacing with '?' preserves text length
    so character offsets remain stable for downstream processing.
    """
    return text.encode("utf-8", errors="replace").decode("utf-8")


def map_label(label: str) -> str | None:
    label = label.upper()
    if label in {"PER", "PERSON"}:
        return "PER"
    if label in {"LOC", "GPE"}:
        return "LOC"
    if label == "ORG":
        return "ORG"
    if label in {"DATE", "TIME"}:
        return "DATE"
    if label == "MISC":
        return "MISC"
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--text", required=True)
    parser.add_argument("--model", default="es_core_news_sm")
    args = parser.parse_args()

    # Sanitize lone surrogates before tokenizing — see sanitize_surrogates()
    sanitized_text = sanitize_surrogates(args.text)

    import spacy

    nlp = spacy.load(args.model, disable=["parser", "lemmatizer", "attribute_ruler", "tagger"])
    doc = nlp(sanitized_text)

    entities = []
    for ent in doc.ents:
        mapped = map_label(ent.label_)
        if mapped is None:
            continue
        entities.append(
            {
                "entity_type": mapped,
                "value": ent.text,
                "start_offset": ent.start_char,
                "end_offset": ent.end_char,
            }
        )

    payload = {"model": args.model, "entities": entities}
    sys.stdout.write(BEGIN + json.dumps(payload, ensure_ascii=False) + END)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
