import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { test } from 'node:test';
import { publishImages, validatedImage, validatedRun } from '../publish-images.mjs';

const sha = 'a'.repeat(40);
const version = '1.4.0';
const image = 'ghcr.io/ohne-b/twitch-drops-miner';
const successful = { databaseId: 42, attempt: 1, headSha: sha, headBranch: 'main', event: 'push', status: 'completed', conclusion: 'success' };
const metadata = arch => ({
  Os: 'linux', Architecture: arch, Digest: `sha256:${(arch === 'amd64' ? 'b' : 'c').repeat(64)}`,
  Labels: { 'org.opencontainers.image.version': version, 'org.opencontainers.image.revision': sha },
});

test('publication accepts only the latest trusted validation of this exact main revision', () => {
  assert.deepEqual(validatedRun([successful], sha), { id: 42, attempt: 1 });
  assert.deepEqual(validatedRun([{ ...successful, event: 'workflow_dispatch' }], sha), { id: 42, attempt: 1 });
  for (const change of [{ event: 'pull_request' }, { headBranch: 'feature' }, { headSha: 'd'.repeat(40) },
    { status: 'in_progress' }, { conclusion: 'failure' }, { conclusion: 'cancelled' }, { databaseId: '42;sh' }, { attempt: 0 }]) {
    assert.throws(() => validatedRun([{ ...successful, ...change }], sha));
  }
  assert.throws(() => validatedRun([], sha));
  assert.throws(() => validatedRun([{ ...successful, conclusion: 'failure' }, successful], sha));
});

test('image identity includes version, revision, platform and a canonical digest', () => {
  assert.equal(validatedImage(metadata('arm64'), 'arm64', version, sha), `${image}@sha256:${'c'.repeat(64)}`);
  for (const change of [{ Os: 'windows' }, { Architecture: 'arm64' }, { Labels: null },
    { Labels: { ...metadata('amd64').Labels, 'org.opencontainers.image.version': '1.3.1' } },
    { Labels: { ...metadata('amd64').Labels, 'org.opencontainers.image.revision': 'd'.repeat(40) } },
    { Digest: 'sha256:bad' }]) {
    assert.throws(() => validatedImage({ ...metadata('amd64'), ...change }, 'amd64', version, sha));
  }
});

function commands(t, failure) {
  const calls = [];
  const environment = { GITHUB_SHA: sha, GITHUB_REPOSITORY: 'ohne-b/twitch-drops-miner',
    GITHUB_REF: 'refs/heads/main', GH_TOKEN: 'test-token', GITHUB_ACTOR: 'test-user' };
  for (const [key, value] of Object.entries(environment)) {
    const previous = process.env[key];
    process.env[key] = value;
    t.after(() => { if (previous === undefined) delete process.env[key]; else process.env[key] = previous; });
  }
  t.mock.method(childProcess, 'execFileSync', (command, args, options) => {
    calls.push({ command, args, options });
    if (command === 'git') {
      const checks = calls.filter(call => call.command === 'git').length;
      const moved = (failure === 'main moved' && checks > 1) || (failure === 'main moved during copy' && checks > 2);
      return `${moved ? 'd'.repeat(40) : sha}\trefs/heads/main\n`;
    }
    if (command === 'gh' && args[1] === 'list') {
      const checks = calls.filter(call => call.command === 'gh' && call.args[1] === 'list').length;
      if (checks > 1 && failure === 'new pending validation') return JSON.stringify([{ ...successful, databaseId: 43, status: 'in_progress' }, successful]);
      if (checks > 1 && failure === 'new failed validation') return JSON.stringify([{ ...successful, databaseId: 43, conclusion: 'failure' }, successful]);
      if (checks > 2 && failure === 'validation retried during copy') return JSON.stringify([{ ...successful, attempt: 2 }]);
      return JSON.stringify([successful]);
    }
    if (command === 'gh' && args[1] === 'download' && failure === 'missing artifacts') throw new Error('expired');
    if (command === 'skopeo' && args.includes('inspect')) {
      const info = metadata(args[1]);
      if (failure === 'wrong second image' && info.Architecture === 'arm64') info.Labels = {};
      return JSON.stringify(info);
    }
    return '';
  });
  return calls;
}

test('release and edge promote both verified digests without compiling or introducing architecture tags', t => {
  const calls = commands(t);
  for (const target of [`${image}:${version}`, `${image}:edge`]) {
    publishImages(version, target);
    const create = calls.at(-1);
    assert.deepEqual(create.args, ['buildx', 'imagetools', 'create', '--tag', target,
      `${image}@sha256:${'b'.repeat(64)}`, `${image}@sha256:${'c'.repeat(64)}`]);
  }
  const copies = calls.filter(call => call.command === 'skopeo' && call.args[0] === 'copy');
  assert.equal(copies.length, 4);
  assert.ok(copies.every(call => call.args.includes('--preserve-digests')));
  assert.ok(copies.every(call => call.args.at(-1).startsWith(`docker://${image}@sha256:`)));
  assert.ok(calls.every(call => !['cargo', 'npm'].includes(call.command) && !call.args.includes('build')));
  const download = calls.find(call => call.command === 'gh' && call.args[1] === 'download');
  assert.ok(download.args.includes('42') && download.args.includes('image-amd64') && download.args.includes('image-arm64'));
});

for (const failure of ['missing artifacts', 'wrong second image', 'main moved', 'new pending validation', 'new failed validation']) {
  test(`${failure} fails before any registry login, push or tag change`, t => {
    const calls = commands(t, failure);
    assert.throws(() => publishImages(version, `${image}:${version}`));
    assert.ok(calls.every(call => call.command !== 'docker' && !call.args.includes('copy')));
  });
}

for (const failure of ['main moved during copy', 'validation retried during copy']) {
  test(`${failure} cannot change a public tag`, t => {
    const calls = commands(t, failure);
    assert.throws(() => publishImages(version, `${image}:${version}`));
    assert.ok(calls.some(call => call.command === 'skopeo' && call.args[0] === 'copy'));
    assert.ok(calls.every(call => !call.args.includes('create')));
  });
}

test('publication rejects arbitrary registries, tags and non-main execution before running commands', t => {
  const calls = commands(t);
  for (const target of [`${image}:latest`, `${image}:other`, 'example.com/miner:1.4.0']) {
    assert.throws(() => publishImages(version, target));
  }
  process.env.GITHUB_REF = 'refs/pull/1/merge';
  assert.throws(() => publishImages(version, `${image}:edge`));
  assert.equal(calls.length, 0);
});
