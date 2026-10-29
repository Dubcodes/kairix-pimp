# Local AI media analysis design

## Status and edition boundary

This is a design for a future PIMP Full milestone. No inference runtime, model or analysis worker ships in PIMP Light. Analysis is opt-in, offline and device-local; ingest, verification, browsing and manual metadata remain usable when the worker is absent, paused or failed. Generated evidence never changes original media or silently replaces manual metadata.

## Bounded pipeline

1. A persistent device-local job records the stable media ID, relative path, expected SHA-256, worker/model versions and resource policy.
2. FFprobe reads duration, streams and time bases. A bounded scene detector selects scene boundaries plus configurable periodic samples; long recordings are not decoded into an unbounded frame collection.
3. A local speech worker produces timestamped transcript segments. `whisper.cpp` is the initial packaging candidate, subject to current license, model-license and representative Windows benchmark review.
4. A small local vision-language worker captions representative frames or short scene groups. Model selection remains open until licensing, quantization, VRAM/RAM use and accuracy on production footage are measured.
5. A lightweight audio-event worker may add non-speech observations such as machinery, applause or alarms. These are probabilistic labels, not facts.
6. A small quantized local language model combines compact observations into a summary. It receives timestamps, captions, transcript segments and audio labels—not raw video frames.
7. Results are written atomically as versioned, rebuildable analysis records. Jobs are resumable at stage boundaries and yield to ingest, verification and interactive previews.

Scene sampling should include the first and last usable frames, scene-change representatives and a conservative periodic fallback. Sampling density is configurable and capped by both frames per hour and total decoded pixels. Significant long-duration changes therefore remain observable without treating every frame as model input.

## Structured evidence contract

```json
{
  "schemaVersion": 1,
  "mediaId": "stable-catalogue-id",
  "mediaSha256": "hex",
  "createdAt": "2026-10-30T00:00:00Z",
  "pipeline": {
    "workerVersion": "future-full-worker-version",
    "transcriptionModel": { "id": "model-id", "revision": "hash" },
    "visionModel": { "id": "model-id", "revision": "hash" },
    "audioModel": { "id": "model-id", "revision": "hash" },
    "summaryModel": { "id": "model-id", "revision": "hash" }
  },
  "durationSeconds": 1234.5,
  "observations": [
    {
      "id": "observation-id",
      "kind": "transcript|visual|audio-event|scene-boundary",
      "startSeconds": 42.1,
      "endSeconds": 48.7,
      "text": "A person appears to use a cutting power tool.",
      "labels": ["person", "power tool"],
      "confidence": 0.63,
      "uncertainty": "The tool may be a jigsaw; the frame is partially occluded.",
      "source": {
        "sampleFrame": 1263,
        "modelId": "model-id",
        "revision": "hash"
      }
    }
  ],
  "summary": {
    "text": "A long recording of workshop activity involving a person and a possible jigsaw.",
    "generatedFromObservationIds": ["observation-id"],
    "modelId": "summary-model-id",
    "revision": "hash"
  }
}
```

Observations are immutable evidence for one pipeline run. Summaries are separate projections and may be regenerated without decoding or transcribing the media again. Search indexes transcript text, visual/audio observations and summaries while retaining media ID plus start/end timecodes for direct playback navigation. Confidence is numeric but never displayed as certainty; ambiguous object/tool names retain explicit uncertainty text.

## Scheduling, storage and safety

- Jobs use the existing device-local persistent-job pattern with queued, running, paused, interrupted, failed and completed states.
- A resource governor limits concurrent decoders, CPU threads, RAM, VRAM and temporary disk. Interactive ingest, verification and foreground preview work has higher priority.
- Intermediate sampled frames and model caches are device-local, capacity-limited and rebuildable. Portable results contain relative media identity and model provenance, never absolute paths.
- Before and after each decode stage, the worker verifies the expected media identity/snapshot. A changed or unavailable file pauses for review.
- Cancellation occurs at bounded stage/sample boundaries. Completed transcript or observation shards remain reusable after restart.
- Light may read compatible result JSON but never imports AI runtimes or model dependencies.

## Phased recommendation

1. Define schema validation, persistent jobs, resource governance and timecode search using synthetic result fixtures.
2. Add transcription only, package and benchmark a small quantized model on representative Windows CPU/GPU hardware.
3. Add scene sampling and one licensed small vision model; measure coverage and false certainty before enabling summaries.
4. Add optional non-speech audio events and summary regeneration.
5. Run installed, long-recording, interruption, upgrade and model-migration acceptance before calling Full operational.

Indicative—not measured—resource targets are a 1–3 GB quantized model footprint per enabled worker, 4–8 GB available system RAM for CPU inference, optional 4–8 GB VRAM for accelerated vision, and a default of one decode/inference job at a time. Actual models, sampling intervals, throughput and minimum hardware must be chosen from repository-recorded benchmarks and license review, not these estimates.
