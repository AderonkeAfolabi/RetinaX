const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const docsDir = path.join(root, 'docs');
const onboardingDir = path.join(docsDir, 'onboarding');

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function walk(dir, predicate) {
  const found = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (['.git', 'node_modules', 'target'].includes(entry.name)) continue;
      found.push(...walk(fullPath, predicate));
    } else if (predicate(entry.name)) {
      found.push(fullPath);
    }
  }
  return found;
}

function markdownFiles(dir) {
  return walk(dir, (name) => name.endsWith('.md')).sort();
}

function relative(file) {
  return path.relative(root, file).split(path.sep).join('/');
}

function markdownLinks(markdown) {
  const stripped = markdown.replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]*`/g, '');
  const links = [];
  const pattern = /\[[^\]]*\]\(([^)]+)\)/g;
  let match = pattern.exec(stripped);
  while (match !== null) {
    links.push(match[1].trim());
    match = pattern.exec(stripped);
  }
  return links;
}

function isExternal(link) {
  return /^(https?:|mailto:|tel:|data:|file:|#)/i.test(link);
}

function localPath(link) {
  return link.split('#')[0].split('?')[0];
}

function sections(markdown) {
  const result = new Map();
  const parts = markdown.split(/^## /m);
  for (const part of parts.slice(1)) {
    const newline = part.indexOf('\n');
    result.set(part.slice(0, newline).trim(), part.slice(newline + 1));
  }
  return result;
}

function decode(target) {
  try {
    return decodeURIComponent(target);
  } catch {
    return target;
  }
}

const audienceHeadings = ['Contributors', 'Integrators', 'Operators', 'Auditors'];
const audienceCounts = { Contributors: 50, Integrators: 31, Operators: 18, Auditors: 21 };
const contractDocs = [
  'access_control',
  'analytics',
  'audit',
  'clinical_records',
  'compliance',
  'cross_chain',
  'emr_bridge',
  'fhir',
  'governor',
  'identity',
  'metering',
  'staking',
  'treasury',
  'zk_verifier',
  'zk_voting',
];
const diagramSymbols = {
  audit: ['create_segment', 'append_entry', 'append_entry_with_checks'],
  fhir: ['register_resource', 'get_resource', 'validate_observation_r4'],
  identity: ['initiate_recovery', 'approve_recovery', 'execute_recovery', 'verify_zk_credential'],
  zk_verifier: ['verify_access', 'verify_auth_level_access', 'verify_access_plonk'],
  analytics: ['encrypt', 'add_ciphertexts'],
  compliance: ['access_control', 'retention'],
  cross_chain: ['add_relayer', 'process_message'],
  emr_bridge: ['register_provider', 'sync_record'],
  governor: ['propose', 'Timelock'],
  metering: ['register_tenant', 'record_gas'],
  staking: ['stake', 'claim_rewards', 'unstake'],
  treasury: ['create_transfer_proposal', 'governor_spend'],
  zk_voting: ['set_merkle_root', 'cast_vote', 'nullifier'],
};

test('docs index groups every guide by audience and links to real files', () => {
  const index = read('docs/README.md');
  const headings = [...index.matchAll(/^## (.+)$/gm)].map((match) => match[1]);
  assert.deepEqual(headings, audienceHeadings);

  const byAudience = sections(index);
  for (const heading of audienceHeadings) {
    const bullets = byAudience
      .get(heading)
      .split('\n')
      .filter((line) => line.startsWith('- ['));
    assert.equal(bullets.length, audienceCounts[heading], `${heading} bullet count`);
    for (const bullet of bullets) {
      assert.match(bullet, /^- \[[^\]]+\]\([^)]+\) — .+\.$/, bullet);
    }
  }

  const indexLinks = markdownLinks(index).filter((link) => !isExternal(link));
  assert.equal(indexLinks.length, new Set(indexLinks).size, 'duplicate docs index link');
  const expectedDocs = markdownFiles(docsDir)
    .map((file) => path.relative(docsDir, file).split(path.sep).join('/'))
    .filter((file) => file !== 'README.md')
    .sort();
  assert.deepEqual(indexLinks.slice().sort(), expectedDocs);

  for (const link of indexLinks) {
    assert.ok(
      fs.existsSync(path.resolve(docsDir, decode(link))),
      `missing docs index target: ${link}`
    );
  }
});

test('root README links the documentation index and current CI workflows', () => {
  const readme = read('README.md');
  assert.ok(readme.includes('[Documentation Index](docs/README.md)'));
  assert.ok(readme.includes('`markdown-links.yml`'));
  assert.ok(readme.includes('cargo-test.yml/badge.svg'));
  assert.ok(!readme.includes('workflows/ci.yml'));
});

test('markdown link workflow and lychee config are wired for PRs', () => {
  const workflow = read('.github/workflows/markdown-links.yml');
  const config = read('lychee.toml');
  assert.ok(workflow.includes('name: Markdown Link Check'));
  assert.ok(workflow.includes('lycheeverse/lychee-action@v2'));
  assert.ok(workflow.includes('fail: true'));
  assert.ok(workflow.includes("'**/*.md'"));
  assert.ok(workflow.includes('pull_request:'));
  assert.ok(workflow.includes('paths:'));
  assert.ok(config.includes('localhost'));
  assert.ok(config.includes('127\\.0\\.0\\.1'));
});

test('all relative markdown links resolve locally', () => {
  for (const file of markdownFiles(root)) {
    const content = fs.readFileSync(file, 'utf8');
    for (const link of markdownLinks(content)) {
      if (isExternal(link)) continue;
      const target = localPath(link);
      if (target === '') continue;
      const resolved = path.resolve(path.dirname(file), decode(target));
      assert.ok(fs.existsSync(resolved), `${relative(file)}: broken local link ${link}`);
    }
  }
});

test('contract API docs include Mermaid diagrams matching code symbols', () => {
  for (const name of contractDocs) {
    const content = read(`docs/api/${name}.md`);
    const blocks = content.match(/```mermaid[\s\S]*?```/g) || [];
    assert.ok(blocks.length > 0, `missing Mermaid diagram in ${name}.md`);
    for (const block of blocks) {
      assert.match(
        block,
        /(flowchart|sequenceDiagram|graph|stateDiagram)/,
        `invalid Mermaid diagram in ${name}.md`
      );
    }
  }

  for (const [name, symbols] of Object.entries(diagramSymbols)) {
    const content = read(`docs/api/${name}.md`);
    const blocks = content.match(/```mermaid[\s\S]*?```/g) || [];
    const diagram = blocks.join('\n');
    for (const symbol of symbols) {
      assert.ok(diagram.includes(symbol), `${name}.md diagram missing ${symbol}`);
    }
  }
});

test('onboarding docs use current commands and contain no stale paths', () => {
  const files = markdownFiles(onboardingDir);
  const content = files.map((file) => fs.readFileSync(file, 'utf8')).join('\n');
  const stale = ['RetinaX-Contracts', 'retinax-contracts:dev', '19 total', 'upgrade.rs', 'v23.1.4'];
  for (const term of stale) {
    assert.ok(!content.includes(term), `stale onboarding term: ${term}`);
  }

  const lower = content.toLowerCase();
  const required = [
    'npm ci',
    'npm test',
    'npm run lint',
    'npm run format:check',
    'offchain-server',
    'test-js',
    'wasm-sizes',
    'good first issue',
  ];
  for (const term of required) {
    assert.ok(lower.includes(term.toLowerCase()), `missing onboarding term: ${term}`);
  }

  assert.ok(read('docs/onboarding/README.md').includes('good-first-issues.md'));
  assert.ok(
    read('docs/onboarding/first-contribution.md').includes('github.com/RetinaX-Org/RetinaX.git')
  );
});

test('Makefile runs the full website test suite', () => {
  const makefile = read('Makefile');
  assert.ok(makefile.includes('node --test website/*.test.js website/components/*.test.js'));
  assert.ok(makefile.includes('test-js:'));
});
