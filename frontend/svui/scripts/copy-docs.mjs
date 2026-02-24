import { cp, mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const sourceDir = resolve(__dirname, '..', '..', '..', 'docs');
const buildDir = resolve(__dirname, '..', '..', 'build');
const targetDir = resolve(buildDir, 'docs');
const manifestPath = resolve(targetDir, 'manifest.json');

function toTitleCase(raw) {
	return raw
		.replace(/[-_]+/g, ' ')
		.replace(/\s+/g, ' ')
		.trim()
		.replace(/\b\w/g, (char) => char.toUpperCase());
}

function routePathFromRelativeMarkdown(relativePath) {
	const normalized = relativePath.replace(/\\/g, '/').replace(/\.md$/i, '');
	if (normalized === 'index') {
		return '';
	}
	if (normalized.endsWith('/index')) {
		return normalized.slice(0, -'/index'.length);
	}
	return normalized;
}

async function extractTitle(filePath, fallback) {
	const content = await readFile(filePath, 'utf8');
	const lines = content.split(/\r?\n/);
	for (const line of lines) {
		const heading = line.match(/^#\s+(.+)$/);
		if (heading) {
			return heading[1].trim();
		}
	}
	return fallback;
}

async function buildManifestItems() {
	const items = [];

	async function walk(currentDir, depth, relativeDir = '') {
		const entries = await readdir(currentDir, { withFileTypes: true });
		entries.sort((a, b) => a.name.localeCompare(b.name));

		const directories = entries.filter((entry) => entry.isDirectory());
		const files = entries.filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.md'));

		if (relativeDir) {
			items.push({
				title: toTitleCase(relativeDir.split('/').at(-1) ?? relativeDir),
				routePath: null,
				depth,
				kind: 'directory'
			});
		}

		files.sort((a, b) => {
			const aIndex = a.name.toLowerCase() === 'index.md' ? -1 : 0;
			const bIndex = b.name.toLowerCase() === 'index.md' ? -1 : 0;
			if (aIndex !== bIndex) {
				return aIndex - bIndex;
			}
			return a.name.localeCompare(b.name);
		});

		for (const file of files) {
			const filePath = resolve(currentDir, file.name);
			const relativePath = relativeDir ? `${relativeDir}/${file.name}` : file.name;
			const fallbackTitle = toTitleCase(file.name.replace(/\.md$/i, ''));
			const title = await extractTitle(filePath, fallbackTitle);
			items.push({
				title,
				routePath: routePathFromRelativeMarkdown(relativePath),
				depth: depth + 1,
				kind: 'document'
			});
		}

		for (const directory of directories) {
			const childRelativeDir = relativeDir ? `${relativeDir}/${directory.name}` : directory.name;
			await walk(resolve(currentDir, directory.name), depth + 1, childRelativeDir);
		}
	}

	await walk(sourceDir, -1);

	return items;
}

async function copyDocs() {
	await mkdir(buildDir, { recursive: true });
	await rm(targetDir, { recursive: true, force: true });
	await mkdir(targetDir, { recursive: true });
	await cp(sourceDir, targetDir, { recursive: true });

	const manifest = {
		generated_at: new Date().toISOString(),
		items: await buildManifestItems()
	};

	await writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf8');
	console.log(`Copied docs from ${sourceDir} to ${targetDir}`);
	console.log(`Generated docs manifest at ${manifestPath}`);
}

copyDocs().catch((error) => {
	console.error('Failed to copy docs for UI build.');
	console.error(error);
	process.exit(1);
});
