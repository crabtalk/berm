/* Guest fault recovery.
 *
 * A guest load or store that misses the committed region hits a guard page and
 * raises a signal inside JIT-compiled code. There is no way to return from
 * that normally, so we longjmp back to the frame that entered the guest.
 *
 * `_setjmp`/`_longjmp` are used rather than `setjmp`/`longjmp` because they do
 * not save or restore the signal mask -- on glibc the latter costs a syscall
 * per guest entry. Skipping the mask is safe because the handler is installed
 * with SA_NODEFER, so the signal is never blocked in the first place.
 */

#include <setjmp.h>
#include <signal.h>
#include <stdbool.h>
#include <string.h>

typedef bool (*rvtime_body)(void *payload);

/* Run `body` under fault protection.
 *
 * Returns whatever `body` returned, or false if a guest fault unwound out of
 * it. `slot` is the caller's thread-local landing-pad pointer; the previous
 * value is saved and restored so nested entries work.
 */
bool rvtime_protect(rvtime_body body, void *payload, void **slot)
{
    jmp_buf buf;
    void *saved = *slot;

    if (_setjmp(buf) != 0)
    {
        *slot = saved;
        return false;
    }

    *slot = (void *)&buf;
    bool ok = body(payload);
    *slot = saved;
    return ok;
}

/* Unwind to the innermost `rvtime_protect`. Never returns. */
void rvtime_unwind(void *landing_pad)
{
    _longjmp(*(jmp_buf *)landing_pad, 1);
}

/* Install `handler` for the signals a guard-page hit can raise.
 *
 * Linux reports these as SIGSEGV; macOS on arm64 reports SIGBUS. Both must be
 * handled or the fault escapes as a process abort.
 */
int rvtime_install(void (*handler)(int, siginfo_t *, void *))
{
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);

    sa.sa_sigaction = handler;
    sa.sa_flags = SA_SIGINFO | SA_NODEFER | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);

    if (sigaction(SIGSEGV, &sa, NULL) != 0)
    {
        return -1;
    }
    if (sigaction(SIGBUS, &sa, NULL) != 0)
    {
        return -2;
    }

    return 0;
}
