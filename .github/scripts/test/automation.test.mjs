import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { bumpVersion, readVersion, validateVersion, isPrerelease, releaseImages, releaseManifest, releaseNotes, writeReleaseArtifacts } from '../release.mjs';

test('release versions use SemVer precedence and reject shell syntax and noncanonical input', () => {
  assert.equal(validateVersion('2.0.0-rc.10', '2.0.0-rc.2'), '2.0.0-rc.10');
  assert.equal(validateVersion('2.0.0+build.1', '1.0.0'), '2.0.0+build.1');
  assert.equal(validateVersion('2.0.0-rc.1+build.1', '1.0.0'), '2.0.0-rc.1+build.1');
  for (const version of ['2.0.0', '2.0.0+build-name']) assert.equal(isPrerelease(version), false);
  for (const version of ['2.0.0-rc.1', '2.0.0-rc.1+build-name']) assert.equal(isPrerelease(version), true);
  for (const version of ['v2.0.0', '02.0.0', '2.0', '2.0.0\n', '2.0.0;true', '$(id)', '1.0.0', '0.9.9']) {
    assert.throws(() => validateVersion(version, '1.0.0'));
  }
});

test('Cargo metadata validates and updates both version files without changing dependencies', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tdm-release-'));
  try {
    mkdirSync(join(directory, 'src'));
    writeFileSync(join(directory, 'src/main.rs'), 'fn main() {}\n');
    writeFileSync(join(directory, 'Cargo.toml'), '[package]\nname = "twitch-miner"\nversion = "1.0.0"\nedition = "2024"\n');
    execFileSync('cargo', ['generate-lockfile', '--offline'], { cwd: directory });
    assert.equal(readVersion(directory), '1.0.0');
    writeFileSync(join(directory, 'CHANGELOG.md'), releaseEntry('1.0.0'));
    writeReleaseArtifacts('1.0.0', directory);
    assert.deepEqual(JSON.parse(readFileSync(join(directory, 'dist/release/latest.json'), 'utf8')), releaseManifest('1.0.0'));
    assert.match(readFileSync(join(directory, 'dist/release/notes.md'), 'utf8'), /commits\/v1\.0\.0/);
    assert.throws(() => writeReleaseArtifacts('2.0.0', directory));
    assert.equal(bumpVersion('1.1.0-rc.1+build.1', directory), '1.1.0-rc.1+build.1');
    assert.match(readFileSync(join(directory, 'Cargo.lock'), 'utf8'), /version = "1.1.0-rc.1\+build.1"/);
    const lock = readFileSync(join(directory, 'Cargo.lock'), 'utf8');
    writeFileSync(join(directory, 'Cargo.lock'), lock.replace('1.1.0-rc.1', '0.0.1'));
    assert.throws(() => readVersion(directory));
    assert.equal(readFileSync(join(directory, 'Cargo.lock'), 'utf8'), lock.replace('1.1.0-rc.1', '0.0.1'));
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

const releaseEntry = version => `## [v${version}](https://github.com/ohne-b/twitch-miner/releases/tag/v${version}) - 2026-09-26\n\n- a reviewed change\n`;
test('release images share one version across registries and reject malformed destinations', () => {
  assert.deepEqual(releaseImages('0.1.0'), ['ghcr.io/ohne-b/twitch-miner:0.1.0']);
  assert.deepEqual(releaseImages('0.2.0-rc.1+build.1', 'example/twitch-miner'), [
    'ghcr.io/ohne-b/twitch-miner:0.2.0-rc.1_build.1',
    'docker.io/example/twitch-miner:0.2.0-rc.1_build.1',
  ]);
  for (const destination of ['Example/miner', 'example/miner:latest', 'docker.io/example/miner', 'example/miner\nother/miner', 'https://example/miner', '$(id)/miner', 'example/' + 'a'.repeat(256)]) {
    assert.throws(() => releaseImages('0.1.0', destination));
  }
});
test('every release has matching metadata and concise reviewed notes, never an installer manifest', () => {
  assert.deepEqual(releaseManifest('0.1.0'), {
    schemaVersion: 1,
    version: '0.1.0',
    notes: '[Check release notes on GitHub](https://github.com/ohne-b/twitch-miner/releases/tag/v0.1.0)',
  });
  assert.equal(releaseManifest('0.2.0-rc.1').version, '0.2.0-rc.1');
  const notes = releaseNotes('0.2.0', releaseEntry('0.2.0') + '\n' + releaseEntry('0.1.0'));
  assert.match(notes, /^- a reviewed change\n/);
  assert.match(notes, /compare\/v0\.1\.0\.\.\.v0\.2\.0/);
  assert.match(notes, /issues\/new/);
  assert.equal(notes.match(/a reviewed change/g).length, 1);
  for (const changelog of ['', releaseEntry('0.1.0').repeat(2), releaseEntry('0.1.0').replace('- a reviewed change', ''), releaseEntry('0.1.0') + releaseEntry('1.0.0')]) {
    assert.throws(() => releaseNotes('0.1.0', changelog));
  }
});

test('English catalog covers production message keys and contains plain text', () => {
  const root = new URL('../../../', import.meta.url);
  const dictionary = JSON.parse(readFileSync(new URL('lang/English.json', root), 'utf8'));
  const lookup = key => key.split('.').reduce((value, part) => value?.[part], dictionary);
  for (const folder of ['src/', 'frontend/src/']) {
    const base = new URL(folder, root);
    for (const name of readdirSync(base, { recursive: true }).filter(name => /\.(rs|tsx?)$/.test(name))) {
      const source = readFileSync(new URL(name.replaceAll('\\', '/'), base), 'utf8');
      for (const match of source.matchAll(/['"]((?:gui|login|status)\.[a-z_]+(?:\.[a-z_]+)*)['"]/g)) {
        assert.equal(typeof lookup(match[1]), 'string', `${name}: missing ${match[1]}`);
      }
    }
  }
  function check(value) {
    for (const text of Object.values(value)) {
      if (typeof text === 'object') check(text);
      else assert.ok(typeof text === 'string' && !/[<>]|\p{Extended_Pictographic}/u.test(text));
    }
  }
  check(dictionary);
});
