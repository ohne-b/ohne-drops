import childProcess from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseImages, validateVersion } from './release.mjs';

const repository = 'ohne-b/twitch-drops-miner';
const image = `ghcr.io/${repository}`;
const architectures = ['amd64', 'arm64'];

export function validatedRun(runs, sha) {
  const run = runs.find(run => run.headSha === sha && run.headBranch === 'main' &&
    ['push', 'workflow_dispatch'].includes(run.event));
  if (!run || run.status !== 'completed' || run.conclusion !== 'success' ||
      !Number.isSafeInteger(run.databaseId) || run.databaseId <= 0 ||
      !Number.isSafeInteger(run.attempt) || run.attempt <= 0) {
    throw new Error('Complete validation on the exact current main commit before publishing.');
  }
  return { id: run.databaseId, attempt: run.attempt };
}

export function validatedImage(info, arch, version, sha) {
  if (info.Os !== 'linux' || info.Architecture !== arch ||
      info.Labels?.['org.opencontainers.image.version'] !== version ||
      info.Labels?.['org.opencontainers.image.revision'] !== sha ||
      !/^sha256:[a-f0-9]{64}$/.test(info.Digest)) {
    throw new Error(`The ${arch} artifact does not match this release's platform, version and commit.`);
  }
  return `${image}@${info.Digest}`;
}

export function publishImages(version, target) {
  validateVersion(version);
  if (![releaseImages(version)[0], `${image}:edge`].includes(target)) {
    throw new Error('Only the requested GHCR version or edge tag may be published.');
  }
  const sha = process.env.GITHUB_SHA;
  if (process.env.GITHUB_REPOSITORY !== repository || process.env.GITHUB_REF !== 'refs/heads/main' ||
      !/^[a-f0-9]{40}$/.test(sha ?? '') || !process.env.GH_TOKEN || !process.env.GITHUB_ACTOR) {
    throw new Error('Publishing requires the canonical main workflow and its scoped token.');
  }
  const run = (command, args, options = {}) => childProcess.execFileSync(command, args, {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], ...options,
  });
  const checkMain = () => {
    if (run('git', ['ls-remote', 'origin', 'refs/heads/main']).split('\t')[0] !== sha) {
      throw new Error('Main advanced. Validate and publish the current main commit.');
    }
  };
  checkMain();
  const currentValidation = () => validatedRun(JSON.parse(run('gh', ['run', 'list', '--repo', repository,
    '--workflow', 'validation.yml', '--branch', 'main', '--commit', sha, '--limit', '20',
    '--json', 'databaseId,attempt,headSha,headBranch,event,status,conclusion'])), sha);
  const { id, attempt } = currentValidation();
  const checkCurrent = () => {
    checkMain();
    const current = currentValidation();
    if (current.id !== id || current.attempt !== attempt) {
      throw new Error('Validation changed during publication. Restart with its tested artifacts.');
    }
  };
  const directory = mkdtempSync(join(tmpdir(), 'tdm-images-'));
  try {
    run('gh', ['run', 'download', String(id), '--repo', repository, '--dir', directory,
      ...architectures.flatMap(arch => ['--name', `image-${arch}`])]);
  } catch {
    throw new Error(`Image artifacts for validation run ${id} are unavailable. Rerun validation on main; publishing never rebuilds them.`);
  }
  // Validate both artifacts before authenticating to the registry or publishing either one.
  const images = architectures.map(arch => {
    const source = `oci-archive:${join(directory, `image-${arch}`, 'image.tar')}`;
    const info = JSON.parse(run('skopeo', ['--override-arch', arch, 'inspect', source]));
    return { source, destination: validatedImage(info, arch, version, sha) };
  });
  checkCurrent();
  run('docker', ['login', 'ghcr.io', '-u', process.env.GITHUB_ACTOR, '--password-stdin'], {
    input: process.env.GH_TOKEN, stdio: ['pipe', 'inherit', 'inherit'],
  });
  for (const { source, destination } of images) {
    run('skopeo', ['copy', '--preserve-digests', '--retry-times', '3', source, `docker://${destination}`], { stdio: 'inherit' });
  }
  checkCurrent();
  run('docker', ['buildx', 'imagetools', 'create', '--tag', target,
    ...images.map(({ destination }) => destination)], { stdio: 'inherit' });
  console.log(`Published ${target} from validation run ${id} (${sha}).`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  publishImages(...process.argv.slice(2));
}
