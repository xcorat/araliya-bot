import { marked } from 'marked';
import DOMPurify from 'dompurify';

// Configure marked for GitHub Flavored Markdown
marked.setOptions({
	breaks: true,
	gfm: true
});

/**
 * Parse markdown text to sanitized HTML.
 * Prevents XSS attacks by sanitizing the output with DOMPurify.
 */
export function parseMarkdown(text: string): string {
	const parsed = marked.parse(text);
	const html = typeof parsed === 'string' ? parsed : '';
	return DOMPurify.sanitize(html);
}
