import assert from 'node:assert/strict';
import { test } from 'node:test';
import { needsFullValidation } from '../validation-scope.mjs';

test('only explicitly allowlisted documentation PRs skip code validation', () => {
  for (const paths of [['README.md'], ['CONTRIBUTING.md', 'AGENTS.md'], ['README.md', 'CONTRIBUTING.md', 'AGENTS.md']]) {
    assert.equal(needsFullValidation('pull_request', paths), false);
    for (const event of ['push', 'workflow_dispatch', '', undefined]) {
      assert.equal(needsFullValidation(event, paths), true);
    }
  }
  for (const path of ['src/main.rs', 'frontend/src/app.tsx', 'Cargo.lock', 'LICENSE.md', 'NOTICE.md',
    'CHANGELOG.md', '.github/workflows/validation.yml', '.github/scripts/validation-scope.mjs',
    'frontend/public/assets/licenses/manrope.txt', 'README.md.old', 'readme.md', 'README.md\nsrc/main.rs']) {
    assert.equal(needsFullValidation('pull_request', ['README.md', path]), true, path);
  }
  assert.equal(needsFullValidation('pull_request', []), true);
});
