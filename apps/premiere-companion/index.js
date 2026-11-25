/* global require, document, PimpManifest */
/* eslint-disable @typescript-eslint/no-require-imports */
'use strict';

const premiere = require('premierepro');
const { localFileSystem } = require('uxp').storage;

let selectedManifest;

const chooseButton = document.getElementById('choose-manifest');
const importButton = document.getElementById('import-media');
const summary = document.getElementById('manifest-summary');
const status = document.getElementById('status');
const results = document.getElementById('results');
const targetBinInput = document.getElementById('target-bin');
const preserveBinsInput = document.getElementById('preserve-bins');

function setStatus(message) {
  status.textContent = message;
}

function addResult(kind, message) {
  const row = document.createElement('li');
  row.className = kind;
  row.textContent = message;
  results.appendChild(row);
}

async function asFolder(item) {
  try {
    const folder = premiere.FolderItem.cast(item);
    await folder.getItems();
    return folder;
  } catch {
    return undefined;
  }
}

async function findChildBin(parent, name) {
  for (const item of await parent.getItems()) {
    if (item.name !== name) continue;
    const folder = await asFolder(item);
    if (folder) return folder;
  }
  return undefined;
}

async function getOrCreateBin(project, parent, name) {
  const existing = await findChildBin(parent, name);
  if (existing) return existing;
  project.lockedAccess(() => {
    project.executeTransaction((compoundAction) => {
      compoundAction.addAction(parent.createBinAction(name, false));
    }, `Create PIMP bin ${name}`);
  });
  const created = await findChildBin(parent, name);
  if (!created) throw new Error(`Premiere did not create bin “${name}”.`);
  return created;
}

async function resolveTargetBin(project, root, targetName, relativePath) {
  let current = targetName
    ? await getOrCreateBin(project, root, targetName)
    : root;
  for (const name of relativePath.split('/').filter(Boolean))
    current = await getOrCreateBin(project, current, name);
  return current;
}

async function collectImportedPaths(folder, destination = new Set()) {
  for (const item of await folder.getItems()) {
    const childFolder = await asFolder(item);
    if (childFolder) {
      await collectImportedPaths(childFolder, destination);
      continue;
    }
    try {
      const clip = premiere.ClipProjectItem.cast(item);
      const path = await clip.getMediaFilePath();
      if (path) destination.add(PimpManifest.normalizePath(path));
    } catch {
      // Sequences and unsupported project items have no source media path.
    }
  }
  return destination;
}

chooseButton.addEventListener('click', async () => {
  setStatus('Choose a PIMP Premiere import manifest.');
  results.replaceChildren();
  try {
    const file = await localFileSystem.getFileForOpening({ types: ['json'] });
    if (!file) return;
    selectedManifest = PimpManifest.validate(JSON.parse(await file.read()));
    summary.hidden = false;
    summary.textContent = `${selectedManifest.items.length} verified manifest item${selectedManifest.items.length === 1 ? '' : 's'} · ${selectedManifest.createdAt}`;
    importButton.disabled = false;
    setStatus('Manifest ready. Review the target bin and import.');
  } catch (error) {
    selectedManifest = undefined;
    importButton.disabled = true;
    summary.hidden = true;
    setStatus(`Manifest rejected: ${error.message ?? error}`);
  }
});

importButton.addEventListener('click', async () => {
  if (!selectedManifest) return;
  importButton.disabled = true;
  results.replaceChildren();
  setStatus('Inspecting the active Premiere project…');
  try {
    const project = await premiere.Project.getActiveProject();
    if (!project) throw new Error('Open a Premiere project before importing.');
    const root = await project.getRootItem();
    const existingPaths = await collectImportedPaths(root);
    const groups = PimpManifest.groupByBin(
      selectedManifest.items,
      preserveBinsInput.checked,
    );
    let imported = 0;
    let skipped = 0;
    let failed = 0;
    const targetName = targetBinInput.value.trim();

    for (const group of groups) {
      const target = await resolveTargetBin(
        project,
        root,
        targetName,
        group.binPath,
      );
      for (const item of group.items) {
        if (existingPaths.has(PimpManifest.normalizePath(item.absolutePath))) {
          skipped += 1;
          addResult('skipped', `Skipped existing: ${item.originalFilename}`);
          continue;
        }
        try {
          const success = await project.importFiles(
            [item.absolutePath],
            true,
            target,
            false,
          );
          if (!success)
            throw new Error('Premiere returned an unsuccessful import result.');
          existingPaths.add(PimpManifest.normalizePath(item.absolutePath));
          imported += 1;
          addResult('success', `Imported: ${item.originalFilename}`);
        } catch (error) {
          failed += 1;
          addResult(
            'failed',
            `Failed: ${item.originalFilename} — ${error.message ?? error}`,
          );
        }
      }
    }
    setStatus(`${imported} imported · ${skipped} skipped · ${failed} failed`);
  } catch (error) {
    setStatus(`Import stopped: ${error.message ?? error}`);
  } finally {
    importButton.disabled = false;
  }
});
