import adapter from '@sveltejs/adapter-static';

// No `paths.base` on purpose: SvelteKit emits relative links by default, so the
// same build serves from crabtalk.github.io/berm and from a local preview
// without a prefix to keep in sync.
export default {
	kit: {
		adapter: adapter({ fallback: '404.html' }),
		prerender: {
			handleHttpError: ({ path, message }) => {
				// The book and the API reference are directories under `static/`, not
				// routes. Pages resolves each to its index.html; the crawler does not,
				// so these are the two 404s that mean nothing. Every other one is real.
				if (path === '/book/' || path === '/api/') return;
				throw new Error(message);
			}
		}
	}
};
