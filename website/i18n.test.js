'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const i18n = require('./i18n/loader.js');
const root = __dirname;
const en = JSON.parse(fs.readFileSync(path.join(root, 'i18n', 'en.json'), 'utf8'));
const es = JSON.parse(fs.readFileSync(path.join(root, 'i18n', 'es.json'), 'utf8'));
const fr = JSON.parse(fs.readFileSync(path.join(root, 'i18n', 'fr.json'), 'utf8'));
const pt = JSON.parse(fs.readFileSync(path.join(root, 'i18n', 'pt.json'), 'utf8'));
const html = fs.readFileSync(path.join(root, 'index.html'), 'utf8');

function getNested(catalog, key) {
  return key.split('.').reduce((value, part) => {
    if (value === null || value === undefined || typeof value !== 'object') return undefined;
    return value[part];
  }, catalog);
}

function flatten(catalog, prefix) {
  const keys = {};
  for (const [key, value] of Object.entries(catalog)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      Object.assign(keys, flatten(value, path));
    } else {
      keys[path] = value;
    }
  }
  return keys;
}

test('every data-i18n key used by the page exists in all supported languages', () => {
  const keys = new Set();
  const pattern = /data-i18n(?:-[a-z-]+)?="([^"]+)"/g;
  let match;
  while ((match = pattern.exec(html))) keys.add(match[1]);

  assert.ok(keys.size > 100, 'expected a substantial number of translation keys');
  for (const key of keys) {
    assert.notEqual(getNested(en, key), undefined, `missing English key: ${key}`);
    assert.notEqual(getNested(es, key), undefined, `missing Spanish key: ${key}`);
    assert.notEqual(getNested(fr, key), undefined, `missing French key: ${key}`);
    assert.notEqual(getNested(pt, key), undefined, `missing Portuguese key: ${key}`);
  }
});

test('English, Spanish, French, and Portuguese catalogs expose matching top-level metadata', () => {
  assert.equal(en._meta.fallback, 'en');
  assert.equal(es._meta.fallback, 'en');
  assert.equal(fr._meta.fallback, 'en');
  assert.equal(pt._meta.fallback, 'en');

  assert.equal(en._meta.direction, 'ltr');
  assert.equal(es._meta.direction, 'ltr');
  assert.equal(fr._meta.direction, 'ltr');
  assert.equal(pt._meta.direction, 'ltr');

  assert.equal(en.contracts.items.vision_records.name, 'Vision Records');
  assert.equal(es.contracts.items.vision_records.name, 'Registros de Visión');
  assert.equal(fr.contracts.items.vision_records.name, 'Dossiers de vision');
  assert.equal(pt.contracts.items.vision_records.name, 'Prontuários de Visão');
});

test('Spanish, French, and Portuguese catalogs cover every English key', () => {
  const enKeys = Object.keys(flatten(en, '')).sort();
  assert.deepEqual(Object.keys(flatten(es, '')).sort(), enKeys);
  assert.deepEqual(Object.keys(flatten(fr, '')).sort(), enKeys);
  assert.deepEqual(Object.keys(flatten(pt, '')).sort(), enKeys);
});

test('catalog lookup supports nested keys in all locales', () => {
  assert.equal(i18n.lookup(en, 'nav.contracts'), 'Contracts');
  assert.equal(i18n.lookup(es, 'nav.contracts'), 'Contratos');
  assert.equal(i18n.lookup(fr, 'nav.contracts'), 'Contrats');
  assert.equal(i18n.lookup(pt, 'nav.contracts'), 'Contratos');
  assert.equal(i18n.lookup(en, 'does.not.exist'), undefined);
});

test('interpolation replaces provided placeholders', () => {
  assert.equal(i18n.interpolate('{count} contracts shown', { count: 24 }), '24 contracts shown');
  assert.equal(i18n.interpolate('Hello {name}', {}), 'Hello {name}');
});

test('missing translations fall back to English', () => {
  const spanish = { nav: { contracts: 'Contratos' } };
  const english = { nav: { contracts: 'Contracts', overview: 'Overview' } };
  assert.equal(i18n.resolveTranslation(spanish, english, 'nav.contracts'), 'Contratos');
  assert.equal(i18n.resolveTranslation(spanish, english, 'nav.overview'), 'Overview');
});

test('language normalization and RTL direction are supported', () => {
  assert.equal(i18n.normalizeLanguage('es-MX'), 'es');
  assert.equal(i18n.normalizeLanguage('fr-FR'), 'fr');
  assert.equal(i18n.normalizeLanguage('pt-BR'), 'pt');
  assert.equal(i18n.normalizeLanguage('de-DE'), 'en');
  assert.equal(i18n.isRtlLanguage('ar-SA'), true);
  assert.equal(i18n.isRtlLanguage('he'), true);
  assert.equal(i18n.isRtlLanguage('es'), false);
  assert.equal(i18n.directionFor('ar', null), 'rtl');
  assert.equal(i18n.directionFor('ar', { direction: 'ltr' }), 'ltr');
  assert.equal(i18n.directionFor('es', null), 'ltr');
});

test('RTL scaffold includes Arabic, Hebrew, Farsi, and Urdu', () => {
  assert.deepEqual(i18n.RTL_LANGUAGES.slice().sort(), ['ar', 'fa', 'he', 'ur']);
});

test('every supported language has a catalog file', () => {
  for (const language of i18n.SUPPORTED_LANGUAGES) {
    const file = path.join(root, 'i18n', `${language.code}.json`);
    assert.ok(fs.existsSync(file), `missing catalog: ${file}`);
  }
});
