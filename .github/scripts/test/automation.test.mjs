import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { bumpVersion, readVersion, validateVersion, isPrerelease } from '../release.mjs';
import { updateContributors } from '../update-contributors.mjs';

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
    assert.equal(bumpVersion('1.1.0-rc.1+build.1', directory), '1.1.0-rc.1+build.1');
    assert.match(readFileSync(join(directory, 'Cargo.lock'), 'utf8'), /version = "1.1.0-rc.1\+build.1"/);
    const lock = readFileSync(join(directory, 'Cargo.lock'), 'utf8');
    writeFileSync(join(directory, 'Cargo.lock'), lock.replace('1.1.0-rc.1', '0.0.1'));
    assert.throws(() => readVersion(directory));
    assert.equal(readFileSync(join(directory, 'Cargo.lock'), 'utf8'), lock.replace('1.1.0-rc.1', '0.0.1'));
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

const table = 'Before\n<!-- contributors:start -->\n| Contributor | Merged pull requests |\n| --- | --- |\n<!-- contributors:end -->\nAfter\n';
const credit = { login: 'Ben', profile: 'https://github.com/Ben', number: 12, url: 'https://github.com/ohne-b/twitch-miner/pull/12' };
test('contributor updates are sorted, case-insensitive, idempotent, and retain newlines', () => {
  for (const input of [table, table.replaceAll('\n', '\r\n')]) {
    const first = updateContributors(input, credit);
    assert.equal(updateContributors(first, credit), first);
    const second = updateContributors(first, { ...credit, login: 'ben', profile: 'https://github.com/ben', number: 13, url: credit.url.replace('/12', '/13') });
    assert.equal(second.match(/\[@/g).length, 1);
    assert.match(second, /#12.* · .*#13/);
    const sorted = updateContributors(second, { ...credit, login: 'Alice', profile: 'https://github.com/Alice', number: 14, url: credit.url.replace('/12', '/14') });
    assert.ok(sorted.indexOf('[@Alice]') < sorted.indexOf('[@Ben]'));
    assert.equal(sorted.includes('\r\n'), input.includes('\r\n'));
    assert.ok(sorted.endsWith('After' + (input.includes('\r\n') ? '\r\n' : '\n')));
  }
});
test('contributor metadata and malformed managed sections fail closed', () => {
  for (const input of [table.replace('contributors:start', 'absent'), table + '<!-- contributors:end -->', table.replace('| --- | --- |', '| bad |'), table.replace('<!-- contributors:end -->', 'bad row\n<!-- contributors:end -->')]) {
    assert.throws(() => updateContributors(input, credit));
  }
  for (const changes of [{ login: 'x\nmalicious' }, { profile: 'https://evil.test/Ben' }, { number: 0 }, { url: credit.url + '?x=1' }]) {
    assert.throws(() => updateContributors(table, { ...credit, ...changes }));
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
