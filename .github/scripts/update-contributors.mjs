import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

export function updateContributors(text, { login, profile, number, url }) {
  if (!/^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?$/.test(login) ||
      !Number.isSafeInteger(number) || number < 1 || profile !== `https://github.com/${login}` ||
      !/^https:\/\/github\.com\/[A-Za-z0-9-]+\/[A-Za-z0-9_.-]+\/pull\/[1-9]\d*$/.test(url) ||
      !url.endsWith(`/pull/${number}`)) throw new Error('Invalid contributor metadata.');
  const lines = text.split(/\r?\n/);
  const start = '<!-- contributors:start -->', end = '<!-- contributors:end -->';
  const first = lines.indexOf(start), last = lines.indexOf(end);
  if (lines.filter(l => l === start).length !== 1 || lines.filter(l => l === end).length !== 1 ||
      first >= last || lines[first + 1] !== '| Contributor | Merged pull requests |' ||
      lines[first + 2] !== '| --- | --- |') throw new Error('Invalid contributor table or markers.');
  const rows = lines.slice(first + 3, last);
  const pattern = /^\| \[@([A-Za-z0-9-]+)\]\(https:\/\/github\.com\/[A-Za-z0-9-]+\) \| .+ \|$/;
  if (rows.some(row => !pattern.test(row))) throw new Error('Malformed contributor row.');
  const link = `[#${number}](${url})`;
  if (rows.some(row => row.includes(link))) return text;
  const index = rows.findIndex(row => row.match(pattern)[1].toLowerCase() === login.toLowerCase());
  if (index >= 0) rows[index] = `${rows[index].slice(0, -2)} · ${link} |`;
  else rows.push(`| [@${login}](${profile}) | ${link} |`);
  rows.sort((a, b) => a.match(pattern)[1].toLowerCase().localeCompare(b.match(pattern)[1].toLowerCase(), 'en'));
  lines.splice(first + 3, last - first - 3, ...rows);
  return lines.join(text.includes('\r\n') ? '\r\n' : '\n');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { values } = parseArgs({ options: Object.fromEntries(
    ['readme', 'login', 'profile-url', 'pr-number', 'pr-url'].map(key => [key, { type: 'string' }])) });
  const original = readFileSync(values.readme, 'utf8');
  const updated = updateContributors(original, {
    login: values.login, profile: values['profile-url'], number: Number(values['pr-number']), url: values['pr-url'],
  });
  if (updated !== original) writeFileSync(values.readme, updated);
  console.log(updated === original ? 'Contributor already credited.' : 'Contributor credit added.');
}
