import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const semver = createRequire(new URL('../../frontend/package.json', import.meta.url))('semver');

export function validateVersion(version, previous) {
  if (typeof version !== 'string' || semver.valid(version) !== version ||
      (previous && !semver.gt(version, previous))) {
    throw new Error('Use a canonical SemVer version greater than the current version.');
  }
  return version;
}

export function readVersion(directory = process.cwd(), locked = true) {
  const args = ['metadata', '--format-version', '1'];
  if (locked) args.push('--locked');
  const metadata = JSON.parse(execFileSync('cargo', args, {
    cwd: directory, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'],
  }));
  const pkg = metadata.packages.find(p => p.name === 'twitch-miner' && metadata.workspace_members.includes(p.id));
  if (!pkg) throw new Error('The twitch-miner package is missing.');
  return validateVersion(pkg.version);
}

export function bumpVersion(version, directory = process.cwd()) {
  validateVersion(version, readVersion(directory));
  const manifest = resolve(directory, 'Cargo.toml');
  const lock = resolve(directory, 'Cargo.lock');
  const original = readFileSync(manifest, 'utf8');
  const previousLock = readFileSync(lock);
  // Cargo owns TOML parsing and lockfile generation. Only edit the root package field.
  const pattern = /(^\[package\]\r?\n[\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m;
  if (!pattern.test(original)) throw new Error('Root package version is missing.');
  try {
    writeFileSync(manifest, original.replace(pattern, (_, before, after) => `${before}${version}${after}`));
    readVersion(directory, false);
    if (readVersion(directory) !== version) throw new Error('Version update did not persist.');
  } catch (error) {
    writeFileSync(manifest, original);
    writeFileSync(lock, previousLock);
    throw error;
  }
  return version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [command, version] = process.argv.slice(2);
  if (command === 'read') console.log(readVersion());
  else if (command === 'bump') console.log(bumpVersion(version));
  else if (command === 'verify') {
    validateVersion(version);
    if (readVersion() !== version) throw new Error('Requested release differs from the checked-out package.');
    console.log(version);
  } else throw new Error('Usage: node .github/scripts/release.mjs read|bump VERSION|verify VERSION');
}
