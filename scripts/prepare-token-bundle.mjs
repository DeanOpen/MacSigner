import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const plainName = value => typeof value === 'string' && /^[a-z0-9][a-z0-9_.-]*$/i.test(value)
  && !/[ .]$/.test(value) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(value);
const inside = (root, candidate) => {
  const relative = path.relative(root, candidate);
  return relative !== '' && !relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative);
};

function verifyArchitecture(bytes, name, platform, architecture) {
  if (name.endsWith('.sig')) return;
  let supported = false;
  if (platform === 'windows' && /\.(dll|exe)$/i.test(name) && bytes.length >= 64 && bytes.toString('ascii', 0, 2) === 'MZ') {
    const offset = bytes.readUInt32LE(60);
    supported = offset + 6 <= bytes.length && bytes.toString('ascii', offset, offset + 4) === 'PE\0\0'
      && bytes.readUInt16LE(offset + 4) === { x86: 0x14c, x86_64: 0x8664, aarch64: 0xaa64 }[architecture];
  } else if (platform === 'macos' && name.endsWith('.dylib') && bytes.length >= 8) {
    const magic = bytes.readUInt32BE(0);
    const littleEndian = [0xcefaedfe, 0xcffaedfe, 0xbebafeca, 0xbfbafeca].includes(magic);
    const number = offset => littleEndian ? bytes.readUInt32LE(offset) : bytes.readUInt32BE(offset);
    const cpus = [];
    if ([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe].includes(magic)) cpus.push(number(4));
    if ([0xcafebabe, 0xbebafeca, 0xcafebabf, 0xbfbafeca].includes(magic)) {
      const width = [0xcafebabf, 0xbfbafeca].includes(magic) ? 32 : 20;
      const count = number(4);
      if (count > 0 && count <= 64 && 8 + count * width <= bytes.length) {
        for (let index = 0; index < count; index++) cpus.push(number(8 + index * width));
      }
    }
    supported = architecture === 'universal' ? [0x01000007, 0x0100000c].every(cpu => cpus.includes(cpu))
      : cpus.includes({ x86: 7, x86_64: 0x01000007, aarch64: 0x0100000c }[architecture]);
  } else if (platform === 'linux' && /\.so(?:\.|$)/.test(name) && bytes.length >= 20 && bytes.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
    const machine = bytes[5] === 1 ? bytes.readUInt16LE(18) : bytes.readUInt16BE(18);
    supported = [1, 2].includes(bytes[5]) && machine === { x86: 3, x86_64: 62, aarch64: 183 }[architecture];
  }
  if (!supported) throw new Error(`Connector format or architecture mismatch: ${name} for ${platform}/${architecture}`);
}

async function sourceBytes(root, source) {
  if (typeof source !== 'string' || source.includes('\\') || path.posix.isAbsolute(source)
    || source.split('/').some(part => !part || part === '.' || part === '..' || part.includes(':'))) {
    throw new Error('Invalid relative source path');
  }
  let current = root;
  for (const part of source.split('/')) {
    current = path.join(current, part);
    if ((await lstat(current)).isSymbolicLink()) throw new Error('Symbolic-link source is not allowed');
  }
  if (!inside(root, await realpath(current))) throw new Error('Source escapes source root');
  const metadata = await lstat(current);
  if (!metadata.isFile() || metadata.size > 64 * 1024 * 1024) throw new Error('Invalid source file size or type');
  return readFile(current);
}

export async function prepareTokenBundle({ manifest, sourceRoot, output, platform, architecture }) {
  if (manifest.schema !== 1 || !Array.isArray(manifest.connectors)
    || !['windows', 'macos', 'linux'].includes(platform)
    || !['x86', 'x86_64', 'aarch64', 'universal'].includes(architecture)) {
    throw new Error('Invalid bundle manifest or target');
  }
  const source = await realpath(sourceRoot);
  const destination = path.resolve(output);
  if (destination === source || inside(destination, source)) throw new Error('Output cannot contain the source root');
  const selected = manifest.connectors.filter(connector => connector.platform === platform
    && (connector.architecture === architecture || (platform === 'windows' && architecture === 'x86_64' && connector.architecture === 'x86')));
  if (!selected.length) throw new Error(`No approved connectors for ${platform}/${architecture}; refusing an empty compatibility bundle`);
  const files = [];
  const destinations = new Set();
  const ids = new Set();
  for (const connector of selected) {
    if (!plainName(connector.id) || !['x86', 'x86_64', 'aarch64', 'universal'].includes(connector.architecture)
      || !Array.isArray(connector.files) || !connector.files.length
      || !['certificates-read', 'signing-tested', 'static-only'].includes(connector.evidence)) {
      throw new Error('Invalid connector entry');
    }
    const identity = `${connector.id}/${connector.architecture}`;
    if (ids.has(identity)) throw new Error('Duplicate connector identity');
    ids.add(identity);
    for (const file of connector.files) {
      if (!plainName(file.name) || !/^[a-f0-9]{64}$/.test(file.sha256)) throw new Error('Invalid connector file');
      const target = `pkcs11/${identity}/${file.name}`;
      if (destinations.has(target.toLowerCase())) throw new Error('Duplicate resource destination');
      destinations.add(target.toLowerCase());
      const bytes = await sourceBytes(source, file.source);
      if (digest(bytes) !== file.sha256) throw new Error(`Hash mismatch: ${file.source}`);
      verifyArchitecture(bytes, file.name, platform, connector.architecture);
      files.push({ target, bytes, sha256: file.sha256 });
    }
  }
  if (selected.some(connector => connector.architecture === 'x86') && architecture === 'x86_64') {
    const worker = manifest.workers?.windows_x86;
    if (!worker || !/^[a-f0-9]{64}$/.test(worker.sha256)) throw new Error('Missing approved x86 worker');
    const bytes = await sourceBytes(source, worker.source);
    if (digest(bytes) !== worker.sha256) throw new Error('Worker hash mismatch');
    verifyArchitecture(bytes, 'macsigner-token-x86.exe', 'windows', 'x86');
    files.push({ target: 'macsigner-token-x86.exe', bytes, sha256: worker.sha256 });
  }
  await mkdir(destination);
  const resources = {};
  for (const file of files) {
    const resourcePath = path.join(destination, 'resources', file.target);
    await mkdir(path.dirname(resourcePath), { recursive: true });
    await writeFile(resourcePath, file.bytes, { flag: 'wx' });
    resources[resourcePath.replaceAll('\\', '/')] = file.target;
  }
  const report = {
    schema: 1, platform, architecture,
    connectors: selected.map(({ id, architecture: cpu, evidence }) => ({ id, architecture: cpu, evidence })),
    files: files.map(({ target, sha256 }) => ({ target, sha256 })),
  };
  const reportPath = path.join(destination, 'token-bundle.json');
  await writeFile(reportPath, JSON.stringify(report, null, 2), { flag: 'wx' });
  resources[reportPath.replaceAll('\\', '/')] = 'token-bundle.json';
  const config = path.join(destination, 'tauri.tokens.conf.json');
  await writeFile(config, JSON.stringify({ bundle: { resources, createUpdaterArtifacts: false } }, null, 2), { flag: 'wx' });
  return { config, report };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [manifestPath, sourceRoot, output, platform, architecture] = process.argv.slice(2);
  if (!architecture) throw new Error('Usage: prepare-token-bundle.mjs MANIFEST SOURCE_ROOT NEW_OUTPUT PLATFORM ARCHITECTURE');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const result = await prepareTokenBundle({ manifest, sourceRoot, output, platform, architecture });
  console.log(JSON.stringify(result));
}
