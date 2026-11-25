/* global module */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  root.PimpManifest = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  var KIND = 'kairix-pimp-premiere-import';
  var SHA256 = /^[a-f0-9]{64}$/i;

  function validate(value) {
    if (!value || typeof value !== 'object')
      throw new Error('Manifest must be a JSON object.');
    if (value.schemaVersion !== 1 || value.kind !== KIND)
      throw new Error('Unsupported PIMP Premiere manifest schema.');
    if (!Array.isArray(value.items) || value.items.length === 0)
      throw new Error('Manifest contains no media items.');
    if (value.items.length > 5000)
      throw new Error('Manifest exceeds the 5,000-item safety limit.');

    var mediaIds = new Set();
    var paths = new Set();
    var items = value.items.map(function (item, index) {
      if (!item || typeof item !== 'object')
        throw new Error('Invalid item at position ' + (index + 1) + '.');
      var required = [
        'mediaId',
        'absolutePath',
        'relativePath',
        'originalFilename',
        'sha256',
        'fileType',
      ];
      required.forEach(function (field) {
        if (typeof item[field] !== 'string' || !item[field].trim())
          throw new Error(
            'Item ' + (index + 1) + ' has an invalid ' + field + '.',
          );
      });
      if (typeof item.binPath !== 'string')
        throw new Error('Item ' + (index + 1) + ' has an invalid binPath.');
      if (!SHA256.test(item.sha256))
        throw new Error('Item ' + (index + 1) + ' has an invalid SHA-256.');
      if (!/^(?:[a-z]:[\\/]|\\\\)/i.test(item.absolutePath))
        throw new Error('Manifest contains a non-absolute media path.');
      if (
        !safeRelative(item.relativePath, false) ||
        !safeRelative(item.binPath, true)
      )
        throw new Error('Manifest contains an unsafe relative path.');
      var pathKey = normalizePath(item.absolutePath);
      if (mediaIds.has(item.mediaId) || paths.has(pathKey))
        throw new Error(
          'Manifest contains a duplicate media identity or path.',
        );
      mediaIds.add(item.mediaId);
      paths.add(pathKey);
      return Object.assign({}, item, {
        absolutePath: item.absolutePath.trim(),
      });
    });
    return Object.assign({}, value, { items: items });
  }

  function normalizePath(path) {
    return String(path).replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
  }

  function safeRelative(path, allowEmpty) {
    var normalized = String(path).replace(/\\/g, '/');
    if (!normalized) return allowEmpty;
    if (normalized.startsWith('/') || /^[a-z]:/i.test(normalized)) return false;
    return normalized.split('/').every(function (segment) {
      return segment && segment !== '.' && segment !== '..';
    });
  }

  function groupByBin(items, preserveHierarchy) {
    var groups = new Map();
    items.forEach(function (item) {
      var binPath = preserveHierarchy
        ? item.binPath.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '')
        : '';
      if (!groups.has(binPath)) groups.set(binPath, []);
      groups.get(binPath).push(item);
    });
    return Array.from(groups.entries()).map(function (entry) {
      return { binPath: entry[0], items: entry[1] };
    });
  }

  return {
    KIND: KIND,
    validate: validate,
    normalizePath: normalizePath,
    groupByBin: groupByBin,
  };
});
