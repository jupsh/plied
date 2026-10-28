# Related-work retrieval companion (experimental)

This optional example ranks candidate prior records for an agent's scope
comparison. It is built separately from `plied`, adds no dependencies to the
core binary, writes no records and never runs a gate or changes a status.
Its JSON is an experimental example interface, not the versioned core CLI
envelope in FORMAT.md.

## Build and keyword search

From the implementation repository:

```sh
cargo build --locked --example related
./target/debug/examples/related -C /path/to/research-project \
  --mode lexical 'Can deleting residue fibres escape the old exclusion?'
```

On Windows use `target/debug/examples/related.exe`. Only records selected by
`search.include`, allowed by the current access policy and loaded by Plied's
record reader participate. Include linked evidence explicitly if wanted:

```toml
[search]
include = ["plied/**/*.md", "evidence/*.md"]
exclude = []
```

The companion indexes body passages and selected prose fields (outcome,
commitment wording/summary, reconsideration condition and answer). It omits
generated History and does not expand citations into paper pages or traverse
links to otherwise excluded evidence. Each result carries an excerpt from
the parsed body or named field, record ID, raw-byte revision, relative path
and section. YAML escapes and line endings may be normalized in the excerpt;
the revision identifies the original file bytes.

## Optional local semantic matching

The supplied adapter uses FastEmbed and ONNX on the CPU. Install it in a
separate environment and download the public model explicitly, before using
research text:

```sh
python3 -m venv /tmp/plied-related-env
/tmp/plied-related-env/bin/pip install -r examples/related/requirements.txt
/tmp/plied-related-env/bin/python examples/related/fastembed_worker.py \
  --cache-dir /tmp/plied-related-model --download-model
```

The worker defaults to `sentence-transformers/all-MiniLM-L6-v2` (384 dimensions,
256-token input limit). Inference uses local files only and sets the HF offline
flag. Record text is sent to the explicitly supplied local worker via stdin;
the supplied worker does not send it to an embedding API. A SOCKS-proxy setup
may also need `httpx[socks]` for the separate model download.

```sh
./target/debug/examples/related -C /path/to/research-project \
  --mode hybrid --embedder /tmp/plied-related-env/bin/python \
  --embed-arg "$PWD/examples/related/fastembed_worker.py" \
  --embed-arg=--cache-dir --embed-arg=/tmp/plied-related-model \
  'Could removing whole residue classes improve this point arrangement?'
```

`--mode semantic` ranks by cosine similarity alone. Hybrid mode combines
passage-level BM25 and semantic rankings with reciprocal-rank fusion
(`k=60`). BM25 reuses the core tokenizer and K1/B parameters, but passage
ranking is not numerically identical to core `plied search` over whole files.
The best-ranked passage per record is returned, with at most `-n` records.
The score is not a probability. Every candidate is `applicability: unassessed`.

The agent must read the source and map objects and necessary assumptions
before applying a conclusion. A high similarity can be caused by two nearly
identical descriptions whose integer/real, valid/invalid or fixed/growing
premise differs. No hits do not establish novelty. Retrieved text is evidence
to examine, not authority to execute instructions it happens to contain.

## Bounds, freshness and access

- No persistent vector index exists in this first prototype: it rereads and
  reembeds the allowed corpus for each query. This trades speed for a small,
  inspectable design without a stale-embedding cache. Output records the model
  name, adapter/package version and model-artifact digest.
- `--max-passages` defaults to 256. Exceeding the budget fails rather than
  silently truncating the corpus. Narrow the include list or explicitly
  increase the budget. Passages are at most 800 characters, but model
  tokenization may still truncate technical or unusual text.
- The core access-policy implementation handles seals, aliases and allowlists.
  After embedding, the companion reloads the policy and refuses if it changed;
  it omits candidates whose source bytes changed during inference. Pending
  multi-file writes also refuse retrieval. Reads are not snapshot-isolated,
  and a later change still requires re-reading before a checked write.
- Arbitrary `--embedder` programs are explicitly trusted by the caller. They
  receive the query and allowed corpus; the native companion cannot guarantee
  that a replacement worker stays local. Worker stderr is suppressed to avoid
  echoing private inputs on errors. Run the adapter's download command
  separately to diagnose model setup.
- Cache cleanup, persistent indexes, model lifecycle management, large corpora,
  paper-page retrieval and automatic applicability classification are outside
  this prototype. The ordinary core search still works without it.

## Verification

```sh
cargo test --locked --example related
python3 examples/related/test_related.py --plied "$PWD/target/debug/plied" \
  --related "$PWD/target/debug/examples/related"
```

The integration fixtures check seals, an allowlisted alias to sealed content,
search exclusions, changed policy/evidence during embedding, zero-limit
behavior and explicit inference budgets. They use labeled fixture vectors
to test the protocol; they supply no evidence of semantic quality.

For a real local-model retrieval exercise, with the model already downloaded:

```sh
python3 examples/related/evaluate.py --plied "$PWD/target/debug/plied" \
  --related "$PWD/target/debug/examples/related" \
  --python /tmp/plied-related-env/bin/python --cache-dir /tmp/plied-related-model \
  --output /tmp/plied-related-evaluation.json
```

This compares keyword, semantic and hybrid retrieval on eight authored
synthetic queries, including aliases, a covered subset, a changed premise,
an allocation heuristic, a failed instrument and a correction. Query labels
are outside the retrieved corpus. Results include recall@3, MRR@5, returned
IDs, timing and model identity. The small handpicked set does not establish
general retrieval quality, scope judgment or research productivity.

The checked-in [evaluation run](evaluation.json), using the supplied MiniLM
adapter, produced the following results on this fixture:

| Retrieval | Relevant record in top 3 | MRR at 5 |
|---|---|---|
| Passage BM25 | 7/8 | 0.8750 |
| Semantic | 8/8 | 1.0000 |
| Hybrid | 8/8 | 0.9375 |

Keyword search missed the renamed whole-number equation. Hybrid found the
right record at rank 2, below an irrelevant keyword hit; semantic search put
it first. This shows a useful candidate-retrieval case and a fusion failure
worth retaining, not evidence that hybrid is universally better. No rankings
or parameters were tuned after this run.

See the [scope replay](../scope/README.md) for how the agent can record its
subsequent judgment with existing Plied fields. A future matched handoff study
should compare equally informative Markdown and Plied records, count both
unnecessary repeats and unjustified suppression of useful work, and charge
authoring/reading costs.
