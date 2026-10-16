import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';

// Every docs page is read from `../docs` at load time, outside anything Vite
// tracks, so an edit there would otherwise need a restart by hand.
const docs = {
	name: 'watch-docs',
	configureServer(server) {
		const dir = resolve(fileURLToPath(new URL('../docs', import.meta.url)));
		server.watcher.add(dir);
		server.watcher.on('all', (event, file) => {
			if (!['add', 'change', 'unlink'].includes(event)) return;
			if (resolve(file).startsWith(`${dir}/`)) server.restart();
		});
	}
};

export default {
	plugins: [sveltekit(), docs]
};
