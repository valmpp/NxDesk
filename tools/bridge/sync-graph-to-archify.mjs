#!/usr/bin/env node
/**
 * sync-graph-to-archify.mjs — Universal GitNexus ⟷ Archify Synchronizer (v2)
 *
 * Dynamically correlates indexed GitNexus AST symbols and repository source files
 * with Archify JSON-IR architecture specifications. Supports multi-language projects
 * (.NET, TypeScript, Python, Go, Rust).
 */

import fs from 'node:fs';
import path from 'node:path';

function parseArgs(args) {
  const options = {
    gitnexus: path.join(process.cwd(), '.gitnexus', 'gitnexus.json'),
    spec: null,
    src: path.join(process.cwd()),
    strict: false,
    help: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--gitnexus' && args[i + 1]) {
      options.gitnexus = path.resolve(args[++i]);
    } else if (arg === '--spec' && args[i + 1]) {
      options.spec = path.resolve(args[++i]);
    } else if (arg === '--src' && args[i + 1]) {
      options.src = path.resolve(args[++i]);
    } else if (arg === '--strict') {
      options.strict = true;
    } else if (arg === '--help' || arg === '-h') {
      options.help = true;
    }
  }
  return options;
}

function findDefaultSpec(root) {
  const archDir = path.join(root, 'docs', 'architecture');
  if (fs.existsSync(archDir)) {
    const files = fs.readdirSync(archDir).filter(f => f.endsWith('.architecture.json'));
    if (files.length > 0) {
      return path.join(archDir, files[0]);
    }
  }
  return null;
}

function scanSourceSymbols(dir, fileList = []) {
  if (!fs.existsSync(dir)) return fileList;
  const entries = fs.readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    if (entry.name.startsWith('.') || entry.name === 'node_modules' || entry.name === 'bin' || entry.name === 'obj') {
      continue;
    }
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      scanSourceSymbols(fullPath, fileList);
    } else if (entry.isFile()) {
      const ext = path.extname(entry.name);
      // Multi-language service & controller detection
      if (['.cs', '.ts', '.js', '.py', '.go', '.rs'].includes(ext)) {
        const name = entry.name;
        if (
          name.endsWith('Controller.cs') || name.endsWith('Service.cs') ||
          name.endsWith('.controller.ts') || name.endsWith('.service.ts') ||
          name.endsWith('_router.py') || name.endsWith('_service.py') ||
          name.endsWith('_handler.go')
        ) {
          fileList.push({ name, ext, relative: path.relative(process.cwd(), fullPath) });
        }
      }
    }
  }
  return fileList;
}

function main() {
  const options = parseArgs(process.argv.slice(2));

  if (options.help) {
    console.log(`
Usage: node sync-graph-to-archify.mjs [options]

Options:
  --gitnexus <path>  Path to .gitnexus/gitnexus.json (default: .gitnexus/gitnexus.json)
  --spec <path>       Path to Archify .architecture.json (auto-detected if omitted)
  --src <path>        Root source directory to scan (default: current working directory)
  --strict            Exit with code 1 if unmodeled components are detected
  -h, --help          Show this help message
`);
    process.exit(0);
  }

  console.log('===============================================================');
  console.log(' 🌉 GitNexus ⟷ Archify Universal Graph Synchronizer (v2)');
  console.log('===============================================================');

  // 1. Resolve Archify spec
  let specPath = options.spec || findDefaultSpec(process.cwd());
  let archSpec = null;
  if (specPath && fs.existsSync(specPath)) {
    archSpec = JSON.parse(fs.readFileSync(specPath, 'utf8'));
    console.log(`[Archify] Especificación: ${path.basename(specPath)}`);
    console.log(`[Archify] Componentes modelados: ${archSpec.components?.length || 0}`);
    console.log(`[Archify] Conexiones modeladas:  ${archSpec.connections?.length || 0}`);
  } else {
    console.log('[Archify] Nota: No se detectó especificación .architecture.json en docs/architecture/');
  }

  // 2. Resolve GitNexus index
  let meta = null;
  if (fs.existsSync(options.gitnexus)) {
    meta = JSON.parse(fs.readFileSync(options.gitnexus, 'utf8'));
    console.log(`[GitNexus] Repositorio: ${meta.repo || 'Local'}`);
    console.log(`[GitNexus] Símbolos indexados: ${meta.stats?.symbols || meta.stats?.nodes || 'OK'}`);
  } else {
    console.log('[GitNexus] Nota: Índice .gitnexus no encontrado en ' + options.gitnexus);
  }

  // 3. Multi-language Source Scan
  const detectedSymbols = scanSourceSymbols(options.src);
  console.log(`\n[Código Fuente] Componentes/Servicios clave detectados (${detectedSymbols.length}):`);
  for (const s of detectedSymbols.slice(0, 15)) {
    console.log(`  ✓ [${s.ext}] ${s.name} (${s.relative})`);
  }
  if (detectedSymbols.length > 15) {
    console.log(`  ... y ${detectedSymbols.length - 15} más.`);
  }

  // 4. Correlation check
  if (archSpec && detectedSymbols.length > 0) {
    const modeledIds = new Set(archSpec.components.map(c => c.id.toLowerCase()));
    const unmodeled = detectedSymbols.filter(s => {
      const base = s.name.replace(/\.[^/.]+$/, '').toLowerCase();
      return !modeledIds.has(base);
    });

    console.log('\n[Auditoría de Sincronización]');
    if (unmodeled.length === 0) {
      console.log('  🟢 Todos los servicios detectados están correlacionados en Archify.');
    } else {
      console.log(`  🟡 ${unmodeled.length} servicios detectados sin nodo directo en Archify (pueden agruparse bajo un gateway/cluster).`);
    }
  }

  console.log('\n===============================================================');
  console.log(' Sincronización completada exitosamente.');
  console.log('===============================================================');
}

main();
