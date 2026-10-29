# Premiere Companion integration

## Status and evidence boundary

`apps/premiere-companion` is a standalone **Kairix PIMP — Premiere Companion** UXP proof of concept. PIMP exports a narrowly scoped, versioned JSON manifest for the current physical Library selection. The panel lets the user choose that manifest, choose or create a target Premiere bin, preserve date-folder bins, skip paths already present in the open project, and report imported, skipped and failed items.

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

## Interchange safety

The schema identifier is `kairix-pimp-premiere-import`, version 1. Every item contains its stable media ID, canonical device-local absolute path, portable Master Bin relative path, original filename, SHA-256, file type, date/package-relative bin path and optional attached-audio parent ID.

The native exporter:

- accepts only known catalogue IDs and at most 5,000 physical files;
- canonicalizes every path beneath the configured Master Bin and requires the file to be locally available;
- includes attached DJI WAV files when they are part of the logical selection;
- creates a new `.json` file and refuses to replace an existing file;
- reads source evidence but never writes, renames or copies media.

The plugin requests access only to the user-selected manifest. It validates schema, count, required fields, SHA-256, absolute media paths, safe relative paths and duplicate IDs/paths before enabling import. It imports original paths without copying. It never opens or edits `.prproj` files.

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

The `0.1.1-6` pull-request workflow publishes the exact developer source above as a distinctly named ZIP artifact. Extract it before adding `manifest.json` to UDT. This ZIP is **not** a `.ccx` and must never be renamed to one.

The plugin ID must be replaced with a valid Adobe Developer Distribution ID before Marketplace publication. No local server, account credential or network permission is used in this proof of concept.
