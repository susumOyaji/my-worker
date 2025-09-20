import { SELF } from 'cloudflare:test';
import { describe, it, expect } from 'vitest';

describe('Hello World worker', () => {
	it('responds with Hello World! (integration style)', async () => {
		const response = await SELF.fetch('http://example.com');
		expect(await response.text()).toMatchInlineSnapshot(`"Hello World!"`);
	});
});