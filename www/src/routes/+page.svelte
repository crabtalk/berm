<script>
	import { siGithub, siX } from 'simple-icons';
	import { Check, Copy } from 'lucide-static';
	import { base } from '$app/paths';
	import Brand from '$lib/Brand.svelte';
	import Disasm from '$lib/Disasm.svelte';
	import { author, description, repo } from '$lib/meta.js';

	const title = 'berm — the OS for agent harnesses';

	// Marked by hand and rendered with `{@html}`, so a `pre` never inherits the
	// template's indentation and Svelte never reads a brace in the source as an
	// expression.
	const rust = `<span class="mark">let</span> berm = Berm::load(&amp;engine, &amp;elf, &amp;[])?;

<span class="mark">match</span> berm.call(<span class="s">"echo"</span>, <span class="s">br#"{"query":"hello"}"#</span>.to_vec())? {
    Ok(result) =&gt; println!(<span class="s">"{result}"</span>),
    Err(failure) =&gt; eprintln!(<span class="s">"{failure}"</span>),
}`;

	const digest = '222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d';

	const console_ = `<span class="c">$</span> berm push 127.0.0.1:5000/berm/fixture:v1 ./fixture
127.0.0.1:5000/berm/fixture:v1
  digest  <span class="mark">sha256:${digest}</span>

<span class="c">$</span> shasum -a256 ./fixture
<span class="mark">${digest}</span>  ./fixture`;

	const running = `<span class="c">$</span> bermd &amp;
<span class="c">$</span> berm deploy example ./program.elf
<span class="c">$</span> berm ls`;

	const moving = `<span class="c">$</span> berm push ghcr.io/org/example:v1 ./program.elf
<span class="c">$</span> berm publish ghcr.io/org/example:v1
<span class="c">$</span> berm search <span class="s">"read a file"</span>`;

	// What berm is, in its own terms — the README and the book already argue
	// these. Reasons to reach for it, not a list of what is in the box.
	const pillars = [
		{
			title: 'Pinned by hash',
			body: 'A program is one statically linked RV64 ELF. berm compiles it once and instantiates it per invocation — arguments go in through syscalls, the result comes back out of guest memory, and the instance is gone.'
		},
		{
			title: 'The linker is the boundary',
			body: 'A program reaches the world only through the syscalls it was given, and that list is the Linker it was instantiated with. A call to anything else traps because nothing is registered for it, not because a check said no.'
		},
		{
			title: 'berm has no host',
			body: 'berm ships no syscalls at all. What a filesystem is bounded by, and where bytes persist, are decisions about a host — so they belong to whoever is building one.'
		},
		{
			title: 'Read it without running it',
			body: 'Manifest::from_elf reads what an image claims to be — its tools, their schemas, when to reach for them — without compiling it and without running it.'
		}
	];

	const jsonLd = {
		'@context': 'https://schema.org',
		'@type': 'SoftwareSourceCode',
		name: 'berm',
		description,
		codeRepository: repo,
		programmingLanguage: 'Rust',
		license: 'https://www.apache.org/licenses/LICENSE-2.0',
		keywords: [
			'RISC-V sandbox',
			'RV64 ELF',
			'Cranelift JIT',
			'MCP server',
			'agent tools',
			'LLM tool sandbox',
			'OCI artifact',
			'Rust sandbox'
		]
	};
	const jsonLdHtml = `<script type="application/ld+json">${JSON.stringify(jsonLd)}<\/script>`;
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<!-- eslint-disable-next-line svelte/no-at-html-tags -->
	{@html jsonLdHtml}
</svelte:head>

<section class="hero">
	<div class="say">
		<h1>Nothing survives the call.</h1>

		<p class="lede">
			The OS for agent harnesses. One statically linked RV64 ELF, pinned by hash, compiled once,
			and instantiated per invocation.
		</p>

		<div class="install code-block">
			<span class="file">crates.io</span>
			<code>cargo add berm</code>
			<button class="copy" type="button" aria-label="Copy">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html Copy}{@html Check}
			</button>
		</div>

		<p class="facts">
			<span><strong>Apache-2.0</strong> licensed</span>
			<span>Compiled by <strong>Cranelift</strong></span>
			<span>Served over <strong>MCP</strong></span>
		</p>
	</div>

	<div class="act">
		<Disasm />
		<div class="cta">
			<a class="button primary" href="{base}/book/">Read the docs</a>
		</div>
	</div>
</section>

