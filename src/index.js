/**
 * Welcome to Cloudflare Workers! This is your first worker.
 *
 * - Run `npm run dev` in your terminal to start a development server
 * - Open a browser tab at http://localhost:8787/ to see your worker in action
 * - Run `npm run deploy` to publish your worker
 *
 * Learn more at https://developers.cloudflare.com/workers/
 */

// Import echo_word directly from the glue code
import { echo_word } from '../dist/index_bg.js';

export default {
	async fetch(request, env, ctx) {
		// Wasm initialization is handled by the bundled dist/index.js and its imports.
		// No explicit init() call needed here.

		let url;
		try {
			url = new URL(request.url);
		} catch (e) {
			return new Response(`Error parsing URL: ${e.message}. Request URL was: ${request.url}`, { status: 500 });
		}

		const name = url.searchParams.get('name') || 'World';

		const echoedWord = echo_word(name);

		return new Response(`Echoed: ${echoedWord}`);
	},
};