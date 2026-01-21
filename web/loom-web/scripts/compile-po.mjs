#!/usr/bin/env node
/**
 * Custom PO compiler that preserves msgid as the key in the output.
 * This is needed because the app uses explicit message IDs like 'nav.threads'
 * instead of hashed IDs.
 */

import { readFileSync, writeFileSync, readdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const localesDir = join(__dirname, '../src/locales');

function parsePo(content) {
	const messages = {};
	const entries = content.split(/\n\n+/);

	for (const entry of entries) {
		const lines = entry.split('\n');
		let msgid = '';
		let msgstr = '';
		let inMsgid = false;
		let inMsgstr = false;

		for (const line of lines) {
			if (line.startsWith('msgid "')) {
				inMsgid = true;
				inMsgstr = false;
				msgid = line.slice(7, -1);
			} else if (line.startsWith('msgstr "')) {
				inMsgid = false;
				inMsgstr = true;
				msgstr = line.slice(8, -1);
			} else if (line.startsWith('"') && line.endsWith('"')) {
				const val = line.slice(1, -1);
				if (inMsgid) msgid += val;
				if (inMsgstr) msgstr += val;
			}
		}

		if (msgid && msgstr) {
			messages[unescapePoString(msgid)] = unescapePoString(msgstr);
		}
	}

	return messages;
}

function unescapePoString(str) {
	return str
		.replace(/\\n/g, '\n')
		.replace(/\\"/g, '"')
		.replace(/\\\\/g, '\\');
}

function escapeJsString(str) {
	return str
		.replace(/\\/g, '\\\\')
		.replace(/"/g, '\\"')
		.replace(/\n/g, '\\n');
}

const locales = readdirSync(localesDir);

for (const locale of locales) {
	const poPath = join(localesDir, locale, 'messages.po');
	const tsPath = join(localesDir, locale, 'messages.ts');

	try {
		const content = readFileSync(poPath, 'utf-8');
		const messages = parsePo(content);

		const jsonObj = JSON.stringify(messages);
		const escapedJson = jsonObj.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
		const output = `/*eslint-disable*/import type{Messages}from"@lingui/core";export const messages=JSON.parse("${escapedJson}") as Messages;`;

		writeFileSync(tsPath, output);
		console.log(`Compiled ${locale}: ${Object.keys(messages).length} messages`);
	} catch (err) {
		console.error(`Error processing ${locale}: ${err.message}`);
	}
}

console.log('Done!');
