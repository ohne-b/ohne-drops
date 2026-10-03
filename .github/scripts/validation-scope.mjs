import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const documentation = new Set(['README.md', 'CONTRIBUTING.md', 'AGENTS.md']);

export function needsFullValidation(event, paths) {
  return event !== 'pull_request' || paths.length === 0 || paths.some(path => !documentation.has(path));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  // Compare the tested PR merge with its base. Disabling rename detection checks both paths.
  const paths = process.env.GITHUB_EVENT_NAME === 'pull_request'
    ? execFileSync('git', ['diff', '--name-only', '--no-renames', '-z', 'HEAD^', 'HEAD', '--'], {
      encoding: 'utf8',
    }).split('\0').filter(Boolean)
    : [];
  console.log(`full=${needsFullValidation(process.env.GITHUB_EVENT_NAME, paths)}`);
}
