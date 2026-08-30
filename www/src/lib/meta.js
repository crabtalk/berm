export const repo = 'https://github.com/crabtalk/berm';

export const repoApi = repo.replace('https://github.com/', 'https://api.github.com/repos/');

export const author = 'https://x.com/tianyi_gc';

/** Search only — no page repeats it. */
export const description =
	'The operating system for agent harnesses. berm pins one statically linked RV64 ELF by hash, compiles it once, and runs it as a process under a syscall table you can read before it runs.';
