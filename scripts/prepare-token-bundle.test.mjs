import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { prepareTokenBundle } from './prepare-token-bundle.mjs';

async function fixture(work) {
  const sourceRoot = await mkdtemp(path.join(os.tmpdir(), 'macsigner-bundle-'));
  const bytes = Buffer.alloc(32);
  bytes.writeUInt32LE(0xfeedfacf, 0);
  bytes.writeUInt32LE(0x0100000c, 4);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  await writeFile(path.join(sourceRoot, 'token.dylib'), bytes);
  const manifest = { schema: 1, connectors: [{
    id: 'fixture', platform: 'macos', architecture: 'aarch64', evidence: 'static-only',
    files: [{ source: 'token.dylib', name: 'token.dylib', sha256 }],
  }] };
  try {
    await work({ manifest, sourceRoot, output: path.join(sourceRoot, 'bundle'), platform: 'macos', architecture: 'aarch64' });
  } finally {
    await rm(sourceRoot, { recursive: true, force: true });
  }
}

test('stages original bytes and maps resources where macOS discovery looks', () => fixture(async options => {
  const { config, report } = await prepareTokenBundle(options);
  const configuration = JSON.parse(await readFile(config, 'utf8'));
  assert.equal(configuration.bundle.createUpdaterArtifacts, false);
  assert.ok(Object.values(configuration.bundle.resources).includes('pkcs11/fixture/aarch64/token.dylib'));
  assert.deepEqual(await readFile(path.join(options.output, 'resources/pkcs11/fixture/aarch64/token.dylib')), await readFile(path.join(options.sourceRoot, 'token.dylib')));
  assert.equal(report.connectors[0].evidence, 'static-only');
  await assert.rejects(prepareTokenBundle(options), /EEXIST/);
}));

test('refuses missing target coverage rather than producing a misleading bundle', () => fixture(async options => {
  await assert.rejects(prepareTokenBundle({ ...options, platform: 'windows' }), /No approved connectors/);
  await assert.rejects(prepareTokenBundle({ ...options, architecture: 'universal' }), /No approved connectors/);
}));

test('rejects changed files before creating output', () => fixture(async options => {
  await writeFile(path.join(options.sourceRoot, 'token.dylib'), 'changed');
  await assert.rejects(prepareTokenBundle(options), /Hash mismatch/);
  await assert.rejects(readFile(path.join(options.output, 'tauri.tokens.conf.json')), /ENOENT/);
}));

test('rejects traversal, reserved names and duplicate resource identities', () => fixture(async options => {
  const file = options.manifest.connectors[0].files[0];
  for (const source of ['../token.dylib', '/token.dylib', 'C:/token.dylib', 'directory\\token.dylib']) {
    await assert.rejects(prepareTokenBundle({ ...options, manifest: { schema: 1, connectors: [{ ...options.manifest.connectors[0], files: [{ ...file, source }] }] } }), /Invalid relative source/);
  }
  file.name = 'con.dylib';
  await assert.rejects(prepareTokenBundle(options), /Invalid connector file/);
  file.name = 'token.dylib';
  options.manifest.connectors.push(options.manifest.connectors[0]);
  await assert.rejects(prepareTokenBundle(options), /Duplicate connector identity/);
}));

test('requires a hash-approved worker for x86 connectors in the Windows x64 app', () => fixture(async options => {
  options.platform = 'windows';
  options.architecture = 'x86_64';
  options.manifest.connectors[0].platform = 'windows';
  options.manifest.connectors[0].architecture = 'x86';
  const bytes = Buffer.alloc(70);
  bytes.write('MZ');
  bytes.writeUInt32LE(64, 60);
  bytes.write('PE\0\0', 64);
  bytes.writeUInt16LE(0x14c, 68);
  await writeFile(path.join(options.sourceRoot, 'token.dll'), bytes);
  options.manifest.connectors[0].files = [{ source: 'token.dll', name: 'token.dll', sha256: createHash('sha256').update(bytes).digest('hex') }];
  await assert.rejects(prepareTokenBundle(options), /Missing approved x86 worker/);
}));

test('rejects a hash-matching Intel module in an ARM64 manifest', () => fixture(async options => {
  const bytes = await readFile(path.join(options.sourceRoot, 'token.dylib'));
  bytes.writeUInt32LE(0x01000007, 4);
  await writeFile(path.join(options.sourceRoot, 'token.dylib'), bytes);
  options.manifest.connectors[0].files[0].sha256 = createHash('sha256').update(bytes).digest('hex');
  await assert.rejects(prepareTokenBundle(options), /architecture mismatch/);
}));