<section class="call">
	<div class="prose">
		<h2>Two results, and they mean different things.</h2>
		<p>
			The outer result is the host's — a missing tool, a trap. The inner one is the program
			reporting failure, which is a result the model should see.
		</p>
	</div>
	<div class="code-block">
		<pre><code>{@html rust}</code></pre>
		<button class="copy" type="button" aria-label="Copy code">
			<!-- eslint-disable-next-line svelte/no-at-html-tags -->
			{@html Copy}{@html Check}
		</button>
	</div>
</section>

<!-- Not a claim about the digest: the digest, twice, from two programs that were
     never told about each other. -->
<section class="stage">
	<div class="window">
		<div class="titlebar">
			<span class="light close" aria-hidden="true"></span>
			<span class="light minimise" aria-hidden="true"></span>
			<span class="light zoom" aria-hidden="true"></span>
			<span class="title">berm push</span>
		</div>
		<div class="code-block">
			<pre><code>{@html console_}</code></pre>
			<button class="copy" type="button" aria-label="Copy transcript">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html Copy}{@html Check}
			</button>
		</div>
	</div>
	<p class="note">
		A program travels as one OCI layer with no tarball around it, so the digest a registry
		addresses it by is <code>shasum</code> of the file on your disk.
	</p>
</section>

<section class="pillars">
	<div class="grid">
		{#each pillars as pillar (pillar.title)}
			<article>
				<h2>{pillar.title}</h2>
				<p>{pillar.body}</p>
			</article>
		{/each}
	</div>
</section>

<section class="cli">
	<div class="column">
		<h2>Running</h2>
		<p><code>bermd</code> serves every deployed program on one MCP endpoint, with tools named
			<code>{'{program}'}.{'{tool}'}</code>.</p>
		<div class="code-block">
			<pre><code>{@html running}</code></pre>
			<button class="copy" type="button" aria-label="Copy commands">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html Copy}{@html Check}
			</button>
		</div>
	</div>
	<div class="column">
		<h2>Moving</h2>
		<p>
			Pushing makes a program fetchable, not findable. The list is a git repository, so
			<code>search</code> reads a clone of it with no service and no credential.
		</p>
		<div class="code-block">
			<pre><code>{@html moving}</code></pre>
			<button class="copy" type="button" aria-label="Copy commands">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html Copy}{@html Check}
			</button>
		</div>
	</div>
</section>

<footer>
	<nav class="left">
		<a href="https://github.com/crabtalk">crabtalk</a>
		<a href="{base}/book/">Docs</a>
	</nav>
	<nav class="right">
		<a href={repo} aria-label="berm on GitHub"><Brand icon={siGithub} size={15} /></a>
		<a href={author} target="_blank" rel="noreferrer" aria-label="The author on X">
			<Brand icon={siX} size={14} />
		</a>
	</nav>
</footer>

