#include <clap/clap.h>

extern "C"
{
    // FIXME: im too stupid to understand why this is needed or how to force reexport `clap_entry` from a static library
    clap_version_t __force_clap_entry()
    {
        return clap_entry.clap_version;
    }
}