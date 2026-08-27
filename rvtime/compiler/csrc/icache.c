// Freshly written instructions are data until the instruction cache is told
// otherwise. On x86_64 the caches are coherent and the builtin expands to
// nothing; on arm64 it issues the cache maintenance the architecture requires.
void rvtime_flush_icache(char *start, unsigned long len)
{
    __builtin___clear_cache(start, start + len);
}