<style>
	.hero {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 0.72fr);
		align-items: center;
		gap: 40px;
		max-width: 1180px;
		margin: 0 auto;
		padding: 112px 40px 56px;
	}

	.act {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		min-height: 220px;
	}

	.cta {
		position: relative;
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
		justify-content: flex-end;
	}

	.button {
		display: inline-flex;
		align-items: center;
		border: 1px solid var(--line-strong);
		border-radius: 6px;
		padding: 0 16px;
		height: 40px;
		color: var(--text);
		font-family: inherit;
		font-size: 14px;
		font-weight: 500;
		background: var(--bg);
		cursor: pointer;
	}

	.button:hover {
		border-color: var(--text);
		text-decoration: none;
	}

	.button.primary {
		background: var(--text);
		border-color: var(--text);
		color: #000;
	}

	.button.primary:hover {
		background: #fff;
		border-color: #fff;
	}

	h1 {
		font-size: clamp(40px, 6vw, 68px);
		line-height: 1.05;
		margin: 0;
		letter-spacing: -0.045em;
		font-weight: 600;
		max-width: 13ch;
	}

	.lede {
		margin: 22px 0 0;
		max-width: 46ch;
		color: var(--muted);
		font-size: 17px;
	}

	.install {
		display: flex;
		align-items: center;
		gap: 14px;
		max-width: 580px;
		margin: 28px 0 0;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--panel);
		overflow-x: auto;
	}

	.install .file {
		flex: none;
		font-family: var(--mono);
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.12em;
		color: var(--faint);
		padding-right: 12px;
		border-right: 1px solid var(--line-strong);
	}

	.install code {
		font-size: 13px;
		white-space: nowrap;
		background: none;
		border: 0;
		padding: 0;
		color: var(--text);
	}

	/* `.copy` is authored for a code block, where it pins to the top-right of a
	   tall `pre`. This row is one line, so it centres instead. */
	.install :global(.copy) {
		top: 50%;
		right: 8px;
		transform: translateY(-50%);
	}

	.facts {
		display: flex;
		gap: 28px;
		flex-wrap: wrap;
		margin: 14px 0 0;
		color: var(--faint);
		font-size: 14px;
	}

	.facts strong {
		color: var(--muted);
		font-weight: 500;
	}

	.call {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1fr);
		align-items: center;
		gap: 48px;
		max-width: 1180px;
		margin: 0 auto;
		padding: 56px 40px;
	}

	.call h2 {
		font-size: 26px;
		font-weight: 600;
		margin: 0 0 14px;
		letter-spacing: -0.02em;
		max-width: 18ch;
	}

	.call .prose p {
		margin: 0;
		color: var(--muted);
		max-width: 44ch;
	}

	.stage {
		max-width: 1180px;
		margin: 0 auto;
		padding: 24px 40px 8px;
	}

	.window {
		border: 1px solid var(--line);
		border-radius: 12px;
		overflow: hidden;
		background: var(--panel);
		box-shadow: 0 40px 90px -30px rgb(0 0 0 / 0.9);
	}

	.titlebar {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 36px;
		padding: 0 14px;
		border-bottom: 1px solid var(--line);
		background: var(--panel);
	}

	.light {
		width: 12px;
		height: 12px;
		border-radius: 50%;
	}

	.close {
		background: #ff5f57;
	}

	.minimise {
		background: #febc2e;
	}

	.zoom {
		background: #28c840;
	}

	.title {
		margin-left: auto;
		margin-right: auto;
		padding-right: 48px;
		color: var(--faint);
		font-family: var(--mono);
		font-size: 13px;
	}

	/* The window already draws the outline, so the transcript inside it drops its
	   own and keeps only the padding. */
	.window :global(pre) {
		margin: 0;
		border: 0;
		border-radius: 0;
		padding: 18px 20px;
	}

	.note {
		margin: 20px 0 0;
		color: var(--faint);
		font-size: 13px;
		text-align: center;
	}

	/* Spacing lives out here, not on the grid: the grid's background is the rule
	   colour showing through its own gaps, so padding on it would paint a thick
	   border instead of leaving room around the box. */
	.pillars {
		max-width: 1180px;
		margin: 0 auto;
		padding: 72px 40px 0;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 1px;
		background: var(--line);
		border: 1px solid var(--line);
		border-radius: 12px;
		overflow: hidden;
	}

	/* The rule between panels is the container showing through a 1px gap, so
	   there is one line between neighbours rather than two borders meeting. */
	.pillars article {
		background: var(--bg);
		padding: 36px 32px;
	}

	.pillars h2 {
		font-size: 16px;
		font-weight: 500;
		margin: 0 0 10px;
		letter-spacing: -0.01em;
	}

	.pillars p {
		margin: 0;
		color: var(--muted);
		font-size: 15px;
		line-height: 1.65;
	}

	.cli {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 40px;
		max-width: 1180px;
		margin: 0 auto;
		padding: 72px 40px 112px;
	}

	.cli .column {
		min-width: 0;
	}

	.cli h2 {
		font-size: 16px;
		font-weight: 500;
		margin: 0 0 10px;
	}

	.cli p {
		margin: 0 0 18px;
		color: var(--muted);
		font-size: 15px;
		min-height: 3.3em;
	}

	footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		max-width: 1180px;
		margin: 0 auto;
		padding: 28px 40px 72px;
		border-top: 1px solid var(--line);
	}

	footer nav {
		display: flex;
		align-items: center;
		gap: 22px;
	}

	/* Drafting-label register: small, spaced and set in the mono face, so it
	   reads as an annotation on the page rather than more prose. */
	.left a {
		font-family: var(--mono);
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.16em;
	}

	footer a {
		color: var(--faint);
	}

	footer a:hover {
		color: var(--text);
	}

	@media (max-width: 900px) {
		.hero {
			grid-template-columns: 1fr;
			gap: 24px;
			padding: 72px 24px 40px;
		}

		.act {
			min-height: 0;
			justify-content: flex-start;
		}

		.cta {
			justify-content: flex-start;
		}

		.call {
			grid-template-columns: 1fr;
			gap: 24px;
			padding: 40px 24px;
		}
	}

	@media (max-width: 780px) {
		.stage {
			padding: 16px 24px 8px;
		}

		.pillars {
			padding: 56px 24px 0;
		}

		.grid {
			grid-template-columns: 1fr;
		}

		.cli {
			grid-template-columns: 1fr;
			gap: 32px;
			padding: 56px 24px 88px;
		}

		.cli p {
			min-height: 0;
		}

		footer {
			flex-direction: column;
			align-items: flex-start;
			padding: 24px 24px 56px;
		}
	}
</style>
