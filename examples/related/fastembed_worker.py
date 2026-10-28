#!/usr/bin/env python3
"""Local-only embedding adapter. Model download is a separate explicit action."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys


def fingerprint(directory):
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        relative = path.relative_to(directory)
        if not path.is_file() or any(part.startswith(".") for part in relative.parts):
            continue
        name = relative.as_posix().encode()
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        digest.update(path.stat().st_size.to_bytes(8, "big"))
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", required=True)
    parser.add_argument("--model", default="sentence-transformers/all-MiniLM-L6-v2")
    parser.add_argument("--download-model", action="store_true")
    opts = parser.parse_args()
    if not opts.download_model:
        os.environ["HF_HUB_OFFLINE"] = "1"
    from fastembed import TextEmbedding

    # The pinned adapter uses this backend's model directory for artifact hashing.
    model = TextEmbedding(model_name=opts.model, cache_dir=opts.cache_dir, threads=2,
                          providers=["CPUExecutionProvider"], local_files_only=not opts.download_model)
    model_dir = Path(model.model._model_dir)
    metadata = {"name": opts.model, "fastembed": importlib.metadata.version("fastembed"),
                "artifacts_sha256": fingerprint(model_dir), "adapter": 1}
    if opts.download_model:
        print(json.dumps({"downloaded": True, "model": metadata}))
        return
    request = json.load(sys.stdin)
    if (request.get("v") != 1 or not isinstance(request.get("query"), str)
            or not isinstance(request.get("documents"), list)
            or any(not isinstance(s, str) for s in request["documents"])):
        raise ValueError("Invalid embedding request")
    query = next(model.query_embed(request["query"])).tolist()
    documents = [v.tolist() for v in model.passage_embed(request["documents"])]
    print(json.dumps({"model": metadata, "query": query, "documents": documents}, allow_nan=False))


if __name__ == "__main__":
    main()
