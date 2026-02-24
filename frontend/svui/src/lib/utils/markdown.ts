import { marked } from 'marked';
import DOMPurify from 'dompurify';

// Configure marked for GitHub Flavored Markdown
marked.setOptions({
	breaks: true,
	gfm: true
});

export type MarkdownHeading = {
	id: string;
	text: string;
	level: number;
};

type ParseMarkdownOptions = {
	docPath?: string;
	docsRouteBase?: string;
};

function isLikelyExternalHref(href: string): boolean {
	return /^[a-zA-Z][a-zA-Z\d+\-.]*:/.test(href) || href.startsWith('//');
}

function normalizeDocsRouteBase(routeBase: string): string {
	if (!routeBase) {
		return '/docs';
	}

	return routeBase.endsWith('/') ? routeBase.slice(0, -1) : routeBase;
}

function normalizeDocsRoutePath(pathname: string): string {
	const trimmed = pathname.replace(/^\/+|\/+$/g, '');
	if (trimmed === '' || trimmed === 'index') {
		return '';
	}

	if (trimmed.endsWith('/index')) {
		return trimmed.slice(0, -'/index'.length);
	}

	return trimmed;
}

function slugifyHeading(text: string): string {
	return text
		.toLowerCase()
		.replace(/<[^>]*>/g, '')
		.replace(/[^a-z0-9\s-]/g, '')
		.trim()
		.replace(/\s+/g, '-');
}

function rewriteMarkdownLinks(documentNode: Document, docPath: string, docsRouteBase: string) {
	const baseDocUrl = new URL(docPath, 'https://docs.local/');
	const normalizedDocsRouteBase = normalizeDocsRouteBase(docsRouteBase);

	for (const anchor of documentNode.querySelectorAll<HTMLAnchorElement>('a[href]')) {
		const href = anchor.getAttribute('href');
		if (!href) {
			continue;
		}

		if (href.startsWith('#') || href.startsWith('/')) {
			continue;
		}

		if (isLikelyExternalHref(href)) {
			continue;
		}

		const [rawPath, fragment] = href.split('#', 2);
		if (!rawPath.toLowerCase().endsWith('.md')) {
			continue;
		}

		const resolved = new URL(rawPath, baseDocUrl);
		const withoutExtension = resolved.pathname.replace(/\.md$/i, '');
		const routePath = normalizeDocsRoutePath(withoutExtension);
		const suffix = fragment ? `#${fragment}` : '';
		const nextHref = routePath
			? `${normalizedDocsRouteBase}/${routePath}${suffix}`
			: `${normalizedDocsRouteBase}${suffix}`;

		anchor.setAttribute('href', nextHref);
	}
}

function addHeadingIds(documentNode: Document): MarkdownHeading[] {
	const headings: MarkdownHeading[] = [];
	const seenIds = new Map<string, number>();

	for (const heading of documentNode.querySelectorAll<HTMLElement>('h1, h2, h3, h4, h5, h6')) {
		const level = Number.parseInt(heading.tagName.slice(1), 10);
		const text = heading.textContent?.trim() ?? '';
		if (!text) {
			continue;
		}

		const baseId = slugifyHeading(text) || 'section';
		const count = seenIds.get(baseId) ?? 0;
		seenIds.set(baseId, count + 1);
		const id = count === 0 ? baseId : `${baseId}-${count}`;

		heading.id = id;
		if (level <= 3) {
			headings.push({ id, text, level });
		}
	}

	return headings;
}

function sanitizeHtml(html: string): string {
	return DOMPurify.sanitize(html);
}

export function parseMarkdownDocument(
	text: string,
	options?: ParseMarkdownOptions
): { html: string; headings: MarkdownHeading[] } {
	const parsed = marked.parse(text);
	const rawHtml = typeof parsed === 'string' ? parsed : '';

	if (typeof DOMParser === 'undefined') {
		return { html: sanitizeHtml(rawHtml), headings: [] };
	}

	const parser = new DOMParser();
	const documentNode = parser.parseFromString(rawHtml, 'text/html');

	if (options?.docPath) {
		const routeBase = options.docsRouteBase ?? '/docs';
		rewriteMarkdownLinks(documentNode, options.docPath, routeBase);
	}

	const headings = addHeadingIds(documentNode);
	return { html: sanitizeHtml(documentNode.body.innerHTML), headings };
}

/**
 * Parse markdown text to sanitized HTML.
 * Prevents XSS attacks by sanitizing the output with DOMPurify.
 */
export function parseMarkdown(text: string): string {
	return parseMarkdownDocument(text).html;
}
