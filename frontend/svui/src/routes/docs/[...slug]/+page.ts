import { base } from '$app/paths';
import type { PageLoad } from './$types';

export const prerender = false;

type DocsNavItem = {
	title: string;
	routePath: string | null;
	depth: number;
	kind: 'directory' | 'document';
};

type DocsManifest = {
	generated_at: string;
	items: DocsNavItem[];
};

type DocsPageData = {
	markdown: string;
	docPath: string;
	requestPath: string;
	resolvedRoutePath: string;
	notFound: boolean;
	manifest: DocsManifest;
};

function normalizeRequestPath(slug?: string): string {
	if (!slug) {
		return '';
	}

	return slug.replace(/^\/+|\/+$/g, '');
}

function createCandidates(requestPath: string): Array<{ assetPath: string; routePath: string }> {
	if (requestPath === '') {
		return [{ assetPath: 'docs/index.md', routePath: '' }];
	}

	return [
		{ assetPath: `docs/${requestPath}.md`, routePath: requestPath },
		{ assetPath: `docs/${requestPath}/index.md`, routePath: requestPath }
	];
}

function toDocPath(assetPath: string): string {
	return assetPath.replace(/^docs\//, '');
}

function toNotFoundMarkdown(requestPath: string): string {
	const pathLabel = requestPath ? `docs/${requestPath}` : 'docs/index';
	return `# Document not found\n\nNo markdown file was found for **${pathLabel}**.`;
}

async function loadManifest(fetcher: typeof fetch): Promise<DocsManifest> {
	const response = await fetcher(`${base}/docs/manifest.json`);
	if (!response.ok) {
		return { generated_at: new Date().toISOString(), items: [] };
	}

	const parsed = (await response.json()) as Partial<DocsManifest>;
	if (!Array.isArray(parsed.items)) {
		return { generated_at: new Date().toISOString(), items: [] };
	}

	return {
		generated_at: typeof parsed.generated_at === 'string' ? parsed.generated_at : new Date().toISOString(),
		items: parsed.items.filter((item): item is DocsNavItem => {
			if (typeof item !== 'object' || item === null) return false;
			return (
				typeof item.title === 'string' &&
				(item.routePath === null || typeof item.routePath === 'string') &&
				typeof item.depth === 'number' &&
				(item.kind === 'directory' || item.kind === 'document')
			);
		})
	};
}

export const load: PageLoad = async ({ fetch, params }): Promise<DocsPageData> => {
	const requestPath = normalizeRequestPath(params.slug);
	const candidates = createCandidates(requestPath);
	const manifest = await loadManifest(fetch);

	for (const candidate of candidates) {
		const response = await fetch(`${base}/${candidate.assetPath}`);
		if (!response.ok) {
			continue;
		}

		const markdown = await response.text();
		return {
			markdown,
			docPath: toDocPath(candidate.assetPath),
			requestPath,
			resolvedRoutePath: candidate.routePath,
			notFound: false,
			manifest
		};
	}

	return {
		markdown: toNotFoundMarkdown(requestPath),
		docPath: `${requestPath || 'index'}.md`,
		requestPath,
		resolvedRoutePath: requestPath,
		notFound: true,
		manifest
	};
};
