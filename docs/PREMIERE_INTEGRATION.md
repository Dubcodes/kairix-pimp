# Premiere Companion integration

## Status and evidence boundary

`apps/premiere-companion` is a standalone **Kairix PIMP — Premiere Companion** UXP proof of concept. PIMP exports a narrowly scoped, versioned JSON manifest for the current physical Library selection. The panel lets the user choose that manifest, choose or create a target Premiere bin, preserve the actual PIMP folder hierarchy, recursively inspect the whole project for already imported paths, and report imported, skipped, failed, created-bin and reused-bin counts. The default hierarchy starts at `PIMP Media`, so a recording stored in `20260809` is imported under `PIMP Media / 20260809` without moving existing project items.

The interchange validator and PIMP exporter are automated-test covered. On the 2026-10-29 development machine, Adobe Premiere Pro 26.0.2 is installed, but UXP Developer Tool is not installed. Adobe's documented installation requires Creative Cloud sign-in and administrator elevation, so unattended installation was not attempted. The panel has therefore **not** been loaded into Premiere, packaged as a `.ccx`, or accepted against a real project. Do not describe this checkpoint as tested Premiere compatibility.

## Supported API baseline

The manifest declares Premiere `25.6.0` as its minimum host because Adobe documents the Premiere UXP API, `Project.getActiveProject`, `Project.importFiles`, `FolderItem.getItems`/`createBinAction`, and `ClipProjectItem.getMediaFilePath` from 25.6. Bin actions are created inside `Project.lockedAccess` and committed through `Project.executeTransaction`.

Primary official references:

