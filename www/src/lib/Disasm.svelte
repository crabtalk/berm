<script>
	// A disassembly for the hero to stand on. Seeded rather than random: the page
	// is prerendered, so an unseeded roll would print one listing into the HTML
	// and a different one on hydration, and the block would visibly change on
	// load.
	//
	// Purely decorative: behind content, no pointer events, out of the
	// accessibility tree. It is RV64 because that is what a harness is, but no
	// encoding here decodes to the mnemonic beside it.
	let { rows = 20, seed = 'berm' } = $props();

	function seedOf(value) {
		let hash = 2166136261;
		for (let i = 0; i < value.length; i++) {
			hash ^= value.charCodeAt(i);
			hash = Math.imul(hash, 16777619);
		}
		return hash >>> 0;
	}

	function prng(start) {
		let a = start;
		return () => {
			a |= 0;
			a = (a + 0x6d2b79f5) | 0;
			let t = Math.imul(a ^ (a >>> 15), 1 | a);
			t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
			return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
		};
	}

	const REGS = ['ra', 'sp', 't0', 't1', 's0', 's1', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5'];

	const OPS = [
		['addi', 'rri'],
		['addiw', 'rri'],
		['slli', 'rri'],
		['andi', 'rri'],
		['add', 'rrr'],
		['sub', 'rrr'],
		['or', 'rrr'],
		['ld', 'mem'],
		['sd', 'mem'],
		['lw', 'mem'],
		['sw', 'mem'],
		['mv', 'rr'],
		['sext.w', 'rr'],
		['lui', 'ri'],
		['auipc', 'ri'],
		['beq', 'br'],
		['bne', 'br'],
		['jal', 'j'],
		['j', 'j'],
		['ret', 'none']
	];

	const BASE = 0x100a4;

	const listing = (() => {
		const next = prng(seedOf(seed));
		const pick = (list) => list[Math.floor(next() * list.length)];
		const hex = (value, width) => value.toString(16).padStart(width, '0');
		const target = () => `0x${hex(BASE + Math.floor(next() * 400) * 2, 5)}`;

		const operands = (kind) => {
			switch (kind) {
				case 'rri':
					return `${pick(REGS)},${pick(REGS)},${Math.floor(next() * 128) - 64}`;
				case 'rrr':
					return `${pick(REGS)},${pick(REGS)},${pick(REGS)}`;
				case 'mem':
					return `${pick(REGS)},${Math.floor(next() * 32) * 8}(sp)`;
				case 'rr':
					return `${pick(REGS)},${pick(REGS)}`;
				case 'ri':
					return `${pick(REGS)},0x${hex(Math.floor(next() * 0x1000), 3)}`;
				case 'br':
					return `${pick(REGS)},${pick(REGS)},${target()}`;
				case 'j':
					return target();
				default:
					return '';
			}
		};

		let pc = BASE;
		return Array.from({ length: rows }, () => {
			const [op, kind] = pick(OPS);
			const compressed = next() < 0.55;
			const encoding = hex(
				Math.floor(next() * (compressed ? 0x10000 : 0x100000000)),
				compressed ? 4 : 8
			);
			const line = `${hex(pc, 8)}  ${encoding.padEnd(10)}${op.padEnd(8)}${operands(kind)}`;
			pc += compressed ? 2 : 4;
			return line;
		}).join('\n');
	})();
</script>

<pre class="disasm" aria-hidden="true">{listing}</pre>

<style>
	.disasm {
		position: absolute;
		inset: 0;
		margin: 0;
		padding: 0;
		border: 0;
		background: none;
		overflow: hidden;
		pointer-events: none;
		user-select: none;
		font-family: var(--mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--line);
		/* Fades into the page instead of ending on a hard rectangle, and fades
		   towards the right, where the buttons are: a listing has columns and
		   words, so at full strength it reads as something to be read rather than
		   as the texture it is. */
		-webkit-mask-image: radial-gradient(110% 100% at 12% 45%, #000 0%, transparent 78%);
		mask-image: radial-gradient(110% 100% at 12% 45%, #000 0%, transparent 78%);
	}
</style>
