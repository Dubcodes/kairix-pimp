# Future lossless trimming design

Lossless stream-copy trimming is normally constrained by codec keyframes. It cannot promise arbitrary-frame precision without re-encoding boundary frames or the complete output. PIMP must present that limitation before a user approves any future trimming operation.

A future reviewed workflow may let the editor preview keep/remove ranges, but it must preserve every original and write each result as a new derived file. The derived file must be independently verified, linked to its source through portable provenance, and never replace the source name. When a logical recording includes an attached DJI WAV, the proposed ranges and resulting provenance must keep the audio relationship synchronized rather than trimming only one physical file.

Before storage can be reclaimed, PIMP must check all project dependencies, verify a complete archive and an independent external backup, and require explicit user authorization. Originals must never be deleted automatically. Any later cleanup design must use recoverable quarantine with restore evidence and remain separate from preview or trimming generation.