- [Premiere UXP introduction](https://developer.adobe.com/premiere-pro/uxp/introduction/)
- [Project API](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/project)
- [FolderItem API](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/folderitem)
- [ClipProjectItem API](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/clipprojectitem)
- [UXP filesystem permissions](https://developer.adobe.com/premiere-pro/uxp/resources/recipes/filesystem-operations/)
- [Package a UXP plugin](https://developer.adobe.com/premiere-pro/uxp/plugins/distribution/package/)
- [Install a UXP plugin](https://developer.adobe.com/premiere-pro/uxp/plugins/distribution/install/)
- [Adobe Premiere UXP samples](https://github.com/AdobeDocs/uxp-premiere-pro-samples)
- [Official import sample](https://github.com/AdobeDocs/uxp-premiere-pro-samples/blob/main/sample-panels/premiere-api/src/import.ts)
- [Official project-panel sample](https://github.com/AdobeDocs/uxp-premiere-pro-samples/blob/main/sample-panels/premiere-api/src/projectPanel.ts)
- [UXP network operations](https://developer.adobe.com/premiere-pro/uxp/resources/recipes/network/)
- [Microsoft Shell drag-and-drop](https://learn.microsoft.com/windows/win32/shell/dragdrop)
- [Microsoft Shell clipboard formats](https://learn.microsoft.com/windows/win32/shell/clipboard)

The Adobe sample repository has an Apache-2.0 top-level licence, while individual current sample files also carry Adobe-specific notices. Kairix uses the documented API shape and an original implementation rather than copying sample source. The legacy CEP samples were reviewed only as historical architecture; UXP remains the supported direction.

## Interchange safety

The schema identifier is `kairix-pimp-premiere-import`, version 1. Every item contains its stable media ID, canonical device-local absolute path, portable Master Bin relative path, original filename, SHA-256, file type, date/package-relative bin path and optional attached-audio parent ID.

The native exporter:

- accepts only known catalogue IDs and at most 5,000 physical files;
- canonicalizes every path beneath the configured Master Bin and requires the file to be locally available;
- includes attached DJI WAV files when they are part of the logical selection;
- creates a new `.json` file and refuses to replace an existing file;
- reads source evidence but never writes, renames or copies media.

The plugin requests access only to the user-selected manifest. It validates schema, count, required fields, SHA-256, absolute media paths, safe relative paths and duplicate IDs/paths before enabling import. It imports original paths without copying. Short, safe `\\?\C:\...` paths are converted for human/API handoff; UNC, extended UNC, genuinely long paths and legacy-unsafe trailing-dot/space paths retain their prefix. Canonical verification paths are never changed. It never opens or edits `.prproj` files.

## Direct handoff and drag feasibility

Premiere 26.0.2 is above the documented UXP 25.6 baseline. Adobe documents `fetch` and WebSocket access from that baseline when explicit manifest network permissions are declared, so a loopback bridge is technically plausible. It is not implemented in this checkpoint: a release design still needs authenticated pairing, loopback-only binding, origin/request validation, expiring explicit selections, positive companion import acknowledgement, restart/port handling and real 26.0.2 permission/CORS acceptance. The manual no-overwrite manifest remains the supported fallback. PIMP does not report a manifest export as a Premiere import.

Windows-native outbound file and physical-folder drag is implemented in the Library as a separate simple handoff. Cards, folders and decorative thumbnail images are not HTML-draggable. A pointer state machine preserves ordinary controls and selection, waits for the Windows `SM_CXDRAG`/`SM_CYDRAG` distance, releases WebView pointer capture and invokes the synchronous native drag while the left button remains held. The host canonicalizes explicit selections beneath Master Bin, expands an unambiguous attached WAV, deduplicates physical paths and rejects unavailable, changed, escaped or excessive inputs. Ordinary short local paths use the maintained permissively licensed `drag` crate in COPY mode. UNC/mapped-share canonical forms and long paths use the guarded Shell/OLE implementation because `drag` 2.1.1 has an unresolved Windows panic for those classes; that fallback removes an extended prefix only for Shell parsing, creates absolute Shell item identities, verifies `CF_HDROP`, retains the PIDLs for its lifetime and permits only `DROPEFFECT_COPY`. Plain clipboard text, thumbnail JPEGs and HTML drag data are not substitutes. A real dragged folder preserves its hierarchy, while arbitrary multi-file selections do not claim virtual-folder preservation; use the structured manifest import when hierarchy matters.

The corrected source implementation and its safety routing are automated-test covered, including a real Windows Shell data object queried for `CF_HDROP` with multiple files, a Unicode folder and a supported long path. An isolated packaged executable also completed configuration and full verification of two synthetic MP4 fixtures without touching the installed production catalogue. The computer-control safety boundary cannot complete a cross-window mouse drag, so Explorer copy and Premiere Pro 26.0.2 import acceptance remain manual gates. Until both pass, the feature is implemented but not release-verified; the manifest Companion remains the structured fallback.

## Development loading and packaging

1. Install Premiere 25.6 or newer and Adobe UXP Developer Tool 2.2 or newer through Creative Cloud.
2. Enable Developer Mode in Premiere's Plugins preferences.
3. Add `apps/premiere-companion/manifest.json` to UXP Developer Tool and load it into a disposable Premiere project.
4. In PIMP Library, select recordings and choose **Export Premiere manifest**. Select that JSON in the panel.
5. Verify target-bin creation, date hierarchy, original-path imports, DJI WAV inclusion, duplicate skipping and failure reporting.
6. Use UXP Developer Tool's **Package** action to create the `.ccx`; do not hand-rename a ZIP as a release package.
7. Install the resulting `.ccx` through Creative Cloud Desktop and repeat acceptance before distribution.

For host acceptance, use a new disposable project and generated MP4/WAV files only. Record the Premiere and UDT versions, then verify all of the following before changing this integration from Partial to Tested:

- the panel loads under **Window → UXP Plugins → Kairix PIMP**;
- a valid PIMP manifest can be chosen while a malformed/schema-changed manifest is rejected;
- the selected target bin and manifest date hierarchy are created;
- MP4 and attached WAV entries reference their original absolute paths;
- importing the same manifest again skips existing paths without duplicate items;
- a manifest whose source was removed reports that item clearly;
- one missing item does not prevent valid remaining items from importing; and
- pre-existing sequences, bins and project items remain unchanged.

Use UDT's Logs and App Logs for any unsupported method, transaction, permission or import failure. Correct only against documented Premiere UXP APIs; never inspect or rewrite `.prproj` internals.

The pull-request Windows workflow publishes the exact developer source above as a versioned, distinctly named ZIP artifact. Extract it before adding `manifest.json` to UDT. This ZIP is **not** a `.ccx` and must never be renamed to one.

The plugin ID must be replaced with a valid Adobe Developer Distribution ID before Marketplace publication. No local server, account credential or network permission is used in this proof of concept.

## Read-only saved-project fallback

PIMP now contains an intentionally narrow, offline fallback for the deterministic main `.prproj` in a registered Working Project. It reads one stable snapshot, accepts bounded gzip-XML or XML containers, rejects document types and excessive size/depth/event/value budgets, and extracts only structurally recognized imported paths, bin names and sequence identities. It never writes `.prproj` or `.prin`, does not interpret effects or proprietary embeddings, and does not yet claim compatibility with a real Premiere-generated project version.

The index is fingerprinted and stored as device-local rebuildable JSON. Parser version 2 treats `ObjectUID`/`ObjectID` records as definitions and `ObjectURef`/`ObjectRef` records as references, reads sequence names from child `Name` elements, and follows only unambiguous project-item/container/bin and clip-track relationships. It deduplicates repeated references and distinguishes established imported and saved-timeline-used evidence. An otherwise arbitrary path is unclassified; it is labelled historical only when the previous device-local projection contains positive imported/used evidence for that exact path. Per-sequence placement counts appear only where media paths resolve, and an incomplete relationship graph makes imported-versus-unused counts visibly **Unknown**, not zero. The parser deliberately omits source/placement ranges until Premiere time-base fields are verified. A path associates with a PIMP media ID only when both resolve to the same existing canonical file; filenames alone are not evidence. Unknown or unsupported structure remains visibly unknown. Live Companion observation remains the preferred future source and must coexist with, not overwrite, saved-project evidence.
