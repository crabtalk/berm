import { sveltekit } from '@sveltejs/kit/vite';

// `vite dev` serves static files by exact path, so the two links that leave
// SvelteKit for mdbook and rustdoc need what Pages does for them in production:
// redirect the bare path, then serve the directory index.
const generated = {
	name: 'generated-docs-dev-host',
	configureServer(server) {
		server.middlewares.use((req, res, next) => {
			const [path, query] = req.url.split('?');
			const suffix = query ? `?${query}` : '';
			if (path === '/book' || path === '/api') {
				res.writeHead(308, { location: `${path}/${suffix}` });
				res.end();
				return;
			}
			if ((path.startsWith('/book/') || path.startsWith('/api/')) && path.endsWith('/')) {
				req.url = `${path}index.html${suffix}`;
			}
			next();
		});
	}
};

export default {
	plugins: [generated, sveltekit()]
};
