#!/usr/bin/env python3
import json
import sys


def expandir(token):
    hijos = [h for h in token.children if h.dep_ in ["amod", "det", "compound", "flat"]]
    frase = sorted(hijos + [token], key=lambda x: x.i)
    return " ".join([w.text for w in frase]).strip()


def extraer_svo_espanol(doc):
    tripletes = []
    for token in doc:
        if token.pos_ != "VERB":
            continue
        sujeto = None
        objeto = None
        for hijo in token.children:
            if hijo.dep_ in ["nsubj", "nsubj:pass"]:
                sujeto = expandir(hijo)
            elif hijo.dep_ in ["obj", "iobj"]:
                objeto = expandir(hijo)
        if sujeto and objeto:
            tripletes.append(
                {
                    "subject": sujeto,
                    "predicate": token.text.strip(),
                    "object": objeto,
                }
            )
    return tripletes


def main():
    text = sys.stdin.read()
    if not text.strip():
        print("===TRIPLES_JSON_BEGIN===")
        print("[]")
        print("===TRIPLES_JSON_END===")
        return

    import spacy

    nlp = spacy.load("es_core_news_lg")
    doc = nlp(text)
    triples = extraer_svo_espanol(doc)
    print("===TRIPLES_JSON_BEGIN===")
    print(json.dumps(triples, ensure_ascii=False))
    print("===TRIPLES_JSON_END===")


if __name__ == "__main__":
    main()
