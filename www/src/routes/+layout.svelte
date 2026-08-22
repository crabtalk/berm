<script>
	import { siGithub } from 'simple-icons';
	import '../app.css';
	import { base } from '$app/paths';
	import Brand from '$lib/Brand.svelte';
	import { repo, repoApi } from '$lib/meta.js';

	let { children, data } = $props();

	// The build-time count goes stale between deploys, so refresh it once on
	// hydration. Unauthenticated, but it is one request per visitor against a
	// 60/hour per-IP limit, and the baked value stands when it fails.
	let stars = $state(data.stars);

	$effect(() => {
		fetch(repoApi)
			.then((response) => (response.ok ? response.json() : null))
			.then((json) => {
				if (typeof json?.stargazers_count === 'number') stars = json.stargazers_count;
			})
			.catch(() => {});
	});

	// Both destinations are built by something else — mdbook and rustdoc — and
	// land in this output after the site does, so neither is a route here.
	const nav = [
		{ title: 'Docs', href: `${base}/book/` },
		{ title: 'API', href: `${base}/api/` }
	];

	// One delegated handler for the whole site, so a block gets a working copy
	// button from its markup alone.
	async function copy(event) {
		const button = event.target.closest?.('.copy');
		if (!button) return;
		const code = button.parentElement?.querySelector('code');
		if (!code) return;
		try {
			await navigator.clipboard.writeText(code.textContent ?? '');
			button.classList.add('copied');
			setTimeout(() => button.classList.remove('copied'), 1400);
		} catch (error) {
			console.warn('copy failed', error);
		}
	}

	$effect(() => {
		document.addEventListener('click', copy);
		return () => document.removeEventListener('click', copy);
	});
</script>

<header>
	<a class="wordmark" href="{base}/">berm</a>

	<!-- Nav sits with the wordmark, not opposite it: the left group is where you
	     are in the project, the right group is what you can do about it. -->
	<nav class="nav">
		{#each nav as item (item.title)}
			<a href={item.href}>{item.title}</a>
		{/each}
	</nav>

	<nav class="links">
		<a class="action" href={repo}>
			Star
			<Brand icon={siGithub} size={14} />
			{#if stars !== null}<span class="count">{stars}</span>{/if}
		</a>
	</nav>
</header>

{@render children()}

<style>
	header {
		display: flex;
		align-items: center;
		gap: 26px;
		padding: 0 24px;
		height: 56px;
		border-bottom: 1px solid var(--line);
		position: sticky;
		top: 0;
		background: color-mix(in srgb, var(--bg) 88%, transparent);
		backdrop-filter: blur(12px);
		z-index: 20;
	}

	.wordmark {
		display: flex;
		align-items: center;
		color: var(--text);
		font-weight: 600;
		letter-spacing: -0.02em;
	}

	.nav {
		display: flex;
		align-items: center;
		gap: 20px;
		min-width: 0;
	}

	.nav a {
		color: var(--muted);
		font-size: 14px;
		font-weight: 500;
		white-space: nowrap;
	}

	.links {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-left: auto;
	}

	/* The label names the verb, the mark names the destination — so spelling the
	   destination out as well would say it twice. */
	.action {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		height: 32px;
		padding: 0 11px;
		border: 1px solid var(--line-strong);
		border-radius: 6px;
		color: var(--text);
	}

	.action:hover {
		border-color: var(--text);
		text-decoration: none;
	}

	.count {
		font-variant-numeric: tabular-nums;
		color: var(--muted);
		border-left: 1px solid var(--line-strong);
		padding-left: 8px;
	}

	header a:hover {
		color: var(--text);
		text-decoration: none;
	}
</style>
