#!/usr/bin/env tsx
/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 *
 * Export documentation content to JSON for server-side FTS5 search indexing.
 * Run with: pnpm docs:index
 */

import { readFileSync, writeFileSync, readdirSync, statSync } from 'fs';
import { join, relative } from 'path';

interface ExportedDoc {
	doc_id: string;
	path: string;
	title: string;
	summary: string;
	diataxis: 'tutorial' | 'how-to' | 'reference' | 'explanation';
	tags: string[];
	updated_at: string;
	body: string;
}

interface ExportedDocsIndex {
	version: number;
	generated_at: string;
	docs: ExportedDoc[];
}

const DOCS_ROOT = 'src/routes/(docs)/docs';
const OUTPUT_PATH = 'static/docs-index.json';

function extractFrontmatter(content: string): { frontmatter: Record<string, any>; body: string } {
	const match = content.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
	if (!match) {
		return { frontmatter: {}, body: content };
	}

	const frontmatterStr = match[1];
	const body = match[2];

	const frontmatter: Record<string, any> = {};
	let currentKey = '';
	let inArray = false;
	const arrayItems: string[] = [];

	for (const line of frontmatterStr.split('\n')) {
		if (line.startsWith('  - ')) {
			arrayItems.push(line.substring(4).trim());
		} else if (line.includes(':')) {
			if (inArray && currentKey) {
				frontmatter[currentKey] = arrayItems.slice();
				arrayItems.length = 0;
			}
			const colonIdx = line.indexOf(':');
			currentKey = line.substring(0, colonIdx).trim();
			const value = line.substring(colonIdx + 1).trim();
			if (value === '') {
				inArray = true;
			} else {
				frontmatter[currentKey] = value;
				inArray = false;
			}
		}
	}
	if (inArray && currentKey) {
		frontmatter[currentKey] = arrayItems;
	}

	return { frontmatter, body };
}

function stripMarkdownToText(markdown: string): string {
	let text = markdown;

	// Remove script tags and their content
	text = text.replace(/<script[\s\S]*?<\/script>/gi, '');

	// Remove HTML/Svelte tags
	text = text.replace(/<[^>]+>/g, ' ');

	// Remove code blocks
	text = text.replace(/```[\s\S]*?```/g, ' ');
	text = text.replace(/`[^`]+`/g, ' ');

	// Remove markdown links but keep text
	text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');

	// Remove markdown formatting
	text = text.replace(/[*_#~]+/g, '');

	// Remove frontmatter-like content that might remain
	text = text.replace(/^---[\s\S]*?---/m, '');

	// Normalize whitespace
	text = text.replace(/\s+/g, ' ').trim();

	return text;
}

function pathToDiataxis(filePath: string): 'tutorial' | 'how-to' | 'reference' | 'explanation' | null {
	if (filePath.includes('/tutorials/')) return 'tutorial';
	if (filePath.includes('/how-to/')) return 'how-to';
	if (filePath.includes('/reference/')) return 'reference';
	if (filePath.includes('/explanation/')) return 'explanation';
	return null;
}

function findSvxFiles(dir: string): string[] {
	const results: string[] = [];

	function walk(currentDir: string) {
		const entries = readdirSync(currentDir);
		for (const entry of entries) {
			const fullPath = join(currentDir, entry);
			const stat = statSync(fullPath);
			if (stat.isDirectory()) {
				walk(fullPath);
			} else if (entry === '+page.svx') {
				results.push(fullPath);
			}
		}
	}

	walk(dir);
	return results;
}

function main() {
	console.log('Exporting docs index...');

	const svxFiles = findSvxFiles(DOCS_ROOT);
	const docs: ExportedDoc[] = [];

	for (const filePath of svxFiles) {
		const content = readFileSync(filePath, 'utf-8');
		const { frontmatter, body } = extractFrontmatter(content);

		const diataxis = frontmatter.diataxis || pathToDiataxis(filePath);
		if (!diataxis) {
			console.warn(`Skipping ${filePath}: no diataxis category`);
			continue;
		}

		// Skip drafts
		if (frontmatter.draft === 'true' || frontmatter.draft === true) {
			console.log(`Skipping draft: ${filePath}`);
			continue;
		}

		// Extract doc_id and path from file path
		// e.g., src/routes/(docs)/docs/tutorials/getting-started/+page.svx
		// -> doc_id: tutorials/getting-started, path: /docs/tutorials/getting-started
		const relPath = relative(DOCS_ROOT, filePath);
		const docId = relPath.replace('/+page.svx', '').replace('+page.svx', '');
		const urlPath = `/docs/${docId}`;

		const plainTextBody = stripMarkdownToText(body);

		const doc: ExportedDoc = {
			doc_id: docId,
			path: urlPath,
			title: frontmatter.title || docId.split('/').pop() || 'Untitled',
			summary: frontmatter.summary || '',
			diataxis: diataxis as ExportedDoc['diataxis'],
			tags: Array.isArray(frontmatter.tags) ? frontmatter.tags : [],
			updated_at: frontmatter.updatedAt || new Date().toISOString(),
			body: plainTextBody,
		};

		docs.push(doc);
		console.log(`  Indexed: ${doc.path} (${doc.title})`);
	}

	const index: ExportedDocsIndex = {
		version: 1,
		generated_at: new Date().toISOString(),
		docs,
	};

	writeFileSync(OUTPUT_PATH, JSON.stringify(index, null, 2));
	console.log(`\nExported ${docs.length} docs to ${OUTPUT_PATH}`);
}

main();
